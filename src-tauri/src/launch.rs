//! How a stored session is started: the program, its arguments, and a password to type at the first prompt.
//!
//! SuperXPuTTY's `PuttyStartInfo` / `PlinkCommandLine` for the Mac: SSH goes through OpenSSH (`ssh -tt`), with the
//! session's folder defaults applied first (`FolderSettings.WithDefaults`) and PuTTY-style extra arguments translated
//! (docs/PLAN.md §2, SSH and other connections). cmd and WSL sessions open the user's login shell instead.
//!
//! A session's PuTTY profile name is looked up in the user's own profiles (`profiles.rs`): the session's own fields
//! win, then its folders' defaults, then the profile, then "Default Settings" (docs/PLAN.md §2 Profiles).
//!
//! A password (the session's `-pw`, else its profile's) is never put on the command line or in the environment: the
//! pseudo terminal types it once, at the first password prompt (`pty.rs`). That also leaves host-key questions in the
//! terminal for the user to answer.

use std::collections::{BTreeMap, HashMap};

use portable_pty::CommandBuilder;
use serde::Serialize;

use crate::profiles::{self, Profile, Profiles};
use crate::variables;
use crate::sessions::{Sessions, Store, StoredSession};

/// How the terminal page draws this tab, from the session's profile (SuperXPuTTY took these from the PuTTY profile)
#[derive(Clone, Debug, Serialize)]
pub struct Look {
    /// "block", "underline" or "bar"
    pub cursor: String,
    pub blink: bool,
    /// "visual", "sound" or "none"
    pub bell: String,
    /// The session's highlighting rules (a rule set's id, "auto" or "none"), else its folder's; "" for the default
    pub rules: String,
    /// The profile's colours, "#rrggbb", else "Default Settings'"; "" for Settings > Terminal's or the theme's
    pub background: String,
    pub foreground: String,
    pub selection: String,
}

/// A session ready to start
pub struct Launch {
    pub command: CommandBuilder,
    /// Typed once at the first "password:" prompt
    pub password: Option<String>,
    /// The tab should behave as a local shell (no reconnect, Ctrl+C as a signal)
    pub local: bool,
    /// Things the user should know, shown in the terminal before it starts
    pub notices: Vec<String>,
    /// The cursor and bell for the page
    pub look: Look,
    /// Telnet, Raw and Serial run in the core instead of a program (`command` is then unused)
    pub stream: Option<crate::streams::Target>,
    /// An SSH session's login as worked out (session, folders, profile), for the apps it opens in: FileZilla
    pub login: Option<Login>,
}

/// Where and as whom an SSH session logs in
#[derive(Clone, Debug, Default)]
pub struct Login {
    pub host: String,
    /// 0 for SSH's own (22)
    pub port: u16,
    pub username: String,
    /// The profile's key file (its path, ~ expanded), "" when it has none
    pub key_file: String,
}

/// Algorithms older devices offer that OpenSSH no longer accepts by default, but PuTTY does (some below its "warn
/// below here" line): ssh's option, `ssh -Q` query and the algorithms. Added for each connection only, as the "Allow
/// older algorithms" profile option (docs/PLAN.md §2 Profiles), on in "Default Settings". Windows' OpenSSH also
/// leaves hmac-sha1 out of its defaults, which an IOS switch offering only hmac-sha1 and hmac-md5 needs.
const OLDER_ALGORITHMS: [(&str, &str, &[&str]); 5] = [
    ("KexAlgorithms", "kex", &["diffie-hellman-group14-sha1", "diffie-hellman-group-exchange-sha1", "diffie-hellman-group1-sha1"]),
    ("HostKeyAlgorithms", "key", &["ssh-rsa", "ssh-dss"]),
    ("PubkeyAcceptedAlgorithms", "key", &["ssh-rsa", "ssh-dss"]),
    ("Ciphers", "cipher", &["aes256-cbc", "aes192-cbc", "aes128-cbc", "3des-cbc"]),
    ("MACs", "mac", &["hmac-sha1", "hmac-sha1-96", "hmac-md5", "hmac-md5-96"]),
];

/// ssh's options for [`OLDER_ALGORITHMS`], with only the algorithms this computer's ssh has (`ssh -Q kex`, ...): ssh
/// refuses to start at all when an option names one it doesn't know, and newer builds drop some (DSA). Asked once
/// per run; when ssh can't be asked, the ones every OpenSSH 9 and 10 still has.
fn older_algorithm_args() -> Vec<String> {
    static ARGS: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    ARGS.get_or_init(|| {
        let mut args = Vec::new();
        for (option, query, wanted) in OLDER_ALGORITHMS {
            let known = ssh_query(query);
            let have: Vec<&str> = match &known {
                Some(list) => wanted.iter().copied().filter(|w| list.iter().any(|k| k == w)).collect(),
                // the safe ones: in every OpenSSH 9 and 10 build
                None => wanted.iter().copied().filter(|w| !w.contains("dss") && !w.contains("group1-") && !w.contains("3des") && !w.contains("md5")).collect(),
            };
            if !have.is_empty() {
                args.extend(["-o".to_string(), format!("{option}=+{}", have.join(","))]);
            }
        }
        args
    })
    .clone()
}

