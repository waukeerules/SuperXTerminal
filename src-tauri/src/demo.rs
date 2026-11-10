//! Demo mode, for screenshots and showing the app to people: started with `--demo` (or `SUPERXTERMINAL_DEMO=1` in
//! the environment), the app uses a folder of made-up sessions instead of the user's own, and every session runs a
//! stand-in device (`demo_device.rs`) instead of ssh, so nothing real is shown or reached.
//!
//! The folder (`SuperXTerminal-demo` in the temporary folder) is made afresh from the files in `src-tauri/demo/` at
//! each demo start: Saved sessions, Private ones, a Database sessions cache of a few thousand (no database is read),
//! folder colours, button bars, Favorites and Recent, Environment Variables, profiles, saved scripts (the examples in
//! docs/) and the layout it opens with. Whatever is changed while demoing goes there and is
//! gone at the next start. `sessions::user_folder` gives this folder in demo mode, so the whole app follows it.
//!
//! A demo window doesn't hand itself over to a SuperXTerminal already running (the single-instance plugin is left
//! out), and it doesn't use or change where the real main window was left: it opens at the default 1400 x 900, the
//! same size each time, for screenshots. The WebView's own storage (a few layout choices) is still the app's.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use portable_pty::CommandBuilder;

use crate::launch::Launch;
use crate::sessions::StoredSession;

/// The command-line switch that starts demo mode
pub const SWITCH: &str = "--demo";

/// The environment variable that starts demo mode, for `npm run tauri dev` (which passes no arguments)
pub const ENV: &str = "SUPERXTERMINAL_DEMO";

/// The demo's saved scripts (Script Editor > Saved scripts): the examples in docs/, by file name
const SCRIPTS: [(&str, &str); 8] = [
    ("cisco-tour.rhai", include_str!("../../docs/rhai-examples/cisco-tour.rhai")),
    ("cisco-login.rhai", include_str!("../../docs/rhai-examples/cisco-login.rhai")),
    ("cisco-save.rhai", include_str!("../../docs/rhai-examples/cisco-save.rhai")),
    ("linux-values.rhai", include_str!("../../docs/rhai-examples/linux-values.rhai")),
    ("cisco-jump.spsl", include_str!("../../docs/spsl-examples/cisco-jump.spsl")),
    ("jump-host.spsl", include_str!("../../docs/spsl-examples/jump-host.spsl")),
    ("linux-tour.spsl", include_str!("../../docs/spsl-examples/linux-tour.spsl")),
    ("linux-tour-2.spsl", include_str!("../../docs/spsl-examples/linux-tour-2.spsl")),
];

/// The layout a demo opens with (Settings > General's default layout), saved in its settings folder
const LAYOUT: &str = "Demo";

/// The environment variable naming a real Remote Desktop server for the demo's win-jump session, as user@host or
/// host: a test machine, for a screenshot of an RDP tab. Its address is written only to the demo's own folder, and
/// no password is: the tab asks for it
pub const RDP_ENV: &str = "SUPERXTERMINAL_DEMO_RDP";

