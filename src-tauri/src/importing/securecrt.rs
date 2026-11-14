//! Import from SecureCRT (SuperXTerminal's own; SuperXPuTTY has none): its Sessions folder, one `.ini` file per
//! session and Session Manager's folders as folders, read into sessions under ImportedFromSecureCRT. An SSH
//! session brings its key (its own, or the global one in SSH2.ini), agent and X11 forwarding, port forwards and
//! a jump through another session. Passwords aren't taken: SecureCRT encrypts them.

use std::path::{Path, PathBuf};

use super::{add_arg, read_text, Found};

/// Where SecureCRT keeps its sessions, where the import window starts: `%APPDATA%\VanDyke\Config\Sessions` on
/// Windows, `~/Library/Application Support/VanDyke/SecureCRT/Config/Sessions` on the Mac, and
/// `~/.vandyke/SecureCRT/Config/Sessions` on Linux. Global Options > Configuration Paths can move it, so another
/// folder can be chosen
pub fn securecrt_folder() -> PathBuf {
    if cfg!(windows) {
        dirs::config_dir().unwrap_or_default().join("VanDyke").join("Config").join("Sessions")
    } else if cfg!(target_os = "macos") {
        dirs::config_dir().unwrap_or_default().join("VanDyke/SecureCRT/Config/Sessions")
    } else {
        dirs::home_dir().unwrap_or_default().join(".vandyke/SecureCRT/Config/Sessions")
    }
}

/// One SecureCRT `.ini` file's settings, by lower-cased name. Each line is `T:"Name"=value`, its type T being `S`
/// (text), `D` (a number, written as 8 hex digits: given here in decimal), `Z` (a list: its count, then one line per
/// item, each starting with a space; given joined by new lines) or `B` (binary: its byte count, then hex lines, each
/// starting with a space; left out)
pub(crate) fn securecrt_values(text: &str) -> std::collections::HashMap<String, String> {
    let mut values = std::collections::HashMap::new();
    let mut list: Option<String> = None;
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim_end_matches('\r');
        if let Some(item) = line.strip_prefix(' ') {
            // a Z list's item, or a B value's bytes
            if let Some(name) = &list {
                let joined: &mut String = values.entry(name.clone()).or_default();
                if !joined.is_empty() {
                    joined.push('\n');
                }
                joined.push_str(item);
            }
            continue;
        }
        list = None;
        let Some((kind, rest)) = line.split_once(":\"") else { continue };
        let Some((name, value)) = rest.split_once("\"=") else { continue };
        let name = name.to_ascii_lowercase();
        match kind {
            "S" => {
                values.insert(name, value.to_string());
            }
            "D" => {
                values.insert(name, u32::from_str_radix(value.trim(), 16).map(|n| n.to_string()).unwrap_or_default());
            }
            "Z" => {
                values.insert(name.clone(), String::new());
                list = Some(name);
            }
            _ => {}
        }
    }
    values
}

/// What a SecureCRT session's settings are read with, from its Config folder: the folder itself, for paths SecureCRT
/// writes from it (`${VDS_CONFIG_PATH}\Keys\Identity`), and the global key, SSH2.ini's `Identity Filename V2`, for
/// sessions set to Use Global Public Key
pub(crate) struct SecureCrtConfig {
    folder: String,
    global_key: String,
}

impl SecureCrtConfig {
    /// A key path as ssh takes it: `${VDS_CONFIG_PATH}` replaced by the Config folder, without the `::rawkey` (or
    /// `::cert`) SecureCRT adds after it, and the private key named when it's given as the public half (`.pub`)
    fn key(&self, path: &str) -> String {
        let path = path.split("::").next().unwrap_or("").trim().replace("${VDS_CONFIG_PATH}", &self.folder);
        path.strip_suffix(".pub").map(str::to_string).unwrap_or(path)
    }
}