/// The algorithms of one kind this computer's ssh has (`ssh -Q kex`, `key`, `cipher`, `mac`); None if it can't say
pub fn ssh_query(kind: &str) -> Option<Vec<String>> {
    let mut command = std::process::Command::new(openssh("ssh"));
    command.args(["-Q", kind]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let output = command.output().ok().filter(|o| o.status.success())?;
    let list: Vec<String> = String::from_utf8_lossy(&output.stdout).lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    (!list.is_empty()).then_some(list)
}

/// An in-memory attribute only Quick connect sets: the password typed in its toolbar, never written to a file
pub const QUICK_PASSWORD: &str = "__QuickConnectPassword";

/// What Quick connect gives: SuperXPuTTY's toolbar fields
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QuickConnect {
    pub protocol: String,
    /// "host", "host:port", "user@host:port" or "ssh://host:port", as SuperXPuTTY's HostConnectionString (plus user@)
    pub host: String,
    pub username: String,
    pub password: String,
    pub profile: String,
}

impl QuickConnect {
    /// The one-off session Quick connect opens (never saved), as SuperXPuTTY's TryConnectFromToolbar builds it
    pub fn session(&self) -> Result<StoredSession, String> {
        let mut text = self.host.trim().trim_end_matches('/').to_string();
        let mut protocol = self.protocol.clone();
        if let Some((scheme, rest)) = text.split_once("://") {
            protocol = scheme.to_ascii_uppercase();
            text = rest.to_string();
        }
        let mut username = self.username.trim().to_string();
        if let Some((user, rest)) = text.split_once('@') {
            if username.is_empty() {
                username = user.to_string();
            }
            text = rest.to_string();
        }
        let (host, port) = match text.rsplit_once(':') {
            Some((host, port)) if port.parse::<u16>().is_ok() => (host.to_string(), port.parse().unwrap_or(0)),
            _ => (text.clone(), 0),
        };
        let local = ["WINCMD", "WSL", "PS"].contains(&protocol.to_ascii_uppercase().as_str());
        if host.is_empty() && !local {
            return Err("Type a host to connect to.".into());
        }
        let mut session = StoredSession::new(&format!("QuickConnect/{host}"));
        session.set("Proto", &protocol);
        session.set("Host", &host);
        session.set("Port", &port.to_string());
        session.set("Username", &username);
        session.set("PuttySession", if self.profile.trim().is_empty() { profiles::DEFAULT_PROFILE } else { self.profile.trim() });
        session.attributes.push((QUICK_PASSWORD.into(), self.password.clone()));
        Ok(session)
    }
}

/// The computer the app runs on, as messages name it: "this Mac", or "this PC" on Windows
pub const THIS_COMPUTER: &str = if cfg!(target_os = "macos") { "this Mac" } else { "this PC" };

/// Build the start of a stored session, or say why it can't start on this computer yet
pub fn launch(
    sessions: &Sessions,
    profiles: &Profiles,
    values: &HashMap<String, String>,
    serial_devices: &BTreeMap<String, String>,
    store: Store,
    session: &StoredSession,
) -> Result<Launch, String> {
    match session.get("Proto").to_ascii_uppercase().as_str() {
        "SSH" | "" => ssh(sessions, profiles, values, store, session),
        proto @ ("TELNET" | "RAW" | "SERIAL") => stream(sessions, profiles, values, serial_devices, store, session, proto),
        proto @ ("PS" | "WINCMD" | "WSL") => windows_shell(profiles, proto, session.get("Proto")),
        other => Err(format!("{other} sessions can't be opened yet.")),
    }
}

/// cmd, PowerShell and WSL sessions, as SuperXPuTTY starts them: on Windows the system's own cmd.exe, Windows
/// PowerShell (powershell.exe) and wsl.exe, in the user's folder. On the Mac, PowerShell sessions run pwsh when it's
/// installed (brew install powershell), and cmd and WSL ones open the user's shell, saying so.
/// `proto` is the protocol in capitals, `named` as the session writes it.
fn windows_shell(profiles: &Profiles, proto: &str, #[cfg_attr(windows, allow(unused_variables))] named: &str) -> Result<Launch, String> {
    let mut launch = local_shell(profiles);
    #[cfg(windows)]
    {
        let system = std::path::PathBuf::from(std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into())).join("System32");
        let program = match proto {
            "PS" => system.join("WindowsPowerShell").join("v1.0").join("powershell.exe"),
            "WSL" => system.join("wsl.exe"),
            _ => system.join("cmd.exe"),
        };
        if !program.exists() {
            return Err(match proto {
                "WSL" => "WSL sessions need the Windows Subsystem for Linux, which isn't installed on this PC (wsl --install).".into(),
                _ => format!("{} isn't on this PC.", program.display()),
            });
        }
        launch.command = CommandBuilder::new(program);
    }
    #[cfg(not(windows))]
    {
        if proto == "PS" {
            let pwsh = ["/opt/homebrew/bin/pwsh", "/usr/local/bin/pwsh", "/usr/local/microsoft/powershell/7/pwsh"]
                .into_iter()
                .find(|p| std::path::Path::new(p).exists())
                .ok_or("PowerShell sessions need pwsh, which isn't installed on this Mac (brew install powershell).")?;
            launch.command = CommandBuilder::new(pwsh);
        } else {
            launch.notices.push(format!("{named} session opened as a Local Shell on this Mac."));
        }
    }
    Ok(launch)
}

/// What an error from `launch` starts with when a serial session's COM name has no device yet: the window then asks
/// which `/dev/cu.*` it is, remembers it, and tries again (SERIAL_DEVICE:COM6)
pub const SERIAL_DEVICE_NEEDED: &str = "SERIAL_DEVICE:";

/// What a launch's error starts with when its profile's key must be fetched first: then the profile's name, a line
/// break, and why (no key yet, or how old it is); the tab runs the fetch (pty_open's KeyFetch) and opens it again
pub const KEY_FETCH_NEEDED: &str = "KEY_FETCH:";

/// A duration in minutes as "37 min" or "4 h 12 min"
fn minutes_text(minutes: u64) -> String {
    match (minutes / 60, minutes % 60) {
        (0, m) => format!("{m} min"),
        (h, 0) => format!("{h} h"),
        (h, m) => format!("{h} h {m} min"),
    }
}

/// How old a key file is, in seconds, by its modified time; None when there's none
pub fn key_age(path: &str) -> Option<u64> {
    let modified = std::fs::metadata(expand_home(path)).ok()?.modified().ok()?;
    Some(std::time::SystemTime::now().duration_since(modified).map(|d| d.as_secs()).unwrap_or(0))
}

/// Why a profile's key needs fetching, or None when it's there and young enough (`lifetime` minutes, 0 for any age)
fn key_fetch_reason(key_file: &str, lifetime: u32) -> Option<String> {
    match key_age(key_file) {
        None => Some(format!("There's no key yet ({key_file})")),
        Some(age) if lifetime > 0 && age > u64::from(lifetime) * 60 => Some(format!(
            "The key is {} old (good for {})",
            minutes_text(age / 60),
            minutes_text(u64::from(lifetime))
        )),
        Some(_) => None,
    }
}

/// Let only the user read a fetched key (ssh refuses one others can): mode 600 on the Mac, the user's own access on
/// Windows, as .ppk copies are kept
fn private_key(path: &std::path::Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    }
    crate::ppk::owner_only(path)
}

/// The program that fetches a profile's key, in the session's tab (KEY_FETCH_NEEDED): its command through the user's
/// shell, its rc file read so a shell function (`pam`, say) works, or PowerShell on Windows
pub fn key_fetch(profiles: &Profiles, values: &HashMap<String, String>, profile_name: &str) -> Result<Launch, String> {
    let default = profiles.default_profile();
    let profile = if profiles::is_default(profile_name) { default } else { profiles.get(profile_name).unwrap_or(default) };
    let key_file = expand_home(&profile.key_file);
    if profile.key_fetch.trim().is_empty() || profile.key_file.trim().is_empty() {
        return Err(format!("Profile \"{}\" has no key fetch: set its Fetch command and Key file in Settings > Profiles.", profile.name));
    }
    let mut with_file = values.clone();
    with_file.insert("key_file".into(), key_file.clone());
    let missing = variables::missing(&profile.key_fetch, &with_file);
    if !missing.is_empty() {
        return Err(format!(
            "The key fetch uses placeholders that have no value yet: {}. Define them in Tools > Environment Variables.",
            missing.join(", ")
        ));
    }
    let text = variables::substitute(profile.key_fetch.trim(), &with_file);
    if let Some(folder) = std::path::Path::new(&key_file).parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    #[cfg(windows)]
    let command = {
        let mut command = CommandBuilder::new("powershell.exe");
        command.args(["-NoLogo", "-Command", &text]);
        command
    };
    #[cfg(not(windows))]
    let command = {
        let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty()).unwrap_or_else(|| "/bin/zsh".into());
        let mut command = CommandBuilder::new(shell);
        // -i reads the rc file (~/.zshrc), where a function like pam lives
        command.args(["-i", "-c", &text]);
        command
    };
    let mut command = command;
    if let Some(home) = dirs::home_dir() {
        command.cwd(home);
    }
    Ok(Launch {
        command,
        password: None,
        local: true,
        notices: vec![format!("Fetching a new key for profile \"{}\" into {key_file}.", profile.name)],
        look: look(profile, default),
        stream: None,
        login: None,
    })
}