/// The demo's Sessions.xml with win-jump pointed at `target` (user@host or host), when one is given
fn with_rdp_target(sessions: &str, target: Option<&str>) -> String {
    let Some(target) = target.map(str::trim).filter(|t| !t.is_empty()) else { return sessions.to_string() };
    let (user, host) = target.split_once('@').unwrap_or(("lab", target));
    let escape = crate::sessions::escape;
    sessions.replace(
        r#"Host="win-jump.lab.example.net" Port="3389" Proto="RDP" PuttySession="Default Settings" Username="lab""#,
        &format!(r#"Host="{}" Port="3389" Proto="RDP" PuttySession="Default Settings" Username="{}""#, escape(host), escape(user)),
    )
}

/// Whether this run is a demo, decided once from the command line and the environment
static ON: OnceLock<bool> = OnceLock::new();

/// Whether the app is running in demo mode
pub fn on() -> bool {
    *ON.get_or_init(|| {
        let args: Vec<String> = std::env::args().skip(1).collect();
        asked(&args, std::env::var(ENV).ok().as_deref())
    })
}

/// Whether these arguments (the program's own left out) or this value of [`ENV`] ask for demo mode: `--demo`
/// anywhere, or the variable set to anything but "", "0" or "false"
fn asked(args: &[String], env: Option<&str>) -> bool {
    let by_env = env.is_some_and(|v| !matches!(v.trim().to_ascii_lowercase().as_str(), "" | "0" | "false"));
    by_env || args.iter().any(|a| a == SWITCH)
}

/// The demo's folder, made afresh at each demo start: /tmp's on the Mac and Linux, whose path reads well in a
/// screenshot (the file transfer's local pane shows it), else the temporary folder
pub fn folder() -> PathBuf {
    if cfg!(unix) {
        PathBuf::from("/tmp/SuperXTerminal-demo")
    } else {
        std::env::temp_dir().join("SuperXTerminal-demo")
    }
}

/// The demo's own Downloads, the file transfer's local pane in place of the user's home (sftp.rs)
pub fn downloads() -> PathBuf {
    folder().join("Downloads")
}

/// The demo's user folder (settings, Private sessions, the Database cache), in place of the user's own
pub fn user_folder() -> PathBuf {
    folder().join("user")
}

/// The demo's settings folder (Saved sessions, folders, button bars), as a team's shared one
fn shared_folder() -> PathBuf {
    folder().join("shared")
}

/// The demo's Database connection: made up, never reached (see `refresh_database` in lib.rs), but it names the
/// cache's Source and shows in Settings > Database
fn database() -> crate::db::DbSettings {
    crate::db::DbSettings {
        enabled: true,
        host: "sessions-db.example.net".into(),
        user: "noc".into(),
        nickname: "Demo".into(),
        check_seconds: 0,
        ..crate::db::DbSettings::default()
    }
}

/// Make the demo's folder afresh, removing what the last demo left
pub fn prepare() -> Result<(), String> {
    let root = folder();
    if root.exists() {
        std::fs::remove_dir_all(&root).map_err(|e| format!("{}: {e}", root.display()))?;
    }
    seed(&user_folder(), &shared_folder()).map_err(|e| format!("{}: {e}", root.display()))
}

/// Write the demo's files into its user and settings folders
fn seed(user: &Path, shared: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(user)?;
    std::fs::create_dir_all(shared)?;
    std::fs::write(shared.join("Sessions.xml"), with_rdp_target(include_str!("../demo/Sessions.xml"), std::env::var(RDP_ENV).ok().as_deref()))?;
    std::fs::write(shared.join("Folders.xml"), include_str!("../demo/Folders.xml"))?;
    std::fs::write(shared.join("ButtonBars.xml"), include_str!("../demo/ButtonBars.xml"))?;
    // the tabs it opens with: switches, a firewall and servers, split in two (dockview's own JSON, as App.svelte saves
    // a layout); JSON has no comments, so this is where it's described
    std::fs::create_dir_all(shared.join("layouts-mac"))?;
    std::fs::write(shared.join("layouts-mac").join(format!("{LAYOUT}.json")), include_str!("../demo/Demo.layout.json"))?;
    // saved scripts for the Script Editor: the Rhai and SPSL examples from docs/, which the stand-in devices answer
    let scripts = shared.join("scripts");
    std::fs::create_dir_all(&scripts)?;
    for (name, text) in SCRIPTS {
        std::fs::write(scripts.join(name), text.replace("\r\n", "\n").replace('\n', "\r\n"))?;
    }
    std::fs::write(user.join("PrivateSessions.xml"), include_str!("../demo/PrivateSessions.xml"))?;
    let source = crate::sessions::escape(&database().source());
    let cache = include_str!("../demo/DatabaseSessions.cache.xml").replace("{source}", &source).replace("{generated}", &site_sessions());
    std::fs::write(user.join("DatabaseSessions.cache.xml"), cache)?;
    // values for the sessions', folders' and profiles' {placeholders}, one a 1Password reference (for scripts, so
    // unused by sessions); {branch_user} and the field team's {field_user}
    // (the Branches folder's username) is left unset, so Environment Variables shows what's missing too
    // the busiest first, as the window lists them in this order and shows about ten before it scrolls
    let variables = [
        ("noc_user", "netops"),
        ("tacacs_user", "netops-ro"),
        ("readonly_user", "noc-ro"),
        ("ops_user", "deploy"),
        ("site_domain", "sites.example.net"),
        ("jump_us", "netops@jump.us.example.net"),
        ("jump_eu", "netops@jump.eu.example.net"),
        ("ssh_timeout", "15"),
        ("enable_secret", "op://Network/Core switches/enable"),
        ("console_user", "admin"),
        ("keepalive_secs", "30"),
        ("jump_ca", "netops@jump.ca.example.net"),
        ("jump_uk", "netops@jump.uk.example.net"),
        ("jump_ap", "netops@jump.ap.example.net"),
        ("jump_me", "netops@jump.me.example.net"),
        ("domain", "example.net"),
        ("region", "us-west"),
        ("bastion_user", "deploy"),
        ("lab_user", "lab"),
        ("dc2_jump", "netops@jump01.dc2.example.net"),
    ];
    let variables: Vec<serde_json::Value> =
        variables.iter().map(|(key, value)| serde_json::json!({ "Key": key, "Value": value, "Secret": false })).collect();
    std::fs::write(user.join("EnvironmentVariables.json"), serde_json::json!({ "Variables": variables }).to_string())?;
    let shortcuts = crate::shortcuts::Shortcuts {
        favorites: vec!["Production/Network/Core/core-sw-01".into(), "Production/Servers/web-01".into(), "@db/NOC/Datacentre/dc2-core-01".into()],
        // the ten kept (the most there are): the Demo layout's six tabs, as if just opened, then others
        recent: [
            "Production/Network/Core/core-sw-01",
            "Production/Servers/web-01",
            "Production/Network/Distribution/dist-sw-01",
            "Production/Servers/k8s-ctl-01",
            "Production/Network/Edge/edge-srx-01",
            "Staging/stg-db-01",
            "@db/NOC/Datacentre/dc2-fw-01",
            "Branches/London/lon-sw-01",
            "Lab/lab-mx-01",
            "Home/nas",
        ]
        .map(String::from)
        .to_vec(),
        favorites_expanded: true,
        recent_expanded: false,
    };
    std::fs::write(user.join("SessionShortcuts.json"), serde_json::to_string_pretty(&shortcuts).unwrap_or_default())?;
    profiles().save(user).map_err(std::io::Error::other)?;
    settings(shared).save(user).map_err(std::io::Error::other)?;
    // the file transfer's local side: a few made-up files, and an image already copied from another switch
    let downloads = user.parent().unwrap_or(user).join("Downloads");
    std::fs::create_dir_all(downloads.join("configs"))?;
    std::fs::create_dir_all(downloads.join("images"))?;
    std::fs::write(downloads.join("configs/core-sw-01-confg"), "hostname core-sw-01\n!\nend\n")?;
    std::fs::write(downloads.join("configs/edge-srx-01.conf"), "system { host-name edge-srx-01; }\n")?;
    std::fs::write(downloads.join("change-2026-11-07.md"), "# Change 4412: core uplinks to 10G\n")?;
    std::fs::File::create(downloads.join("images/cat9k_iosxe.17.09.04a.SPA.bin"))?.set_len(1_243_762_436)?;
    Ok(())
}

/// The demo's PuTTY profiles (Settings > Profiles), which its sessions use: network devices (Juniper, console
/// servers and old IOS among them), Linux servers, web servers with a tunnel, servers through a jump host or
/// CyberArk, cloud VMs, short-lived keys and a light-coloured lab. Made up; no keys or passwords
fn profiles() -> crate::profiles::Profiles {
    use crate::profiles::{Forward, Profile};
    let mut profiles = crate::profiles::Profiles::default();
    profiles.profiles.push(Profile {
        name: "Network devices".into(),
        username: "netops".into(),
        offer_keys: Some(false),
        older_algorithms: Some(true),
        keepalive: 30,
        anti_idle: "\\n".into(),
        anti_idle_seconds: 240,
        button_bar: "Cisco".into(),
        scrollback: 50000,
        ..Profile::default()
    });
    profiles.profiles.push(Profile {
        name: "Linux servers".into(),
        username: "deploy".into(),
        key_file: "~/.ssh/id_ed25519".into(),
        agent_forwarding: Some(true),
        server_load: Some(true),
        button_bar: "Linux".into(),
        ..Profile::default()
    });
    // the web servers' own, for their tunnel: one port on this computer can be one tab's tunnel only, so the Demo
    // layout's other Linux tabs (k8s-ctl-01, stg-db-01) don't ask for it too
    profiles.profiles.push(Profile {
        name: "Web servers".into(),
        based_on: "Linux servers".into(),
        forwards: vec![Forward { kind: "local".into(), listen: "8080".into(), to: "localhost:80".into() }],
        ..Profile::default()
    });
    profiles.profiles.push(Profile {
        name: "Via bastion".into(),
        based_on: "Linux servers".into(),
        jump_host: "{bastion_user}@bastion.{domain}".into(),
        ..Profile::default()
    });
    profiles.profiles.push(Profile {
        name: "Short-lived keys".into(),
        username: "{noc_user}".into(),
        key_file: "~/.ssh/noc_cert".into(),
        key_fetch: "step ssh certificate {noc_user} {key_file} --provisioner noc --force".into(),
        key_lifetime: 480,
        ..Profile::default()
    });
    profiles.profiles.push(Profile {
        name: "Juniper".into(),
        based_on: "Network devices".into(),
        button_bar: "Junos".into(),
        ..Profile::default()
    });
    profiles.profiles.push(Profile {
        name: "Console servers".into(),
        username: "admin".into(),
        port: 3001,
        keepalive: 15,
        charset: "ISO-8859-1".into(),
        ..Profile::default()
    });
    profiles.profiles.push(Profile {
        name: "Legacy IOS".into(),
        based_on: "Network devices".into(),
        username: "{tacacs_user}".into(),
        algorithms: "KexAlgorithms=+diffie-hellman-group1-sha1".into(),
        key_backspace: "bs".into(),
        ..Profile::default()
    });
    profiles.profiles.push(Profile {
        name: "CyberArk PSM".into(),
        username: "{noc_user}@netops#{domain}@{host_address}".into(),
        host: "psm.{domain}".into(),
        port: 22,
        ..Profile::default()
    });
    profiles.profiles.push(Profile {
        name: "Cloud VMs".into(),
        username: "ubuntu".into(),
        host: "gw.{region}.example.org".into(),
        key_file: "~/.ssh/cloud-{region}.pem".into(),
        compression: Some(true),
        forwards: vec![Forward { kind: "dynamic".into(), listen: "1080".into(), to: String::new() }],
        ..Profile::default()
    });
    profiles.profiles.push(Profile {
        name: "Lab (light)".into(),
        username: "lab".into(),
        colours: "custom".into(),
        background: "#FDF6E3".into(),
        foreground: "#3B3B3B".into(),
        selection: "#C9DDF2".into(),
        cursor_color: "#6A3D9A".into(),
        tab_color: "#6A3D9A".into(),
        font_family: "Menlo".into(),
        font_size: 13,
        ..Profile::default()
    });
    profiles
}

/// The demo's settings: its own settings folder and Database, the environment colour rules its folders are named
/// for, the button bar and Send Commands shown, the Demo layout at start, the terminal unwrapped with line numbers and
/// a steady block cursor, and nothing that reaches out (update checks, session logs, the menu bar icon beside a real
/// SuperXTerminal's)
fn settings(shared: &Path) -> crate::settings::AppSettings {
    let mut settings = crate::settings::AppSettings::default();
    settings.settings_folder = Some(shared.to_path_buf());
    settings.environment_color_rules = "Production=#C62828;Prod=#C62828;Staging=#EF6C00;UAT=#EF6C00".into();
    settings.database = database();
    settings.use_sessions_xml = true;
    settings.use_private_sessions = true;
    settings.show_button_bar = true;
    settings.show_send_commands = true;
    settings.show_status_bar = true;
    settings.session_log_enabled = false;
    settings.auto_update_check = false;
    settings.auto_reconnect = false;
    settings.show_in_menu_bar = false;
    settings.default_layout_name = LAYOUT.into();
    // lines not wrapped and numbered, two of the terminal's less usual features; a font small enough for a switch's
    // interface table to fit half the window
    settings.terminal_prefs.insert("wrap".into(), "false".into());
    settings.terminal_prefs.insert("lineNumbers".into(), "true".into());
    settings.terminal_prefs.insert("fontSize".into(), "12".into());
    // a block cursor that doesn't blink, so a screenshot always has it, whatever the demo's profiles say
    settings.terminal_prefs.insert("cursorShape".into(), "block".into());
    settings.terminal_prefs.insert("cursorBlink".into(), "off".into());
    settings
}

/// The regions of the made-up company's sites, and the start of their sites' codes
const REGIONS: [(&str, &str); 6] = [("Americas", "us"), ("Canada", "ca"), ("Europe", "eu"), ("UK", "uk"), ("Asia Pacific", "ap"), ("Middle East", "me")];

/// Sites in each region
const SITES: u32 = 30;

/// The devices at every site: name, icon and how many
const SITE_DEVICES: [(&str, &str, u32); 7] = [
    ("sw", "switch", 10),
    ("rtr", "router", 2),
    ("fw", "firewall", 1),
    ("wlc", "wlc", 1),
    ("ups", "ups", 1),
    ("con", "consoleserver", 1),
    ("srv", "server", 2),
];

/// The Database sessions of the made-up company's sites, as cache lines: NOC/Sites/<region>/<site>/<device>, 18 at
/// each of 180 sites (3,240), so the demo shows the tree and its search with a table of thousands. Their usernames,
/// jump hosts, host names and extra arguments use Environment Variables unevenly, as a real team's table grows
/// ([`pick`]), so Environment Variables shows each used in its own number of places. The same each time
fn site_sessions() -> String {
    let editors = ["Alex", "Sam", "Priya", "Jordan", "Mei"];
    let mut out = String::new();
    let mut n: u32 = 0;
    for (r, (region, code)) in REGIONS.iter().enumerate() {
        for site in 1..=SITES {
            let site_name = format!("{code}-site-{site:03}");
            // a site goes through its region's jump host, or not, and is named in DNS, or by address
            let jumped = pick(r as u32 * 100 + site, 1, 100) < 55;
            let named = pick(r as u32 * 100 + site, 2, 100) < 30;
            for (kind, icon, count) in SITE_DEVICES {
                for unit in 1..=count {
                    n += 1;
                    let name = format!("{site_name}-{kind}-{unit:02}");
                    let host = if named { format!("{name}.{{site_domain}}") } else { format!("10.{}.{}.{}", 100 + r, site, n % 200 + 10) };
                    let profile = match icon {
                        "server" => "Linux servers",
                        "firewall" => "CyberArk PSM",
                        "consoleserver" => "Console servers",
                        _ => "Network devices",
                    };
                    let username = match (icon, pick(n, 3, 100)) {
                        ("server", roll) if roll < 70 => "{ops_user}",
                        ("consoleserver", _) => "{console_user}",
                        (_, roll) if roll < 62 => "{noc_user}",
                        (_, roll) if roll < 80 => "{tacacs_user}",
                        (_, roll) if roll < 93 => "{readonly_user}",
                        // a few the field team added, whose value nobody here has yet
                        _ => "{field_user}",
                    };
                    let jump = if jumped { format!("{{jump_{code}}}") } else { String::new() };
                    let extra = match pick(n, 4, 100) {
                        roll if roll < 12 => "-o ConnectTimeout={ssh_timeout}",
                        roll if roll < 16 => "-o ServerAliveInterval={keepalive_secs}",
                        _ => "",
                    };
                    out.push_str(&format!(
                        "    <SessionData SessionId=\"@db/NOC/Sites/{region}/{site_name}/{name}\" DatabaseId=\"00000000-0000-4000-8000-{n:012}\" \
                         DatabaseVersion=\"{}\" DatabaseLockedFields=\"\" DatabaseUpdatedBy=\"{}\" SessionName=\"{name}\" ImageKey=\"{icon}\" \
                         Host=\"{host}\" Port=\"22\" Proto=\"SSH\" PuttySession=\"{profile}\" Username=\"{username}\" ExtraArgs=\"{extra}\" \
                         JumpHost=\"{jump}\" CollectionID=\"\" CollectionLocation=\"\" />\r\n",
                        639291000000000000u64 + n as u64,
                        editors[n as usize % editors.len()],
                    ));
                }
            }
        }
    }
    out
}

/// A number below `below` that looks random but is the same each time for this `n` and `salt`, so the demo's
/// sessions differ from each other without changing between starts
fn pick(n: u32, salt: u32, below: u32) -> u32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x % below
}