/// Sessions in a SecureCRT Sessions folder (or a folder in it), or in the Config folder that holds it: one `.ini` file
/// per session, named as the session, and Session Manager's folders as folders. SecureCRT's own files in the top
/// folder aren't sessions: Default.ini (and Default_RDP.ini, Default_LocalShell.ini...), the template new sessions
/// take, and each folder's `__FolderData__.ini`; nor are a cloud sync's "conflicted copy" duplicates. Ordered by folder
/// and name, as Session Manager lists them
pub(crate) fn read_securecrt_folder(picked: &Path) -> Result<Vec<Found>, String> {
    std::fs::read_dir(picked).map_err(|e| format!("Couldn't read {}: {e}", picked.display()))?;
    // the Config folder chosen: its Sessions folder; a Sessions folder (SecureCRT's own): its parent is the Config
    let (folder, config) = if picked.join("Sessions").is_dir() {
        (picked.join("Sessions"), picked.to_path_buf())
    } else {
        (picked.to_path_buf(), picked.parent().map(Path::to_path_buf).unwrap_or_default())
    };
    let mut config = SecureCrtConfig { folder: config.display().to_string(), global_key: String::new() };
    if let Ok(text) = read_text(&Path::new(&config.folder).join("SSH2.ini")) {
        config.global_key = config.key(securecrt_values(&text).get("identity filename v2").map(String::as_str).unwrap_or(""));
    }
    let mut found = Vec::new();
    let mut pending = vec![(folder.clone(), String::new())];
    while let Some((dir, sub)) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let file = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                pending.push((path, if sub.is_empty() { file } else { format!("{sub}/{file}") }));
                continue;
            }
            let Some(name) = file.strip_suffix(".ini").or_else(|| file.strip_suffix(".INI")) else { continue };
            let default = sub.is_empty() && (name.eq_ignore_ascii_case("Default") || name.starts_with("Default_"));
            if name.eq_ignore_ascii_case("__FolderData__") || default || name.contains("conflicted copy") {
                continue;
            }
            let Ok(text) = read_text(&path) else { continue };
            found.push(securecrt_session(name, &sub, &securecrt_values(&text), &config));
        }
    }
    found.sort_by_key(|f| if f.folder.is_empty() { f.name.to_lowercase() } else { format!("{}/{}", f.folder, f.name).to_lowercase() });
    Ok(found)
}