/// The built-in placeholder for the session's own host (its target), for a username that names it; `host_port` is
/// its port (docs/CYBERARK.md)
pub const HOST_ADDRESS: &str = "host_address";

/// What a launch's error starts with when an SSH session has no username and its profile asks for one; the tab asks
/// ("login as:") and opens it again with that username (pty_open's login_as), followed by the host
pub const USERNAME_NEEDED: &str = "USERNAME:";

/// A Telnet, Raw or Serial session, as plink's -telnet, -raw and -serial: folder defaults, the profile's look, the
/// port (Telnet's default 23), and for serial the device (a COM name asked once) and -sercfg's line settings
fn stream(
    sessions: &Sessions,
    profiles: &Profiles,
    values: &HashMap<String, String>,
    serial_devices: &BTreeMap<String, String>,
    store: Store,
    session: &StoredSession,
    proto: &str,
) -> Result<Launch, String> {
    let mut port = session.port();
    let mut profile_name = session.get("PuttySession").to_string();
    let mut rules = session.get("HighlightRules").to_string();
    let mut jump_host = session.get("JumpHost").to_string();
    // a local override of "none" keeps the folders' and profile's jump host off
    let no_jump_host = session.get(crate::overrides::NO_JUMP_HOST);
    for folder in sessions.along(store, session.folder()) {
        if port == 0 {
            port = folder.port;
        }
        if jump_host.is_empty() && no_jump_host.is_empty() {
            jump_host = folder.jump_host.clone();
        }
        if rules.is_empty() {
            rules = folder.highlight_rules.clone();
        }
        if profiles::is_default(&profile_name) && !folder.putty_profile.is_empty() {
            profile_name = folder.putty_profile.clone();
        }
    }
    let default = profiles.default_profile();
    let profile = if profiles::is_default(&profile_name) { default } else { profiles.get(&profile_name).unwrap_or(default) };
    let host = variables::substitute(session.get("Host").trim(), values);
    if host.is_empty() {
        return Err(format!("The session has no {}.", if proto == "SERIAL" { "serial line" } else { "host" }));
    }
    let mut notices = Vec::new();
    // Telnet and Raw can go through an SSH jump host (ssh -W), as SuperXPuTTY's proxy command does; the bastion must
    // take a key, as there's nowhere to type its password
    let jump_host = variables::substitute(jump_host.trim(), values);
    let via = if jump_host.is_empty() || proto == "SERIAL" {
        None
    } else {
        Some(jump_address(sessions, values, store, session, &jump_host)?)
    };
    let target = match proto {
        "TELNET" => crate::streams::Target::Telnet { host, port: if port == 0 { 23 } else { port }, via },
        "RAW" => {
            if port == 0 {
                return Err("A Raw session needs a port.".into());
            }
            crate::streams::Target::Raw { host, port, via }
        }
        _ => {
            // on Windows the COM name is the port itself; on the Mac a COM name is asked once which /dev/cu.* device
            // it is, and a /dev path is used as it is
            let device = if cfg!(windows) {
                host.to_uppercase()
            } else if host.starts_with("/dev/") {
                host.clone()
            } else {
                serial_devices
                    .iter()
                    .find(|(name, _)| name.eq_ignore_ascii_case(&host))
                    .map(|(_, device)| device.clone())
                    .ok_or_else(|| format!("{SERIAL_DEVICE_NEEDED}{host}"))?
            };
            let mut line = crate::streams::SerialLine::default();
            let words = split_words(&variables::substitute(session.get("ExtraArgs"), values));
            if let Some(i) = words.iter().position(|w| w == "-sercfg") {
                line.apply(words.get(i + 1).map(String::as_str).unwrap_or(""))?;
            }
            if !host.eq_ignore_ascii_case(&device) {
                notices.push(format!("{host} is {device} on {THIS_COMPUTER}."));
            }
            crate::streams::Target::Serial { device, line }
        }
    };
    Ok(Launch {
        command: CommandBuilder::new("true"),
        password: None,
        local: false,
        notices,
        look: Look { rules, ..look(profile, default) },
        stream: Some(target),
        login: None,
    })
}

/// The user's login shell, as for the New Local Shell button; it takes "Default Settings"' cursor and bell
pub fn local_shell(profiles: &Profiles) -> Launch {
    let default = profiles.default_profile();
    Launch {
        command: CommandBuilder::new_default_prog(),
        password: None,
        local: true,
        notices: Vec::new(),
        look: look(default, default),
        stream: None,
        login: None,
    }
}

/// The cursor and bell from a profile, else "Default Settings", else the page's own defaults
fn look(profile: &Profile, default: &Profile) -> Look {
    let pick = |a: &str, b: &str, fallback: &str| {
        [a, b].into_iter().find(|v| !v.is_empty()).unwrap_or(fallback).to_string()
    };
    Look {
        cursor: pick(&profile.cursor, &default.cursor, "block"),
        blink: profile.cursor_blink.or(default.cursor_blink).unwrap_or(true),
        bell: pick(&profile.bell, &default.bell, "visual"),
        rules: String::new(),
        background: colour(&profile.background, &default.background),
        foreground: colour(&profile.foreground, &default.foreground),
        selection: colour(&profile.selection, &default.selection),
    }
}

/// A profile's colour, else "Default Settings'", when it's "#rrggbb"; "" for none
fn colour(profile: &str, default: &str) -> String {
    let good = |c: &str| c.len() == 7 && c.starts_with('#') && c[1..].chars().all(|d| d.is_ascii_hexdigit());
    [profile.trim(), default.trim()].into_iter().find(|c| good(c)).map(|c| c.to_ascii_lowercase()).unwrap_or_default()
}

/// The first value that's set: the profile's, else "Default Settings"'
fn first<'a>(profile: &'a str, default: &'a str) -> &'a str {
    if profile.is_empty() { default } else { profile }
}