/// What a stand-in device acts as, from the session: a Juniper for hosts named so (srx, mx) or a firewall icon, a
/// Cisco for other network icons, else a Linux server
pub fn kind_of(session: &StoredSession) -> &'static str {
    let host = session.get("Host").to_ascii_lowercase();
    let icon = session.get("ImageKey").to_ascii_lowercase();
    let icon = icon.split('#').next().unwrap_or_default();
    let junos = ["srx", "-mx-", "junos"].iter().any(|n| host.contains(n));
    if junos || icon == "firewall" {
        "junos"
    } else if matches!(
        icon,
        "switch" | "stackswitch" | "coreswitch" | "distswitch" | "router" | "voicerouter" | "nexus" | "vpn" | "wlc" | "accesspoint" | "consoleserver" | "ups"
    ) {
        "ios"
    } else {
        "linux"
    }
}

/// The session's start with the stand-in device in place of its program (ssh, Telnet, a serial line...): the
/// session's look, highlighting and notices are kept as worked out for the real thing, but no password is typed,
/// no connection made and no server load read
pub fn stand_in(mut launch: Launch, session: &StoredSession) -> Launch {
    let name = match session.get("SessionName") {
        "" => session.id().rsplit('/').next().unwrap_or_default(),
        name => name,
    };
    launch.command = device_command(kind_of(session), name, session.get("Host"), session.get("ImageKey"));
    launch.password = None;
    launch.stream = None;
    launch.server_load = None;
    launch.login = None;
    launch
}

