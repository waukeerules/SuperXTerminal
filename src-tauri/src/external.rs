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

/// Open FileZilla (or FileZilla Pro) at an sftp:// address
pub fn open_filezilla(host: &str, port: u16, username: &str) -> Result<String, String> {
    let app = ["FileZilla", "FileZilla Pro"]
        .into_iter()
        .find(|name| std::path::Path::new(&format!("/Applications/{name}.app")).exists())
        .ok_or("FileZilla isn't installed in Applications.")?;
    let user = if username.is_empty() { String::new() } else { format!("{username}@") };
    let port = if port != 0 { port } else { 22 };
    let url = format!("sftp://{user}{host}:{port}");
    let status = std::process::Command::new("open").args(["-a", app, "--args", &url]).status().map_err(|e| e.to_string())?;
    if status.success() { Ok(format!("Opened {url} in {app}")) } else { Err(format!("{app} didn't start")) }
}

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
    fn quotes_words_for_the_shell() {
        assert_eq!(shell_line(&["ssh".into(), "-o".into(), "Ciphers=+aes256-cbc".into(), "my host".into(), "it's".into()]), "ssh -o Ciphers=+aes256-cbc 'my host' 'it'\\''s'");
    }
}