/// `ssh -tt` for an SSH session, with its folder defaults and profile applied
fn ssh(
    sessions: &Sessions,
    profiles: &Profiles,
    values: &HashMap<String, String>,
    store: Store,
    session: &StoredSession,
) -> Result<Launch, String> {
    let mut username = session.get("Username").to_string();
    let mut port = session.port();
    let mut jump_host = session.get("JumpHost").to_string();
    // a local override of "none" keeps the folders' and profile's jump host off
    let no_jump_host = session.get(crate::overrides::NO_JUMP_HOST);
    let mut profile_name = session.get("PuttySession").to_string();
    let mut rules = session.get("HighlightRules").to_string();
    for folder in sessions.along(store, session.folder()) {
        if username.is_empty() {
            username = folder.username.clone();
        }
        if port == 0 {
            port = folder.port;
        }
        if jump_host.is_empty() && no_jump_host.is_empty() {
            jump_host = folder.jump_host.clone();
        }
        if rules.is_empty() {
            rules = folder.highlight_rules.clone();
        }
        if profiles::is_default(&profile_name) && !folder.putty_profile.is_empty() {
            profile_name = folder.putty_profile.clone();
        }
    }

    let mut notices = Vec::new();
    let default = profiles.default_profile();
    let profile = if profiles::is_default(&profile_name) {
        default
    } else if let Some(profile) = profiles.get(&profile_name) {
        profile
    } else {
        notices.push(format!(
            "Profile \"{profile_name}\" isn't set up on {THIS_COMPUTER}, so Default Settings is used. Add it in Settings > Profiles."
        ));
        default
    };
    if username.is_empty() {
        username = first(&profile.username, &default.username).to_string();
    }
    if port == 0 {
        port = if profile.port != 0 { profile.port } else { default.port };
    }
    if jump_host.is_empty() && no_jump_host.is_empty() {
        jump_host = first(&profile.jump_host, &default.jump_host).to_string();
    }

    // the built-in placeholders: the session's own target, for a username that names it (a CyberArk gateway's,
    // docs/CYBERARK.md); they win over Environment Variables of the same name, so none can redirect a session
    let mut with_builtins = values.clone();
    with_builtins.insert(HOST_ADDRESS.into(), variables::substitute(session.get("Host").trim(), values));
    with_builtins.insert("host_port".into(), (if port != 0 { port } else { 22 }).to_string());
    let values = &with_builtins;

    // {key} placeholders, after the folder and profile defaults (a folder's "{user}" counts), as SessionVariables
    let raw_extra = session.get("ExtraArgs");
    let mut missing = Vec::new();
    for text in [session.get("Host"), username.as_str(), raw_extra, jump_host.as_str()] {
        for key in variables::missing(text, values) {
            if !missing.contains(&key) {
                missing.push(key);
            }
        }
    }
    if !missing.is_empty() {
        return Err(format!(
            "This session uses placeholders that have no value yet: {}. Define them in Tools > Environment Variables.",
            missing.join(", ")
        ));
    }
    let mut host = variables::substitute(session.get("Host").trim(), values);
    // the profile's Host (docs/CYBERARK.md), at the profile's port: for a session with no host of its own, as a PuTTY
    // saved session's Host Name; and for a username naming the session's host ({host_address}), a gateway's, which
    // takes the target in the username. Any other username connects to the session's host, so a local override's
    // own username (and profile) goes straight there.
    let through_gateway = username.to_ascii_lowercase().contains(&format!("{{{HOST_ADDRESS}}}"));
    let username = variables::substitute(&username, values);
    let jump_host = variables::substitute(&jump_host, values);
    let profile_host = variables::substitute(first(&profile.host, &default.host).trim(), values);
    let profile_port = [profile.port, default.port].into_iter().find(|p| *p != 0);
    if host.is_empty() {
        if through_gateway {
            return Err(format!(
                "This session's username names its host ({{{HOST_ADDRESS}}}), but the session has no host to put there."
            ));
        }
        if profile_host.is_empty() {
            return Err(format!("This session has no host, and profile \"{}\" has none either. Set one in Settings > Profiles.", profile.name));
        }
        notices.push(format!("No host of its own: connecting to {profile_host} (the Host of profile \"{}\").", profile.name));
        host = profile_host;
        port = profile_port.unwrap_or(if port != 0 { port } else { 22 });
    } else if through_gateway {
        if profile_host.is_empty() {
            return Err(format!(
                "This session's username names its host ({{{HOST_ADDRESS}}}), so it connects through a gateway, but profile \"{}\" has no Host. Set one in Settings > Profiles.",
                profile.name
            ));
        }
        notices.push(format!("Connecting to {host} through the gateway {profile_host}."));
        host = profile_host;
        // the session's port is the target's, which travels as {host_port}; the gateway's is the profile's
        port = profile_port.unwrap_or(22);
    }
    let mut login = Login { host: host.clone(), port, username: username.clone(), key_file: String::new() };

    let mut args: Vec<String> = vec!["-tt".into()];
    // the session's own algorithm lists, else its profile's, else Default Settings', each kind on its own
    // (algorithms.rs); a kind set there is that exact list, so the older ones aren't added to it
    let own = crate::algorithms::chosen(&[session.get("SshAlgorithms"), &profile.algorithms, &default.algorithms]);
    if profile.older_algorithms.or(default.older_algorithms).unwrap_or(true) {
        let older = older_algorithm_args();
        for pair in older.chunks(2) {
            let option = pair.get(1).and_then(|o| o.split_once('=')).map(|(name, _)| name).unwrap_or("");
            if !own.iter().any(|(set, _)| *set == option) {
                args.extend(pair.iter().cloned());
            }
        }
    }
    args.extend(crate::algorithms::args(&own));
    let extra = translate_extra_args(&variables::substitute(raw_extra, values));
    notices.extend(extra.notices);
    // no username from the session, its folders, the profile, -l or an ~/.ssh/config Host: ask, when the profile says
    let config_host = first(&profile.ssh_config_host, &default.ssh_config_host);
    if username.is_empty()
        && config_host.is_empty()
        && !extra.args.iter().any(|a| a == "-l")
        && profile.ask_username.or(default.ask_username).unwrap_or(false)
    {
        return Err(format!("{USERNAME_NEEDED}{host}"));
    }
    // Quick connect's password (never stored), else the session's own -pw, else its profile's, else Default Settings'
    let password = Some(session.get(QUICK_PASSWORD).to_string())
        .filter(|p| !p.is_empty())
        .or_else(|| extra.password.clone())
        .or_else(|| profiles::password(&profile.name))
        .or_else(|| if profile.name == default.name { None } else { profiles::password(&default.name) });
    if port != 0 && port != 22 {
        args.extend(["-p".into(), port.to_string()]);
    }
    if !username.is_empty() {
        args.extend(["-l".into(), username]);
    }
    let key_file = first(&profile.key_file, &default.key_file);
    // a key fetched for a short time (the key file's own profile's fetch): missing or too old, it's fetched first
    let key_owner = if profile.key_file.is_empty() { default } else { profile };
    if !key_file.is_empty() && !key_owner.key_fetch.trim().is_empty() {
        if let Some(reason) = key_fetch_reason(key_file, key_owner.key_lifetime) {
            return Err(format!("{KEY_FETCH_NEEDED}{}\n{reason}: fetching a new one.", key_owner.name));
        }
        let _ = private_key(std::path::Path::new(&expand_home(key_file)));
    }
    if !key_file.is_empty() {
        let mut path = expand_home(key_file);
        // OpenSSH can't read PuTTY's .ppk keys: ssh gets an OpenSSH copy of it (ppk.rs), when it has no passphrase
        let usable = if !std::path::Path::new(&path).exists() {
            notices.push(format!("The key file {key_file} isn't there; connecting without it."));
            false
        } else if path.to_ascii_lowercase().ends_with(".ppk") {
            match crate::ppk::openssh_copy(std::path::Path::new(&path)) {
                Ok(copy) => {
                    path = copy.display().to_string();
                    true
                }
                Err(e) => {
                    notices.push(format!("{e}; connecting without it."));
                    false
                }
            }
        } else {
            true
        };
        if usable {
            login.key_file = path.clone();
            args.extend(["-i".into(), path, "-o".into(), "IdentitiesOnly=yes".into()]);
        }
    }
    // Offer SSH keys off, with no key file of its own: straight to the password (each key offered is a failed
    // attempt to a Cisco, which closes after 3); a -i in the extra arguments is still a key to offer
    if login.key_file.is_empty() && !extra.args.iter().any(|a| a == "-i") && !profile.offer_keys.or(default.offer_keys).unwrap_or(true) {
        args.extend(["-o".into(), "PubkeyAuthentication=no".into()]);
    }
    let keepalive = if profile.keepalive != 0 { profile.keepalive } else { default.keepalive };
    if keepalive != 0 {
        args.extend(["-o".into(), format!("ServerAliveInterval={keepalive}")]);
    }
    if password.is_some() {
        // try the password before any key: ssh would otherwise offer every key in ~/.ssh and the agent first, and
        // Cisco closes the connection after 3 failed attempts (ip ssh authentication-retries), before the password
        args.extend(["-o".into(), "PreferredAuthentications=keyboard-interactive,password,publickey".into()]);
    }
    if !jump_host.is_empty() {
        args.extend(["-J".into(), jump_address(sessions, values, store, session, &jump_host)?]);
    }
    args.extend(split_words(first(&profile.ssh_options, &default.ssh_options)));
    args.extend(extra.args);
    if config_host.is_empty() {
        args.push(host);
    } else {
        // the ~/.ssh/config Host gives everything else; the session's own address still wins
        args.extend(["-o".into(), format!("HostName={host}"), config_host.to_string()]);
    }

    let mut command = CommandBuilder::new(openssh("ssh"));
    command.args(&args);
    Ok(Launch { command, password, local: false, notices, look: Look { rules, ..look(profile, default) }, stream: None, login: Some(login) })
}

