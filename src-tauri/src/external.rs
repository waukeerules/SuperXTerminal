//! Sessions that open in another app, as SuperXPuTTY's launchers do on Windows: RDP in Windows App (or Microsoft
//! Remote Desktop) through a .rdp file with the session's options, VNC in Screen Sharing, Connect external in a
//! Terminal window, and FileZilla with an sftp:// address.

use std::path::PathBuf;

use crate::rdp_options::RdpOptions;
use crate::sessions::StoredSession;

/// A .rdp file's settings for a session: its address and username, then the options it connects with (the
/// defaults, Settings > Remote Desktop's, its own RdpOptions) as the matching .rdp settings
pub fn rdp_file(session: &StoredSession, host: &str, username: &str, defaults: &str) -> String {
    let port = session.port();
    let address = if port != 0 && port != 3389 { format!("{host}:{port}") } else { host.to_string() };
    let mut lines = vec![format!("full address:s:{address}")];
    if !username.is_empty() {
        lines.push(format!("username:s:{username}"));
    }
    let o = RdpOptions::effective(defaults, session.get("RdpOptions"));
    if o.display.as_deref() == Some("Fixed") {
        lines.push("screen mode id:i:1".into());
        if let (Some(w), Some(h)) = (o.width, o.height) {
            lines.push(format!("desktopwidth:i:{w}"));
            lines.push(format!("desktopheight:i:{h}"));
        }
    } else {
        lines.push("dynamic resolution:i:1".into());
    }
    for (value, setting) in [
        (o.smart_sizing, "smart sizing"),
        (o.clipboard, "redirectclipboard"),
        (o.printers, "redirectprinters"),
        (o.smart_cards, "redirectsmartcards"),
        (o.microphone, "audiocapturemode"),
        (o.console, "administrative session"),
    ] {
        if let Some(on) = value {
            lines.push(format!("{setting}:i:{}", on as u8));
        }
    }
    if let Some(on) = o.drives {
        lines.push(format!("drivestoredirect:s:{}", if on { "*" } else { "" }));
    }
    for (value, setting) in [(o.color_depth, "session bpp"), (o.audio, "audiomode"), (o.keyboard, "keyboardhook"), (o.authentication, "authentication level")] {
        if let Some(value) = value {
            lines.push(format!("{setting}:i:{value}"));
        }
    }
    if let Some(gateway) = o.gateway.filter(|g| !g.is_empty()) {
        lines.push(format!("gatewayhostname:s:{gateway}"));
        lines.push("gatewayusagemethod:i:1".into());
    }
    lines.join("\r\n") + "\r\n"
}

/// Write a session's .rdp file (in the temporary folder) and open it in the Mac's Remote Desktop app
pub fn open_rdp(session: &StoredSession, host: &str, username: &str, defaults: &str) -> Result<String, String> {
    let folder = std::env::temp_dir().join("SuperXTerminal").join("rdp");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let name: String = session.get("SessionName").chars().map(|c| if c.is_alphanumeric() || " -_.".contains(c) { c } else { '_' }).collect();
    let path: PathBuf = folder.join(format!("{}.rdp", if name.trim().is_empty() { "session" } else { name.trim() }));
    std::fs::write(&path, rdp_file(session, host, username, defaults)).map_err(|e| e.to_string())?;
    let status = std::process::Command::new("open").arg(&path).status().map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("No app opens .rdp files on this Mac: install Windows App from the App Store.".into());
    }
    Ok(format!("Opened {} in Windows App", session.get("SessionName")))
}

/// Open a VNC session in Screen Sharing
pub fn open_vnc(host: &str, port: u16, username: &str) -> Result<String, String> {
    let user = if username.is_empty() { String::new() } else { format!("{username}@") };
    let port = if port != 0 && port != 5900 { format!(":{port}") } else { String::new() };
    open_url(&format!("vnc://{user}{host}{port}"))?;
    Ok(format!("Opened {host} in Screen Sharing"))
}

/// Run a command line in a new Terminal window (Connect external)
pub fn open_in_terminal(command_line: &str) -> Result<(), String> {
    let script = format!(
        "tell application \"Terminal\"\nactivate\ndo script \"{}\"\nend tell",
        command_line.replace('\\', "\\\\").replace('"', "\\\"")
    );
    let status = std::process::Command::new("osascript").args(["-e", &script]).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err("Terminal didn't open the session (allow SuperXTerminal to control Terminal in System Settings > Privacy & Security > Automation).".into()) }
}

