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

use std::collections::HashMap;

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
}

/// Algorithms older devices offer and OpenSSH 10 no longer accepts by default; added for each connection only.
/// They're the "Allow older algorithms" profile option (docs/PLAN.md §2 Profiles), on in "Default Settings".
/// Checked against an IOS lab switch.
const OLDER_ALGORITHMS: [&str; 8] = [
    "-o", "KexAlgorithms=+diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1",
    "-o", "HostKeyAlgorithms=+ssh-rsa",
    "-o", "PubkeyAcceptedAlgorithms=+ssh-rsa",
    "-o", "Ciphers=+aes256-cbc,aes192-cbc,aes128-cbc",
];

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

/// Build the start of a stored session, or say why it can't start on this Mac yet
pub fn launch(
    sessions: &Sessions,
    profiles: &Profiles,
    values: &HashMap<String, String>,
    store: Store,
    session: &StoredSession,
) -> Result<Launch, String> {
    match session.get("Proto").to_ascii_uppercase().as_str() {
        "SSH" | "" => ssh(sessions, profiles, values, store, session),
        "WINCMD" | "WSL" => {
            let mut launch = local_shell(profiles);
            launch.notices.push(format!("{} session opened as a Local Shell on this Mac.", session.get("Proto")));
            Ok(launch)
        }
        other => Err(format!("{other} sessions can't be opened yet.")),
    }
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
    }
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
    let mut profile_name = session.get("PuttySession").to_string();
    let mut rules = session.get("HighlightRules").to_string();
    for folder in sessions.along(store, session.folder()) {
        if username.is_empty() {
            username = folder.username.clone();
        }
        if port == 0 {
            port = folder.port;
        }
        if jump_host.is_empty() {
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
            "Profile \"{profile_name}\" isn't set up on this Mac, so Default Settings is used. Add it in Settings > Profiles."
        ));
        default
    };
    if username.is_empty() {
        username = first(&profile.username, &default.username).to_string();
    }
    if port == 0 {
        port = if profile.port != 0 { profile.port } else { default.port };
    }
    if jump_host.is_empty() {
        jump_host = first(&profile.jump_host, &default.jump_host).to_string();
    }

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
    let host = variables::substitute(session.get("Host").trim(), values);
    let username = variables::substitute(&username, values);
    let jump_host = variables::substitute(&jump_host, values);
    if host.is_empty() {
        return Err("This session has no host.".into());
    }

    let mut args: Vec<String> = vec!["-tt".into()];
    if profile.older_algorithms.or(default.older_algorithms).unwrap_or(true) {
        args.extend(OLDER_ALGORITHMS.iter().map(|s| s.to_string()));
    }
    let extra = translate_extra_args(&variables::substitute(raw_extra, values));
    notices.extend(extra.notices);
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
    if !key_file.is_empty() {
        let path = expand_home(key_file);
        if !std::path::Path::new(&path).exists() {
            notices.push(format!("The key file {key_file} isn't there; connecting without it."));
        } else if path.to_ascii_lowercase().ends_with(".ppk") {
            notices.push(format!("{key_file} is a PuTTY key; convert it with: puttygen key.ppk -O private-openssh"));
        } else {
            args.extend(["-i".into(), path, "-o".into(), "IdentitiesOnly=yes".into()]);
        }
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
        args.extend(["-J".into(), jump_address(sessions, store, &jump_host)?]);
    }
    args.extend(split_words(first(&profile.ssh_options, &default.ssh_options)));
    args.extend(extra.args);
    let config_host = first(&profile.ssh_config_host, &default.ssh_config_host);
    if config_host.is_empty() {
        args.push(host);
    } else {
        // the ~/.ssh/config Host gives everything else; the session's own address still wins
        args.extend(["-o".into(), format!("HostName={host}"), config_host.to_string()]);
    }

    let mut command = CommandBuilder::new("ssh");
    command.args(&args);
    Ok(Launch { command, password, local: false, notices, look: Look { rules, ..look(profile, default) } })
}

/// A path with a leading "~/" turned into the home folder
fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), dirs::home_dir()) {
        (Some(rest), Some(home)) => home.join(rest).display().to_string(),
        _ => path.to_string(),
    }
}

/// A jump host as `ssh -J` takes it: "user@host:port" as it is, or a saved session's id turned into its address
fn jump_address(sessions: &Sessions, store: Store, jump: &str) -> Result<String, String> {
    let Some(session) = sessions.find(store, jump).or_else(|| sessions.find(Store::Saved, jump)) else {
        return Ok(jump.to_string());
    };
    let mut address = session.get("Host").to_string();
    if !session.get("Username").is_empty() {
        address = format!("{}@{address}", session.get("Username"));
    }
    if session.port() != 0 && session.port() != 22 {
        address = format!("{address}:{}", session.port());
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
                        out.notices.push(format!("{v} is a PuTTY key; convert it with: puttygen key.ppk -O private-openssh"));
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
        out.notices.push(format!("Not used on this Mac: {}", skipped.join(" ")));
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
        let launch = launch(sessions, profiles, &HashMap::new(), Store::Saved, session).unwrap();
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

    #[test]
    fn the_profile_fills_what_the_session_and_folder_leave_empty() {
        let mut sessions = Sessions::default();
        let folder = FolderSetting { path: "Lab".into(), port: 2200, ..FolderSetting::default() };
        sessions.folders.entry(Store::Saved).or_default().insert("Lab".into(), folder);
        let (args, notices) = args_of(&sessions, &profiles(), &session(&[("PuttySession", "root")]));
        // the folder's port beats the profile's; the profile gives the username; Default Settings the keep-alive
        assert_eq!(args, ["-tt", "-p", "2200", "-l", "root", "-o", "ServerAliveInterval=30", "10.0.0.1"]);
        assert!(notices.is_empty());
    }

    #[test]
    fn a_missing_profile_falls_back_to_default_settings_and_says_so() {
        let (args, notices) = args_of(&Sessions::default(), &profiles(), &session(&[("PuttySession", "netops")]));
        assert_eq!(args, ["-tt", "-o", "ServerAliveInterval=30", "10.0.0.1"]);
        assert!(notices[0].contains("\"netops\" isn't set up on this Mac"));
    }

    #[test]
    fn extra_args_keep_the_password_off_the_command_line() {
        let t = translate_extra_args("-pw secret -P 2222 -A -load profile -sercfg 9600");
        assert_eq!(t.password.as_deref(), Some("secret"));
        assert_eq!(t.args, ["-p", "2222", "-A"]);
        assert_eq!(t.notices, ["Not used on this Mac: -sercfg 9600"]);
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
    fn quoted_words_stay_together() {
        assert_eq!(split_words(r#"-i "/Users/me/my key" -v"#), ["-i", "/Users/me/my key", "-v"]);
    }
}