/// A path with a leading "~/" turned into the home folder
fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).display().to_string(),
        _ => path.to_string(),
    }
}

/// The OpenSSH program to run ("ssh", "scp"): on Windows its own OpenSSH when it's installed, as another ssh.exe on
/// the PATH (MSYS2's, Cygwin's, Git's) may come first and read other files; elsewhere the one on the PATH
pub fn openssh(program: &str) -> std::ffi::OsString {
    #[cfg(windows)]
    if let Some(root) = std::env::var_os("SystemRoot") {
        let own = std::path::Path::new(&root).join("System32").join("OpenSSH").join(format!("{program}.exe"));
        if own.exists() {
            return own.into_os_string();
        }
    }
    program.into()
}

/// A jump host as `ssh -J` takes it, as SuperXPuTTY's JumpHosts.Resolve: a saved session (by its id, in the same
/// store, Saved sessions, or Database sessions' path), with its folders' username and port and its placeholders, or
/// "user@host:port" as it is. Chains aren't followed, and a session can't jump through itself.
fn jump_address(
    sessions: &Sessions,
    values: &HashMap<String, String>,
    store: Store,
    from: &StoredSession,
    jump: &str,
) -> Result<String, String> {
    let jump = jump.trim();
    let database_id = format!("@db/{}", jump.trim_start_matches('/'));
    let found = [(store, jump), (Store::Saved, jump), (Store::Private, jump), (Store::Database, database_id.as_str())]
        .into_iter()
        .find_map(|(s, id)| sessions.find(s, id).map(|session| (s, session)));
    let Some((jump_store, session)) = found else {
        let valid = jump.split_once('@').map(|(user, rest)| !user.is_empty() && !rest.is_empty()).unwrap_or(true)
            && !jump.chars().any(char::is_whitespace);
        return if valid { Ok(jump.to_string()) } else { Err(format!("The jump host \"{jump}\" isn't a session or user@host:port.")) };
    };
    if jump_store == store && session.id() == from.id() {
        return Err("A session can't be its own jump host.".into());
    }
    let mut username = session.get("Username").to_string();
    let mut port = session.port();
    for folder in sessions.along(jump_store, session.folder()) {
        if username.is_empty() {
            username = folder.username.clone();
        }
        if port == 0 {
            port = folder.port;
        }
    }
    let mut address = variables::substitute(session.get("Host").trim(), values);
    let username = variables::substitute(&username, values);
    if !username.is_empty() {
        address = format!("{username}@{address}");
    }
    if port != 0 && port != 22 {
        address = format!("{address}:{port}");
    }
    Ok(address)
}

/// PuTTY-style extra arguments, translated for `ssh`
struct Translated {
    args: Vec<String>,
    password: Option<String>,
    notices: Vec<String>,
}

/// Translate PuTTY/plink arguments into `ssh`'s where it has an equivalent (docs/PLAN.md §2); the rest are named
fn translate_extra_args(extra: &str) -> Translated {
    let mut out = Translated { args: Vec::new(), password: None, notices: Vec::new() };
    let words = split_words(extra);
    let mut skipped = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let word = words[i].as_str();
        let value = words.get(i + 1).cloned();
        match word {
            "-pw" => {
                out.password = value;
                i += 1;
            }
            "-P" => {
                if let Some(v) = value {
                    out.args.extend(["-p".into(), v]);
                }
                i += 1;
            }
            "-l" | "-i" | "-L" | "-R" | "-D" => {
                if let Some(v) = value {
                    if word == "-i" && v.to_ascii_lowercase().ends_with(".ppk") {
                        // ssh can't read a .ppk: its OpenSSH copy (ppk.rs), as for a profile's key file
                        match crate::ppk::openssh_copy(std::path::Path::new(&expand_home(&v))) {
                            Ok(copy) => out.args.extend([word.to_string(), copy.display().to_string()]),
                            Err(e) => out.notices.push(format!("{e}; connecting without it.")),
                        }
                    } else {
                        out.args.extend([word.to_string(), v]);
                    }
                }
                i += 1;
            }
            "-A" | "-X" | "-C" | "-4" | "-6" | "-v" => out.args.push(word.to_string()),
            "-ssh" | "-2" | "-batch" | "-t" => {}
            "-load" => i += 1,
            other => skipped.push(other.to_string()),
        }
        i += 1;
    }
    if !skipped.is_empty() {
        out.notices.push(format!("Not used on {THIS_COMPUTER}: {}", skipped.join(" ")));
    }
    out
}