/// Open FileZilla (or FileZilla Pro) at an sftp:// address, as SuperXPuTTY's openFileZilla: the session's remote
/// path on the address, its local path as `--local`. The password goes on the address only when Settings allows it
/// (it shows in the process list); otherwise FileZilla asks for it, or uses one it saved.
pub fn open_filezilla(host: &str, port: u16, username: &str, password: Option<&str>, local: &str, remote: &str) -> Result<String, String> {
    let app = ["FileZilla", "FileZilla Pro"]
        .into_iter()
        .find(|name| std::path::Path::new(&format!("/Applications/{name}.app")).exists())
        .ok_or("FileZilla isn't installed in Applications.")?;
    let url = filezilla_url(host, port, username, None, remote);
    let with_password = matches!(password, Some(pw) if !pw.is_empty()) && !username.is_empty();
    let address = if with_password { filezilla_url(host, port, username, password, remote) } else { url.clone() };
    // -n: a new FileZilla each time, as SuperXPuTTY starts one (a running one would drop the address)
    let mut args = vec!["-n".to_string(), "-a".to_string(), app.to_string(), "--args".to_string(), address];
    if !with_password {
        // log in with a key (ssh-agent, FileZilla's own) and ask for a password only if the server wants one;
        // otherwise FileZilla asks before trying, which a key login doesn't need
        args.push("--logontype=interactive".into());
    }
    if !local.trim().is_empty() {
        args.push(format!("--local={}", local.trim()));
    }
    let status = std::process::Command::new("open").args(&args).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(format!("Opened {url} in {app}")) } else { Err(format!("{app} didn't start")) }
}

/// The username ssh itself would log in with (`ssh -G`: ~/.ssh/config's User for the host, else this Mac's account),
/// for a session that names none; FileZilla otherwise asks for one
pub fn ssh_username(host: &str, port: u16) -> Option<String> {
    let mut command = std::process::Command::new("ssh");
    command.arg("-G");
    if port != 0 {
        command.args(["-p", &port.to_string()]);
    }
    let output = command.arg(host).output().ok()?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find_map(|line| line.strip_prefix("user ").map(|u| u.trim().to_string()))
        .filter(|u| !u.is_empty())
}

/// The sftp:// address: the user and password (URL-escaped), host, port (22 when the session has none) and remote
/// path
fn filezilla_url(host: &str, port: u16, username: &str, password: Option<&str>, remote: &str) -> String {
    let escape = |text: &str| -> String {
        text.bytes()
            .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
            .collect()
    };
    let user = match (username, password) {
        ("", _) => String::new(),
        (u, Some(pw)) => format!("{}:{}@", escape(u), escape(pw)),
        (u, None) => format!("{}@", escape(u)),
    };
    let port = if port != 0 { port } else { 22 };
    let remote = remote.trim();
    let path = if remote.is_empty() { String::new() } else if remote.starts_with('/') { remote.to_string() } else { format!("/{remote}") };
    format!("sftp://{user}{host}:{port}{path}")
}

/// Open a URL with the app the Mac has for it (`open`), or say nothing here does
fn open_url(url: &str) -> Result<(), String> {
    let status = std::process::Command::new("open").arg(url).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(()) } else { Err(format!("Nothing on this Mac opens {url}")) }
}

/// A command line for a shell, each word quoted where it needs it
pub fn shell_line(words: &[String]) -> String {
    words
        .iter()
        .map(|w| if !w.is_empty() && w.chars().all(|c| c.is_ascii_alphanumeric() || "-_.,:/@=+%".contains(c)) { w.clone() } else { format!("'{}'", w.replace('\'', "'\\''")) })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_rdp_options_as_rdp_settings() {
        let session = StoredSession {
            attributes: vec![
                ("SessionName".into(), "Win".into()),
                ("Port".into(), "3390".into()),
                ("RdpOptions".into(), "Display=Fixed;Width=1280;Height=800;Clipboard=0;Gateway=gw.example.com".into()),
            ],
        };
        let file = rdp_file(&session, "10.0.0.5", "me", "ColorDepth=16");
        for line in ["full address:s:10.0.0.5:3390", "username:s:me", "desktopwidth:i:1280", "redirectclipboard:i:0", "gatewayhostname:s:gw.example.com", "session bpp:i:16"] {
            assert!(file.contains(line), "{line} in {file}");
        }
    }

    #[test]
    fn writes_filezilla_addresses() {
        assert_eq!(filezilla_url("10.0.0.5", 0, "", None, ""), "sftp://10.0.0.5:22");
        assert_eq!(filezilla_url("h", 2222, "me@corp", None, "/var/log"), "sftp://me%40corp@h:2222/var/log");
        assert_eq!(filezilla_url("h", 22, "me", None, "home/me"), "sftp://me@h:22/home/me");
        assert_eq!(filezilla_url("h", 22, "me", Some("p@ss:w/rd"), ""), "sftp://me:p%40ss%3Aw%2Frd@h:22");
    }

    #[test]
    fn quotes_words_for_the_shell() {
        assert_eq!(shell_line(&["ssh".into(), "-o".into(), "Ciphers=+aes256-cbc".into(), "my host".into(), "it's".into()]), "ssh -o Ciphers=+aes256-cbc 'my host' 'it'\\''s'");
    }
}