/// One SecureCRT session, from its settings ([`securecrt_values`]); a kind SuperXTerminal can't open (TAPI, a Local
/// Shell) comes back with why it's skipped. For SSH, as extra arguments: its key file as `-i` (its own
/// `Identity Filename V2`, or SSH2.ini's when it's set to Use Global Public Key; see [`SecureCrtConfig::key`]),
/// unless its authentications leave public keys out; agent forwarding (`-A`) and X11 forwarding (`-X`) when they're on;
/// and its port forwards (`-L`). A jump through another session (`Firewall Name` "Session:name") names that session as
/// it's imported, in the same folder of imports. Passwords aren't taken: SecureCRT encrypts them, and the profile or
/// the session's own Password keeps them here
pub(crate) fn securecrt_session(name: &str, folder: &str, values: &std::collections::HashMap<String, String>, config: &SecureCrtConfig) -> Found {
    let get = |key: &str| values.get(key).map(|v| v.trim().to_string()).unwrap_or_default();
    let number = |key: &str| get(key).parse::<u32>().ok();
    let kind = get("protocol name");
    let (protocol, icon, port_key) = match kind.to_ascii_lowercase().as_str() {
        "ssh2" | "ssh1" | "" => ("SSH", "computer", "[ssh2] port"),
        "telnet" | "telnet/ssl" => ("Telnet", "application_xp_terminal", "port"),
        "rlogin" => ("Rlogin", "application_xp_terminal", "port"),
        "raw" => ("Raw", "application_xp_terminal", "port"),
        "serial" => ("Serial", "telephone", ""),
        "rdp" => ("RDP", "monitor", "port"),
        _ => {
            let skip = Some(format!("a {kind} session, which can't be opened here"));
            return Found { folder: folder.to_string(), name: name.to_string(), skip, ..Found::default() };
        }
    };
    let mut f = Found {
        folder: folder.to_string(),
        name: name.to_string(),
        protocol,
        icon,
        host: get("hostname"),
        port: if port_key.is_empty() { 0 } else { number(port_key).and_then(|p| u16::try_from(p).ok()).unwrap_or(0) },
        username: get("username"),
        note: values.get("description").cloned().unwrap_or_default(),
        ..Found::default()
    };
    if protocol == "Serial" {
        // PuTTY's -sercfg speed,data,parity,stop,flow: SecureCRT's parity 0-4 is none, odd, even, mark, space; its
        // stop bits 0-2 are 1, 1.5 and 2; flow is RTS/CTS, DSR/DTR or XON/XOFF, the first that's on
        f.host = get("com port");
        f.username.clear();
        let parity = ["n", "o", "e", "m", "s"].get(number("parity").unwrap_or(0) as usize).copied().unwrap_or("n");
        let stop = ["1", "1.5", "2"].get(number("stop bits").unwrap_or(0) as usize).copied().unwrap_or("1");
        let on = |key: &str| number(key).unwrap_or(0) != 0;
        let flow = if on("cts flow") { "R" } else if on("dsr flow") { "D" } else if on("xon flow") { "X" } else { "N" };
        if let Some(speed) = number("baud rate").filter(|&s| s > 0) {
            f.extra_args = format!("-sercfg {speed},{},{parity},{stop},{flow}", number("data bits").unwrap_or(8));
        }
    }
    if protocol == "SSH" {
        let key = if number("use global public key").unwrap_or(0) != 0 { config.global_key.clone() } else { config.key(&get("identity filename v2")) };
        // a session SecureCRT logs in to by password alone would have its key offered first, a failed attempt
        let auths = get("ssh2 authentications v2").to_ascii_lowercase();
        if auths.is_empty() || auths.contains("publickey") {
            add_arg(&mut f.extra_args, "-i", &key);
        }
        // 1 is on; SecureCRT writes 2 for the global setting, which lives in its own Global Options
        for (setting, switch) in [("enable agent forwarding", "-A"), ("forward x11", "-X")] {
            if number(setting) == Some(1) {
                f.extra_args = [f.extra_args.as_str(), switch].iter().filter(|a| !a.is_empty()).copied().collect::<Vec<_>>().join(" ");
            }
        }
        // Port Forward Table V3, a line each: flag|name|[bind,]port|1 when it names a host|host|port|... With no host
        // named, SecureCRT connects to the SSH server itself, which is localhost there
        for entry in values.get("port forward table v3").map(String::as_str).unwrap_or("").lines() {
            let fields: Vec<&str> = entry.split('|').map(str::trim).collect();
            if let [_, _, listen, has_host, host, port, ..] = fields[..] {
                let (bind, listen) = listen.rsplit_once(',').unwrap_or(("", listen));
                let host = if has_host == "1" && !host.is_empty() { host } else { "localhost" };
                if listen.parse::<u16>().is_ok_and(|p| p > 0) && port.parse::<u16>().is_ok_and(|p| p > 0) {
                    let bind = if bind.is_empty() { String::new() } else { format!("{bind}:") };
                    add_arg(&mut f.extra_args, "-L", &format!("{bind}{listen}:{host}:{port}"));
                }
            }
        }
        let firewall = get("firewall name");
        if let Some(session) = firewall.strip_prefix("Session:").or_else(|| firewall.strip_prefix("session:")) {
            let session = session.trim().replace('\\', "/");
            f.jump_host = format!("ImportedFromSecureCRT/{}", session.trim_matches('/'));
        }
    }
    f
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importing::{plan_added, Plan};
    use crate::sessions::Store;

    /// A SecureCRT session file as SecureCRT writes it: a UTF-8 byte order mark, then `T:"Name"=value` lines
    fn securecrt_file(path: &Path, lines: &[&str]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("\u{feff}{}\r\n", lines.join("\r\n"))).unwrap();
    }

    #[test]
    fn reads_a_securecrt_sessions_folder_with_its_folders() {
        let dir = std::env::temp_dir().join(format!("sxt-securecrt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        // SecureCRT's own files beside the sessions, which aren't sessions to import
        securecrt_file(&dir.join("Default.ini"), &[r#"S:"Protocol Name"=SSH2"#, r#"S:"Hostname"="#]);
        securecrt_file(&dir.join("Default_RDP.ini"), &[r#"S:"Protocol Name"=RDP"#]);
        securecrt_file(&dir.join("__FolderData__.ini"), &[r#"Z:"Session List V2"=00000001"#, " openmediavault.localdomain"]);
        // an SSH session, its port in hex, a description over two lines and a key file of its own
        securecrt_file(&dir.join("openmediavault.localdomain.ini"), &[
            r#"S:"Username"=root"#,
            r#"D:"Is Session"=00000001"#,
            r#"S:"Protocol Name"=SSH2"#,
            r#"S:"Hostname"=openmediavault.localdomain"#,
            r#"S:"Firewall Name"=None"#,
            r#"D:"[SSH2] Port"=00000898"#,
            r#"D:"Use Global Public Key"=00000000"#,
            r#"S:"Identity Filename V2"=C:\Users\me\.ssh\omv key::rawkey"#,
            r#"Z:"Description"=00000002"#,
            " The NAS",
            " in the rack",
            r#"B:"Keyword Set"=00000004"#,
            " 01 02 03 04",
        ]);
        // a serial console: 9600 baud, 8 data bits, no parity, 1 stop bit, RTS/CTS
        securecrt_file(&dir.join("Serial-COM3.ini"), &[
            r#"S:"Protocol Name"=Serial"#,
            r#"S:"Com Port"=COM3"#,
            r#"D:"Baud Rate"=00002580"#,
            r#"D:"Data Bits"=00000008"#,
            r#"D:"Parity"=00000000"#,
            r#"D:"Stop Bits"=00000000"#,
            r#"D:"CTS Flow"=00000001"#,
            r#"D:"XON Flow"=00000000"#,
        ]);
        // in a folder: Telnet on its own port, and SSH through another session (SecureCRT's Firewall "Session:...")
        securecrt_file(&dir.join("Lab").join("router.ini"), &[r#"S:"Protocol Name"=Telnet"#, r#"S:"Hostname"=10.0.0.1"#, r#"D:"Port"=00000017"#]);
        securecrt_file(&dir.join("Lab").join("Core").join("sw1.ini"), &[
            r#"S:"Protocol Name"=SSH2"#,
            r#"S:"Hostname"=10.0.0.2"#,
            r#"S:"Username"=admin"#,
            r#"D:"[SSH2] Port"=00000016"#,
            r#"S:"Firewall Name"=Session:openmediavault.localdomain"#,
            r#"D:"Use Global Public Key"=00000001"#,
            r#"S:"Identity Filename V2"=C:\ignored\while\the\global\key\is\used"#,
        ]);
        // a kind SuperXTerminal can't open: skipped, saying why
        securecrt_file(&dir.join("modem.ini"), &[r#"S:"Protocol Name"=TAPI"#]);

        let found = read_securecrt_folder(&dir).unwrap();
        let plan = plan_added(found, "ImportedFromSecureCRT", Store::Private);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(plan.summary().skipped, ["ImportedFromSecureCRT/modem: a TAPI session, which can't be opened here"]);
        let plan = Plan { rows: plan.rows.into_iter().filter(|r| r.skip.is_none()).collect(), ..plan };
        let ids: Vec<&str> = plan.rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, [
            "ImportedFromSecureCRT/Lab/Core/sw1",
            "ImportedFromSecureCRT/Lab/router",
            "ImportedFromSecureCRT/openmediavault.localdomain",
            "ImportedFromSecureCRT/Serial-COM3",
        ]);
        let value = |row: usize, name: &str| plan.rows[row].values.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()).unwrap_or_default();

        // SSH: host, port 2200, user, the key file as -i (without SecureCRT's ::rawkey), the description as the note
        assert_eq!((value(2, "Proto"), value(2, "Host"), value(2, "Port"), value(2, "Username")), ("SSH".into(), "openmediavault.localdomain".into(), "2200".into(), "root".into()));
        assert_eq!(value(2, "ExtraArgs"), r#"-i "C:\Users\me\.ssh\omv key""#);
        assert_eq!(value(2, "Note"), "The NAS\nin the rack");
        assert_eq!(value(2, "JumpHost"), "", "Firewall None: no jump host");

        // through another session: that session as imported, and the global key (SecureCRT's own) not taken
        assert_eq!(value(0, "JumpHost"), "ImportedFromSecureCRT/openmediavault.localdomain");
        assert_eq!(value(0, "ExtraArgs"), "");
        assert_eq!((value(0, "Port"), value(0, "Username")), ("22".into(), "admin".into()));

        // Telnet on port 23, and the serial line with its settings as PuTTY's -sercfg
        assert_eq!((value(1, "Proto"), value(1, "Host"), value(1, "Port")), ("Telnet".into(), "10.0.0.1".into(), "23".into()));
        assert_eq!((value(3, "Proto"), value(3, "Host"), value(3, "ExtraArgs")), ("Serial".into(), "COM3".into(), "-sercfg 9600,8,n,1,R".into()));
    }

    /// This computer's own SecureCRT sessions, as the import would add them: for checking against a real SecureCRT,
    /// on Windows and the Mac. Not run with the rest, as most computers have none:
    /// `cargo test --lib this_computers_securecrt -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn this_computers_securecrt_sessions_as_imported() {
        let folder = securecrt_folder();
        let plan = plan_added(read_securecrt_folder(&folder).unwrap(), "ImportedFromSecureCRT", Store::Private);
        println!("{}: {} sessions", folder.display(), plan.rows.len());
        for row in &plan.rows {
            let shown: Vec<String> = row.values.iter().filter(|(n, v)| !v.is_empty() && n != "SessionName" && n != "ImageKey").map(|(n, v)| format!("{n}={v:?}")).collect();
            println!("  {}  {}", row.id, shown.join(" "));
        }
        assert!(plan.error.is_none(), "{:?}", plan.error);
    }

    #[test]
    fn takes_securecrt_keys_forwards_and_forwarding_from_its_config_folder() {
        let dir = std::env::temp_dir().join(format!("sxt-securecrt-config-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let config = dir.join("Config");
        let sessions = config.join("Sessions");
        // SecureCRT's global key, in SSH2.ini, its path written from the Config folder
        securecrt_file(&config.join("SSH2.ini"), &[r#"S:"Identity Filename V2"=${VDS_CONFIG_PATH}\Keys\Identity::rawkey"#]);
        securecrt_file(&sessions.join("global.ini"), &[r#"S:"Protocol Name"=SSH2"#, r#"S:"Hostname"=g"#, r#"D:"Use Global Public Key"=00000001"#]);
        // its own key, given as the public half; agent and X11 forwarding; two local forwards, one to the server itself
        securecrt_file(&sessions.join("own.ini"), &[
            r#"S:"Protocol Name"=SSH2"#,
            r#"S:"Hostname"=o"#,
            r#"D:"Use Global Public Key"=00000000"#,
            r#"S:"Identity Filename V2"=C:\keys\me.pub"#,
            r#"D:"Enable Agent Forwarding"=00000001"#,
            r#"D:"Forward X11"=00000001"#,
            r#"Z:"Port Forward Table V3"=00000002"#,
            " 0|web|8080|1|intranet|80|",
            " 0|mail|127.0.0.1,2525|0||25|",
        ]);
        // a key that can't be used: the session logs in by password only
        securecrt_file(&sessions.join("password.ini"), &[
            r#"S:"Protocol Name"=SSH2"#,
            r#"S:"Hostname"=p"#,
            r#"D:"Use Global Public Key"=00000000"#,
            r#"S:"Identity Filename V2"=C:\keys\unused"#,
            r#"S:"SSH2 Authentications V2"=password,keyboard-interactive"#,
            r#"D:"Enable Agent Forwarding"=00000002"#,
        ]);
        // a cloud sync's duplicate, not a session of its own; and a kind that can't be opened, said as skipped
        securecrt_file(&sessions.join("own (conflicted copy 2026-01-01).ini"), &[r#"S:"Protocol Name"=SSH2"#, r#"S:"Hostname"=o"#]);
        securecrt_file(&sessions.join("modem.ini"), &[r#"S:"Protocol Name"=TAPI"#]);

        // the Config folder or its Sessions folder: the same sessions, the key paths from the Config folder
        for picked in [&config, &sessions] {
            let plan = plan_added(read_securecrt_folder(picked).unwrap(), "ImportedFromSecureCRT", Store::Private);
            let row = |id: &str| plan.rows.iter().find(|r| r.id == format!("ImportedFromSecureCRT/{id}")).unwrap_or_else(|| panic!("no {id}: {:?}", plan.rows));
            let args = |id: &str| row(id).values.iter().find(|(n, _)| n == "ExtraArgs").map(|(_, v)| v.clone()).unwrap_or_default();
            assert_eq!(plan.rows.len(), 4, "{picked:?}: {:?}", plan.rows.iter().map(|r| &r.id).collect::<Vec<_>>());
            let global = format!("{}\\Keys\\Identity", config.display());
            let global = if global.contains(' ') { format!("-i \"{global}\"") } else { format!("-i {global}") };
            assert_eq!(args("global"), global, "{picked:?}");
            assert_eq!(args("own"), r"-i C:\keys\me -A -X -L 8080:intranet:80 -L 127.0.0.1:2525:localhost:25");
            assert_eq!(args("password"), "");
            assert_eq!(row("modem").skip.as_deref(), Some("a TAPI session, which can't be opened here"));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_securecrt_folder_with_no_sessions_says_so() {
        let dir = std::env::temp_dir().join(format!("sxt-securecrt-empty-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        securecrt_file(&dir.join("Default.ini"), &[r#"S:"Protocol Name"=SSH2"#]);
        let plan = plan_added(read_securecrt_folder(&dir).unwrap(), "ImportedFromSecureCRT", Store::Private);
        assert_eq!(plan.error.as_deref(), Some("No sessions were found in it."));
        std::fs::remove_dir_all(&dir).unwrap();
        assert!(read_securecrt_folder(&dir).unwrap_err().contains("Couldn't read"));
    }
}