/// Split a command line into words; double quotes group words, as PuTTY's command line does
fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    words.push(std::mem::take(&mut current));
                    any = false;
                }
            }
            c => {
                current.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sessions::FolderSetting;

    /// A saved session in folder "Lab" with the given attributes
    fn session(attributes: &[(&str, &str)]) -> StoredSession {
        let mut all = vec![("SessionId".to_string(), "Lab/sw1".to_string()), ("Host".to_string(), "10.0.0.1".to_string())];
        all.extend(attributes.iter().map(|(n, v)| (n.to_string(), v.to_string())));
        StoredSession { attributes: all }
    }

    /// The arguments ssh would be started with
    fn args_of(sessions: &Sessions, profiles: &Profiles, session: &StoredSession) -> (Vec<String>, Vec<String>) {
        let launch = launch(sessions, profiles, &HashMap::new(), &BTreeMap::new(), Store::Saved, session).unwrap();
        let args = launch.command.get_argv().iter().skip(1).map(|a| a.to_string_lossy().to_string()).collect();
        (args, launch.notices)
    }

    /// Profiles with "Default Settings" (older algorithms off, to keep the arguments short) and "root"
    fn profiles() -> Profiles {
        Profiles {
            profiles: vec![
                Profile { name: profiles::DEFAULT_PROFILE.into(), older_algorithms: Some(false), keepalive: 30, ..Profile::default() },
                Profile { name: "root".into(), username: "root".into(), port: 2222, ..Profile::default() },
            ],
        }
    }

    /// ssh's arguments for a session with these variables, or why it can't start
    fn launched(profiles: &Profiles, values: &[(&str, &str)], session: &StoredSession) -> Result<Vec<String>, String> {
        let values: HashMap<String, String> = values.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();
        let launch = launch(&Sessions::default(), profiles, &values, &BTreeMap::new(), Store::Saved, session)?;
        Ok(launch.command.get_argv().iter().skip(1).map(|a| a.to_string_lossy().to_string()).collect())
    }

    /// Profiles with a "CyberArk" one: its connection string as the username, and a gateway
    fn cyberark() -> Profiles {
        let mut profiles = profiles();
        profiles.profiles.push(Profile {
            name: "CyberArk".into(),
            username: "{ad_user}#{region}@{target_user}#{vault}@{host_address}#{connector}".into(),
            host: "gw.example.test".into(),
            ..Profile::default()
        });
        profiles
    }

    /// A session at `host` (the helper's own is 10.0.0.1) with these attributes
    fn at(host: &str, attributes: &[(&str, &str)]) -> StoredSession {
        let mut sw = session(attributes);
        sw.set("Host", host);
        sw
    }

    const CYBERARK_VALUES: [(&str, &str); 5] =
        [("ad_user", "me"), ("region", "r1"), ("target_user", "admin"), ("vault", "v1"), ("connector", "c1")];

    #[test]
    fn a_profiles_colours_win_over_default_settings_and_bad_ones_are_left_out() {
        let default = Profile { background: "#101010".into(), foreground: "#EEEEEE".into(), selection: "blue".into(), ..Profile::default() };
        let prod = Profile { background: "#3a0000".into(), ..Profile::default() };
        let look = look(&prod, &default);
        assert_eq!((look.background.as_str(), look.foreground.as_str(), look.selection.as_str()), ("#3a0000", "#eeeeee", ""));
        let look = super::look(&Profile::default(), &Profile::default());
        assert_eq!(look.background, "");
    }

    #[test]
    fn a_username_naming_the_target_connects_through_the_gateway() {
        let sw = at("192.168.0.50", &[("PuttySession", "CyberArk")]);
        let args = launched(&cyberark(), &CYBERARK_VALUES, &sw).unwrap();
        // the target travels in the username (with -l, so its @ and # stay as they are); ssh connects to the gateway
        let user = args.windows(2).find(|w| w[0] == "-l").map(|w| w[1].clone()).unwrap();
        assert_eq!(user, "me#r1@admin#v1@192.168.0.50#c1");
        assert_eq!(args.last().unwrap(), "gw.example.test");
        assert!(!args.contains(&"192.168.0.50".to_string()), "{args:?}");

        // the profile's port is the gateway's; the session's (the target's) isn't used for it
        let mut profiles = cyberark();
        profiles.profiles[2].port = 2222;
        let args = launched(&profiles, &CYBERARK_VALUES, &at("10.0.0.9", &[("Port", "2200"), ("PuttySession", "CyberArk")])).unwrap();
        assert!(args.windows(2).any(|w| w == ["-p", "2222"]), "{args:?}");
    }

    #[test]
    fn host_address_is_built_in_and_wins_over_a_variable() {
        // a variable of the same name can't redirect the session: {host_address} is always its own host
        let mut values = CYBERARK_VALUES.to_vec();
        values.push(("host_address", "evil.example.test"));
        let args = launched(&cyberark(), &values, &at("10.0.0.7", &[("PuttySession", "CyberArk")])).unwrap();
        assert!(args.iter().any(|a| a.contains("@10.0.0.7#")), "{args:?}");
    }

    #[test]
    fn an_own_username_connects_to_the_host_and_no_gateway_refuses() {
        // a local override's username (and the profile kept): no {host_address}, so straight to the host
        let own = at("10.0.0.7", &[("PuttySession", "CyberArk"), ("Username", "me")]);
        let args = launched(&cyberark(), &CYBERARK_VALUES, &own).unwrap();
        assert_eq!(args.last().unwrap(), "10.0.0.7");

        // naming the target with no gateway to go through: it says so rather than sending that username to the host
        let mut profiles = cyberark();
        profiles.profiles[2].host.clear();
        let error = launched(&profiles, &CYBERARK_VALUES, &session(&[("PuttySession", "CyberArk")])).unwrap_err();
        assert!(error.contains("has no Host"), "{error}");
    }

    /// A profile "Vault" with a key fetch for `key` (a path), good for `lifetime` minutes
    fn fetching(key: &std::path::Path, lifetime: u32) -> Profiles {
        let mut profiles = profiles();
        profiles.profiles.push(Profile {
            name: "Vault".into(),
            key_file: key.display().to_string(),
            key_fetch: "get-key --to {key_file} --as {who}".into(),
            key_lifetime: lifetime,
            ..Profile::default()
        });
        profiles
    }

    #[test]
    fn a_missing_or_old_key_is_fetched_first() {
        let folder = std::env::temp_dir().join(format!("sxt-keyfetch-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let key = folder.join("key");
        let sw = at("10.0.0.7", &[("PuttySession", "Vault"), ("Username", "me")]);

        // no key yet: the tab is told to fetch, with the profile and why
        let error = launched(&fetching(&key, 240), &[], &sw).unwrap_err();
        assert!(error.starts_with(&format!("{KEY_FETCH_NEEDED}Vault\n")), "{error}");
        assert!(error.contains("There's no key yet"), "{error}");

        // a fresh key: connects with it, made the user's own
        std::fs::write(&key, "key").unwrap();
        let args = launched(&fetching(&key, 240), &[], &sw).unwrap();
        assert!(args.contains(&key.display().to_string()), "{args:?}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&key).unwrap().permissions().mode() & 0o777, 0o600);
        }

        // older than its lifetime: fetched again, saying how old
        let old = std::time::SystemTime::now() - std::time::Duration::from_secs(5 * 3600);
        std::fs::File::options().write(true).open(&key).unwrap().set_modified(old).unwrap();
        let error = launched(&fetching(&key, 240), &[], &sw).unwrap_err();
        assert!(error.contains("The key is 5 h old (good for 4 h)"), "{error}");
        // a lifetime of 0: any age will do
        assert!(launched(&fetching(&key, 0), &[], &sw).is_ok());
        // and a profile with no fetch command is as before: the key file is just used
        let mut no_fetch = fetching(&key, 240);
        no_fetch.profiles[2].key_fetch.clear();
        assert!(launched(&no_fetch, &[], &sw).is_ok());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn the_fetch_runs_its_command_with_the_key_file_filled_in() {
        let key = std::env::temp_dir().join(format!("sxt-keyfetch-cmd-{}", std::process::id())).join("key");
        let values: HashMap<String, String> = [("who".to_string(), "me@example.test".to_string())].into();
        let launch = key_fetch(&fetching(&key, 240), &values, "Vault").unwrap();
        let argv: Vec<String> = launch.command.get_argv().iter().map(|a| a.to_string_lossy().to_string()).collect();
        assert!(argv.last().unwrap().contains(&format!("--to {} --as me@example.test", key.display())), "{argv:?}");
        assert!(launch.password.is_none());
        // an undefined placeholder says so instead of running
        let error = key_fetch(&fetching(&key, 240), &HashMap::new(), "Vault").err().unwrap();
        assert!(error.contains("{who}"), "{error}");
        let _ = std::fs::remove_dir_all(key.parent().unwrap());
    }

    #[test]
    fn a_session_with_no_host_uses_its_profiles() {
        // as a PuTTY saved session's Host Name: several profiles, each its own username, one address
        let mut profiles = cyberark();
        profiles.profiles[1].host = "jump.example.test".into();
        let no_host = at("", &[("PuttySession", "root")]);
        let args = launched(&profiles, &[], &no_host).unwrap();
        assert_eq!(args.last().unwrap(), "jump.example.test");
        // the profile's port (2222 for root) wins over the session's
        assert!(args.windows(2).any(|w| w == ["-p", "2222"]), "{args:?}");

        // no host anywhere: says so, naming the profile
        let error = launched(&profiles, &[], &at("", &[])).unwrap_err();
        assert!(error.contains("has none either"), "{error}");
        // a username naming a host the session doesn't have: nothing to put there
        let error = launched(&cyberark(), &CYBERARK_VALUES, &at("", &[("PuttySession", "CyberArk")])).unwrap_err();
        assert!(error.contains("no host to put there"), "{error}");
    }

    #[test]
    fn offer_ssh_keys_off_goes_straight_to_the_password() {
        let mut profiles = profiles();
        let sw = session(&[("Username", "admin")]);
        let (args, _) = args_of(&Sessions::default(), &profiles, &sw);
        assert!(!args.contains(&"PubkeyAuthentication=no".to_string()), "{args:?}");

        // off in Default Settings: no keys offered, so a Cisco's 3 attempts aren't spent on them
        profiles.profiles[0].offer_keys = Some(false);
        let (args, _) = args_of(&Sessions::default(), &profiles, &sw);
        assert!(args.contains(&"PubkeyAuthentication=no".to_string()), "{args:?}");

        // a profile of its own can still offer them, and a key file of the session's own is always offered
        profiles.profiles[1].offer_keys = Some(true);
        let (args, _) = args_of(&Sessions::default(), &profiles, &session(&[("PuttySession", "root")]));
        assert!(!args.contains(&"PubkeyAuthentication=no".to_string()), "{args:?}");
        let with_key = session(&[("Username", "admin"), ("ExtraArgs", "-i /tmp/key")]);
        let (args, _) = args_of(&Sessions::default(), &profiles, &with_key);
        assert!(!args.contains(&"PubkeyAuthentication=no".to_string()), "{args:?}");
    }

    #[test]
    fn ask_for_the_username_only_when_there_is_none() {
        let mut profiles = profiles();
        let no_user = session(&[]);
        // off (the default): ssh uses this computer's user name, as before
        assert!(launch(&Sessions::default(), &profiles, &HashMap::new(), &BTreeMap::new(), Store::Saved, &no_user).is_ok());

        profiles.profiles[0].ask_username = Some(true);
        let error = launch(&Sessions::default(), &profiles, &HashMap::new(), &BTreeMap::new(), Store::Saved, &no_user).err().unwrap();
        assert_eq!(error, format!("{USERNAME_NEEDED}10.0.0.1"));
        // a username from the session, the profile or -l needs no asking
        for sw in [session(&[("Username", "admin")]), session(&[("PuttySession", "root")]), session(&[("ExtraArgs", "-l admin")])] {
            assert!(launch(&Sessions::default(), &profiles, &HashMap::new(), &BTreeMap::new(), Store::Saved, &sw).is_ok());
        }
    }

    #[test]
    fn algorithm_lists_come_from_the_session_then_its_profile() {
        let mut profiles = profiles();
        profiles.profiles[1].algorithms = "kex=diffie-hellman-group14-sha1;mac=hmac-sha2-256".into();
        let mut session = StoredSession::new("sw");
        session.set("Host", "sw1");
        session.set("PuttySession", "root");
        session.set("SshAlgorithms", "mac=hmac-sha1,hmac-md5");
        let (args, _) = args_of(&Sessions::default(), &profiles, &session);
        let options: Vec<&str> = args.windows(2).filter(|w| w[0] == "-o").map(|w| w[1].as_str()).collect();
        assert!(options.contains(&"KexAlgorithms=diffie-hellman-group14-sha1"), "{options:?}");
        assert!(options.contains(&"MACs=hmac-sha1,hmac-md5"), "{options:?}");
        assert!(!options.iter().any(|o| o.starts_with("MACs=hmac-sha2")), "{options:?}");

        // with the older algorithms allowed, a kind with its own list isn't added to; the others still are
        profiles.profiles[0].older_algorithms = Some(true);
        let (args, _) = args_of(&Sessions::default(), &profiles, &session);
        let options: Vec<&str> = args.windows(2).filter(|w| w[0] == "-o").map(|w| w[1].as_str()).collect();
        assert!(!options.iter().any(|o| o.starts_with("MACs=+") || o.starts_with("KexAlgorithms=+")), "{options:?}");
    }

    #[test]
    fn the_profile_fills_what_the_session_and_folder_leave_empty() {
        let mut sessions = Sessions::default();
        let folder = FolderSetting { path: "Lab".into(), port: 2200, ..FolderSetting::default() };
        sessions.folders.entry(Store::Saved).or_default().insert("lab".into(), folder);
        let (args, notices) = args_of(&sessions, &profiles(), &session(&[("PuttySession", "root")]));
        // the folder's port beats the profile's; the profile gives the username; Default Settings the keep-alive
        assert_eq!(args, ["-tt", "-p", "2200", "-l", "root", "-o", "ServerAliveInterval=30", "10.0.0.1"]);
        assert!(notices.is_empty());
    }

    #[test]
    fn a_missing_profile_falls_back_to_default_settings_and_says_so() {
        let (args, notices) = args_of(&Sessions::default(), &profiles(), &session(&[("PuttySession", "netops")]));
        assert_eq!(args, ["-tt", "-o", "ServerAliveInterval=30", "10.0.0.1"]);
        assert!(notices[0].contains(&format!("\"netops\" isn't set up on {THIS_COMPUTER}")));
    }

    #[test]
    fn extra_args_keep_the_password_off_the_command_line() {
        let t = translate_extra_args("-pw secret -P 2222 -A -load profile -sercfg 9600");
        assert_eq!(t.password.as_deref(), Some("secret"));
        assert_eq!(t.args, ["-p", "2222", "-A"]);
        assert_eq!(t.notices, [format!("Not used on {THIS_COMPUTER}: -sercfg 9600")]);
    }

    #[test]
    fn quick_connect_reads_the_host_field_as_superxputty_does() {
        let quick = QuickConnect {
            protocol: "SSH".into(),
            host: "admin@192.168.0.50:2222".into(),
            username: String::new(),
            password: "pw".into(),
            profile: String::new(),
        };
        let session = quick.session().unwrap();
        assert_eq!((session.get("Host"), session.get("Port"), session.get("Username")), ("192.168.0.50", "2222", "admin"));
        let (args, _) = args_of(&Sessions::default(), &profiles(), &session);
        assert_eq!(
            args,
            ["-tt", "-p", "2222", "-l", "admin", "-o", "ServerAliveInterval=30", "-o",
             "PreferredAuthentications=keyboard-interactive,password,publickey", "192.168.0.50"]
        );
        let telnet = QuickConnect { host: "telnet://10.0.0.1".into(), ..quick };
        assert_eq!(telnet.session().unwrap().get("Proto"), "TELNET");
    }

    #[test]
    fn older_algorithms_are_ones_this_ssh_takes() {
        // ssh -G reads the options without connecting, and fails on an algorithm it doesn't know
        let args = older_algorithm_args();
        let Ok(output) = std::process::Command::new(openssh("ssh")).args(&args).args(["-G", "example.invalid"]).output() else {
            return; // no ssh here to ask
        };
        assert!(output.status.success(), "ssh refused {args:?}: {}", String::from_utf8_lossy(&output.stderr));
        let config = String::from_utf8_lossy(&output.stdout).to_string();
        let macs = config.lines().find(|l| l.starts_with("macs ")).unwrap_or_default();
        if ssh_query("mac").is_some_and(|m| m.iter().any(|a| a == "hmac-sha1")) {
            assert!(macs.split([' ', ',']).any(|m| m == "hmac-sha1"), "{macs}");
        }
    }

    #[test]
    fn telnet_raw_and_serial_are_streams() {
        use crate::streams::{SerialLine, Target};
        let devices: BTreeMap<String, String> = [("COM6".to_string(), "/dev/cu.usbserial-1".to_string())].into();
        // the attributes given win over the helper's host
        let run = |attributes: &[(&str, &str)]| {
            let mut s = session(attributes);
            for (name, value) in attributes {
                s.set(name, value);
            }
            launch(&Sessions::default(), &profiles(), &HashMap::new(), &devices, Store::Saved, &s)
        };
        assert_eq!(run(&[("Proto", "Telnet")]).unwrap().stream, Some(Target::Telnet { host: "10.0.0.1".into(), port: 23, via: None }));
        assert_eq!(
            run(&[("Proto", "Raw"), ("Port", "2001"), ("JumpHost", "me@bastion:2222")]).unwrap().stream,
            Some(Target::Raw { host: "10.0.0.1".into(), port: 2001, via: Some("me@bastion:2222".into()) })
        );
        assert!(run(&[("Proto", "Raw")]).is_err(), "Raw needs a port");
        let serial = run(&[("Proto", "Serial"), ("Host", "com6"), ("ExtraArgs", "-sercfg 115200,8,n,1,N")]).unwrap();
        let line = SerialLine { speed: 115200, flow: 'N', ..SerialLine::default() };
        if cfg!(windows) {
            // the COM name is the port itself, matched or not
            assert_eq!(serial.stream, Some(Target::Serial { device: "COM6".into(), line: line.clone() }));
            assert!(serial.notices.is_empty(), "{:?}", serial.notices);
            assert_eq!(run(&[("Proto", "Serial"), ("Host", "COM7")]).unwrap().stream, Some(Target::Serial { device: "COM7".into(), line: SerialLine::default() }));
        } else {
            assert_eq!(serial.stream, Some(Target::Serial { device: "/dev/cu.usbserial-1".into(), line }));
            // a COM name not yet matched to a device is asked
            assert_eq!(run(&[("Proto", "Serial"), ("Host", "COM7")]).err().unwrap(), format!("{SERIAL_DEVICE_NEEDED}COM7"));
        }
    }

    #[test]
    fn jump_hosts_resolve_as_superxputty() {
        let bastion = StoredSession {
            attributes: vec![("SessionId".into(), "Bastions/b1".into()), ("Host".into(), "{dc}-bastion".into()), ("Port".into(), "0".into())],
        };
        let folder = FolderSetting { path: "Bastions".into(), username: "ops".into(), port: 2222, ..FolderSetting::default() };
        let sessions = Sessions {
            saved: vec![bastion.clone()],
            folders: [(Store::Saved, [("bastions".to_string(), folder)].into())].into(),
            ..Sessions::default()
        };
        let values: HashMap<String, String> = [("dc".to_string(), "lon".to_string())].into();
        let from = session(&[]);
        // a saved session: its folder's username and port, its placeholders filled
        assert_eq!(jump_address(&sessions, &values, Store::Saved, &from, "Bastions/b1").unwrap(), "ops@lon-bastion:2222");
        // user@host:port as it is
        assert_eq!(jump_address(&sessions, &values, Store::Saved, &from, "me@jump:22").unwrap(), "me@jump:22");
        assert!(jump_address(&sessions, &values, Store::Saved, &from, "not a host").is_err());
        assert!(jump_address(&sessions, &values, Store::Saved, &bastion, "Bastions/b1").is_err(), "not through itself");
    }

    #[test]
    fn quoted_words_stay_together() {
        assert_eq!(split_words(r#"-i "/Users/me/my key" -v"#), ["-i", "/Users/me/my key", "-v"]);
    }
}