/// The stand-in for New Local Shell: this computer's own shell, made up
pub fn local_shell(mut launch: Launch) -> Launch {
    launch.command = device_command("shell", "", "", "");
    launch
}

/// This program, run as a stand-in device of this kind for this session (`demo_device::run` reads the variables)
fn device_command(kind: &str, name: &str, host: &str, icon: &str) -> CommandBuilder {
    let program = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("superxterminal"));
    let mut command = CommandBuilder::new(program);
    command.env(crate::demo_device::KIND_VAR, kind);
    command.env(crate::demo_device::NAME_VAR, name);
    command.env(crate::demo_device::HOST_VAR, host);
    command.env(crate::demo_device::ICON_VAR, icon);
    command
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session with these attributes
    fn session(host: &str, icon: &str) -> StoredSession {
        StoredSession { attributes: vec![("Host".into(), host.into()), ("ImageKey".into(), icon.into())] }
    }

    /// The demo's terminal draws a block cursor that doesn't blink, whatever a profile says, as the terminal menu's
    /// Cursor > Block and Don't Blink do
    #[test]
    fn the_demo_terminal_has_a_steady_block_cursor() {
        let shared = std::env::temp_dir().join("sxt-demo-cursor");
        let prefs = settings(&shared).terminal_prefs;
        assert_eq!(prefs.get("cursorShape").map(String::as_str), Some("block"));
        assert_eq!(prefs.get("cursorBlink").map(String::as_str), Some("off"));
    }

    /// Some demo sessions name profiles the demo doesn't have, so Settings > Profiles' "Named by sessions, not set up"
    /// has something to show: Arista EOS by two switches, Palo Alto by one firewall
    #[test]
    fn some_demo_sessions_name_profiles_that_arent_set_up() {
        let root = std::env::temp_dir().join(format!("sxt-demo-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (user, shared) = (root.join("user"), root.join("shared"));
        seed(&user, &shared).unwrap();
        let mut problems = Vec::new();
        let saved = crate::sessions::read_sessions_file(&shared.join("Sessions.xml"), &mut problems);
        let named = saved.iter().map(|s| (s.get("PuttySession"), format!("session {}", s.id())));
        let missing = crate::profiles::missing(&profiles(), named);
        let counts: Vec<(&str, usize)> = missing.iter().map(|(name, places)| (name.as_str(), places.len())).collect();
        assert_eq!(counts, [("Arista EOS", 2), ("Palo Alto", 1)]);
        let _ = std::fs::remove_dir_all(&root);
    }

    /// No two tabs of the Demo layout ask for the same port on this computer, so none opens with "Port 8080 on this
    /// Mac is already the tunnel of the tab..." at the top: only web-01's profile has the 8080 tunnel, whichever tab
    /// starts first (the main-window screenshot shows web-01 with its tunnel listed, not a warning)
    #[test]
    fn no_two_tabs_of_the_demo_layout_ask_for_the_same_port() {
        let sessions = include_str!("../demo/Sessions.xml");
        let layout: serde_json::Value = serde_json::from_str(include_str!("../demo/Demo.layout.json")).unwrap();
        let profiles = profiles();
        let mut held: std::collections::HashMap<String, String> = Default::default();
        for (_, panel) in layout["tabs"]["panels"].as_object().unwrap() {
            let id = panel["params"]["target"]["id"].as_str().unwrap();
            let profile = sessions
                .lines()
                .find(|l| l.contains(&format!(r#"SessionId="{id}""#)))
                .and_then(|l| l.split(r#"PuttySession=""#).nth(1))
                .and_then(|rest| rest.split('"').next())
                .unwrap_or_else(|| panic!("{id} names no profile"));
            let Some(resolved) = profiles.resolved(profile) else { continue };
            if resolved.tunnels_off == Some(true) {
                continue;
            }
            for forward in resolved.forwards.iter().filter(|f| f.kind != "remote") {
                if let Some(other) = held.insert(forward.listen.clone(), id.to_string()) {
                    panic!("{id} and {other} both ask for port {}", forward.listen);
                }
            }
        }
        assert_eq!(held.get("8080").map(String::as_str), Some("Production/Servers/web-01"));
    }

    #[test]
    fn demo_mode_is_asked_for_by_the_switch_or_the_variable() {
        assert!(asked(&["--demo".into()], None));
        assert!(asked(&["-session".into(), "Lab/sw1".into(), "--demo".into()], None));
        assert!(asked(&[], Some("1")));
        assert!(!asked(&[], None));
        assert!(!asked(&[], Some("0")));
        assert!(!asked(&[], Some("false")));
        assert!(!asked(&["-demo".into()], None));
    }

    #[test]
    fn the_demo_rdp_session_can_be_pointed_at_a_test_server() {
        let sessions = include_str!("../demo/Sessions.xml");
        assert_eq!(with_rdp_target(sessions, None), sessions);
        let pointed = with_rdp_target(sessions, Some("admin@192.0.2.10"));
        assert!(pointed.contains(r#"SessionId="Lab/win-jump" SessionName="win-jump" ImageKey="windows#2E7D32" Host="192.0.2.10" Port="3389" Proto="RDP" PuttySession="Default Settings" Username="admin""#));
        assert!(with_rdp_target(sessions, Some("192.0.2.10")).contains(r#"Host="192.0.2.10" Port="3389" Proto="RDP" PuttySession="Default Settings" Username="lab""#));
    }

    #[test]
    fn each_session_is_stood_in_for_by_a_device_of_its_kind() {
        assert_eq!(kind_of(&session("core-sw-01.example.net", "coreswitch")), "ios");
        assert_eq!(kind_of(&session("10.44.0.1", "voicerouter#1E63B4")), "ios");
        assert_eq!(kind_of(&session("edge-srx-01.example.net", "firewall")), "junos");
        assert_eq!(kind_of(&session("lab-mx-01.lab.example.net", "router#F9A825")), "junos");
        assert_eq!(kind_of(&session("web-01.example.net", "ubuntu")), "linux");
        assert_eq!(kind_of(&session("db-01.example.net", "")), "linux");
    }

    /// The demo's files are what the app reads: every sessions file parses, with the sessions it was written with,
    /// the Database cache matches the demo's connection, the settings point at the demo's own folder, and the button
    /// bars read
    #[test]
    fn the_demo_folder_holds_sessions_the_app_reads() {
        let root = std::env::temp_dir().join(format!("sxt-demo-seed-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let (user, shared) = (root.join("user"), root.join("shared"));
        seed(&user, &shared).unwrap();

        let mut problems = Vec::new();
        let saved = crate::sessions::read_sessions_file(&shared.join("Sessions.xml"), &mut problems);
        let private = crate::sessions::read_sessions_file(&user.join("PrivateSessions.xml"), &mut problems);
        assert!(problems.is_empty(), "{problems:?}");
        assert!(saved.len() > 20 && saved.iter().any(|s| s.id() == "Production/Network/Core/core-sw-01"));
        assert!(private.iter().any(|s| s.id() == "Home/nas"));

        let cached = crate::db::read_cache(&user, &database()).expect("the cache is the demo connection's");
        assert!(cached.sessions.iter().any(|s| s.id() == "@db/NOC/Datacentre/dc2-core-01"));
        // thousands of them, each id once
        assert!(cached.sessions.len() > 3000, "{}", cached.sessions.len());
        let ids: std::collections::HashSet<&str> = cached.sessions.iter().map(|s| s.id()).collect();
        assert_eq!(ids.len(), cached.sessions.len());

        let settings = crate::settings::AppSettings::load(&user);
        assert_eq!(settings.settings_folder.as_deref(), Some(shared.as_path()));
        assert!(settings.database.enabled && !settings.auto_update_check && !settings.show_in_menu_bar);

        let values = crate::variables::values(&user);
        assert_eq!(values.get("noc_user").map(String::as_str), Some("netops"));
        assert!(!values.contains_key("branch_user"), "left unset, to show as missing");
        // the sites' sessions use the variables unevenly: each in its own number of places, every one set but
        // {field_user}, which is missing on purpose
        let mut uses: std::collections::BTreeMap<String, usize> = Default::default();
        for session in &cached.sessions {
            for text in [session.get("Host"), session.get("Username"), session.get("ExtraArgs"), session.get("JumpHost")] {
                for name in crate::variables::names_in(text) {
                    *uses.entry(name).or_default() += 1;
                }
            }
        }
        for (name, count) in &uses {
            assert!(name == "field_user" || values.contains_key(name), "{{{name}}} is used {count} times but has no value");
        }
        let counts: std::collections::BTreeSet<usize> = uses.values().copied().collect();
        assert!(uses.len() >= 12 && counts.len() >= 10, "{uses:?}");

        let shortcuts = crate::shortcuts::load(&user);
        let layout: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(shared.join("layouts-mac/Demo.json")).unwrap()).unwrap();
        for (_, panel) in layout["tabs"]["panels"].as_object().unwrap() {
            let id = panel["params"]["target"]["id"].as_str().unwrap();
            assert!(saved.iter().any(|s| s.id() == id), "the layout opens {id}, which is no session");
        }
        assert_eq!(settings.default_layout_name, LAYOUT);
        // Recent is full, and has every tab the layout opens
        assert_eq!(shortcuts.recent.len(), crate::shortcuts::RECENT_COUNT);
        for (_, panel) in layout["tabs"]["panels"].as_object().unwrap() {
            let id = panel["params"]["target"]["id"].as_str().unwrap();
            assert!(shortcuts.recent.iter().any(|r| r == id), "{id} is open but not in Recent");
        }

        // every profile the sessions name is there, but the two left out on purpose, for Settings > Profiles to list
        // as not set up (some_demo_sessions_name_profiles_that_arent_set_up)
        let profiles = crate::profiles::Profiles::load(&user);
        for session in saved.iter().chain(&private).chain(&cached.sessions) {
            let name = session.get("PuttySession");
            let on_purpose = name == "Arista EOS" || name == "Palo Alto";
            assert!(on_purpose || profiles.profiles.iter().any(|p| p.name == name), "{} uses profile {name}, which isn't there", session.id());
        }

        let bars = crate::buttonbars::load(&shared.join("ButtonBars.xml"));
        assert_eq!(bars.iter().map(|b| b.name.as_str()).collect::<Vec<_>>(), ["Cisco", "Junos", "Linux"]);

        let ids: Vec<&str> = saved.iter().chain(&private).chain(&cached.sessions).map(|s| s.id()).collect();
        for id in shortcuts.favorites.iter().chain(&shortcuts.recent) {
            assert!(ids.contains(&id.as_str()), "{id} is a Favorite or Recent but no session");
        }
        let _ = std::fs::remove_dir_all(&root);
    }
}
