//! Import from PuTTY and its relatives: PuTTY's and KiTTY's saved sessions (the registry on Windows, a folder of
//! session files elsewhere, as KiTTY's portable mode and PuTTY for Unix keep them), and a PuTTY Connection
//! Manager export with its folders. A saved session's name is stored URL-encoded, and decoded here.

use std::path::Path;

// the registry is read on Windows only; elsewhere PuTTY's sessions are files
#[cfg(windows)]
use super::registry_values;
use super::{protocol_named, Found};

/// A name as PuTTY stores it in the registry and in file names: `%20` and other `%xx` decoded
pub(crate) fn url_decode(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Some(byte) = std::str::from_utf8(&bytes[i + 1..i + 3]).ok().and_then(|hex| u8::from_str_radix(hex, 16).ok()) {
                out.push(byte);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

/// A PuTTY or KiTTY saved session from its settings (lower-cased names): HostName, PortNumber, Protocol and UserName.
/// Its name is its profile too (PuttySession), as SuperXPuTTY's import makes it, so SuperXPuTTY opens it with every
/// setting PuTTY has for it.
pub(crate) fn putty_session(name: &str, folder: &str, values: &std::collections::HashMap<String, String>) -> Found {
    let get = |key: &str| values.get(key).map(|v| v.trim().to_string()).unwrap_or_default();
    let protocol = protocol_named(&get("protocol"));
    let port = get("portnumber");
    // the registry's REG_DWORD reads as 0x16 through reg.exe; files have it in decimal
    let port = match port.strip_prefix("0x") {
        Some(hex) => u16::from_str_radix(hex, 16).ok(),
        None => port.parse().ok(),
    };
    let icon = match protocol {
        "Telnet" | "Rlogin" | "Raw" => "application_xp_terminal",
        "Serial" => "telephone",
        _ => "computer",
    };
    let mut found = Found {
        folder: folder.to_string(),
        name: name.to_string(),
        protocol,
        host: get("hostname"),
        port: port.unwrap_or(if protocol == "SSH" { 22 } else { 0 }),
        username: get("username"),
        icon,
        profile: name.to_string(),
        ..Found::default()
    };
    if protocol == "Serial" {
        // a serial session's line is SerialLine, not HostName
        found.host = get("serialline");
        found.port = 0;
    }
    found
}

/// Sessions in a folder of PuTTY session files (PuTTY Portable's or portable KiTTY's `Sessions`, one file per
/// session named as in the registry, with `Name\Value\` lines), as SuperXPuTTY's LoadSessionsFromFolder. PuTTY for the
/// Mac and Linux keeps `Name=Value` lines in ~/.putty/sessions, which are read too.
pub(crate) fn read_putty_folder(folder: &Path, into: &str) -> Result<Vec<Found>, String> {
    let entries = std::fs::read_dir(folder).map_err(|e| format!("Couldn't read {}: {e}", folder.display()))?;
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let Ok(text) = std::fs::read(&path).map(|b| String::from_utf8_lossy(&b).to_string()) else { continue };
        let mut values = std::collections::HashMap::new();
        for line in text.lines() {
            let split = line.find(['\\', '=']);
            if let Some(at) = split.filter(|&at| at > 0) {
                let value = line[at + 1..].trim_end_matches('\\');
                values.insert(line[..at].to_ascii_lowercase(), value.to_string());
            }
        }
        if values.is_empty() {
            continue;
        }
        let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        // KiTTY may add an extension (kitty.ini's fileextension); PuTTY's names have none
        let file = file.strip_suffix(".ktx").unwrap_or(&file);
        found.push(putty_session(&url_decode(file), into, &values));
    }
    found.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(found)
}

/// The registry key PuTTY keeps its saved sessions in
#[cfg(windows)]
pub(crate) const PUTTY_SESSIONS: &str = "HKEY_CURRENT_USER\\Software\\SimonTatham\\PuTTY\\Sessions";

/// The registry key KiTTY keeps its saved sessions in (when it isn't portable)
#[cfg(windows)]
pub(crate) const KITTY_SESSIONS: &str = "HKEY_CURRENT_USER\\Software\\9bis.com\\KiTTY\\Sessions";

/// PuTTY's and KiTTY's saved sessions, as SuperXPuTTY's GetAllSessionsFromPuTTY: from the registry on Windows (PuTTY's
/// in the import's folder, KiTTY's in a KiTTY folder inside it), from ~/.putty/sessions elsewhere
pub(crate) fn read_putty_saved() -> Result<Vec<Found>, String> {
    #[cfg(windows)]
    {
        let mut found = Vec::new();
        for (key, folder) in [(PUTTY_SESSIONS, ""), (KITTY_SESSIONS, "KiTTY")] {
            let mut sessions: Vec<(String, std::collections::HashMap<String, String>)> = Vec::new();
            for (name, value_name, value) in registry_values(key) {
                if name.is_empty() {
                    continue;
                }
                match sessions.iter_mut().find(|(n, _)| *n == name) {
                    Some((_, values)) => {
                        values.insert(value_name, value);
                    }
                    None => sessions.push((name, std::collections::HashMap::from([(value_name, value)]))),
                }
            }
            found.extend(sessions.iter().map(|(name, values)| putty_session(&url_decode(name), folder, values)));
        }
        Ok(found)
    }
    #[cfg(not(windows))]
    {
        let folder = dirs::home_dir().unwrap_or_default().join(".putty/sessions");
        if !folder.is_dir() {
            return Err("PuTTY has no saved sessions here (~/.putty/sessions).".into());
        }
        read_putty_folder(&folder, "")
    }
}

/// Sessions in a PuTTY Connection Manager export, as SuperXPuTTY's GetAllSessionsFromPuTTYCM: each
/// `<connection type="PuTTY">` with its `connection_info` (name, host, port, protocol, session) and `login/login`,
/// in the folders (`<container type="folder" name=...>`) it's in
pub(crate) fn read_puttycm(text: &str) -> Result<Vec<Found>, String> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(text);
    let mut found = Vec::new();
    // each open container: its name when it's a folder, None for others
    let mut containers: Vec<Option<String>> = Vec::new();
    // inside a PuTTY connection: its values so far and the elements open in it
    let mut connection: Option<std::collections::HashMap<String, String>> = None;
    let mut path: Vec<String> = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = e.name().as_ref().to_string();
                let attr = |wanted: &str| {
                    e.attributes()
                        .flatten()
                        .find(|a| a.key.as_ref() == wanted)
                        .and_then(|a| a.normalized_value(quick_xml::XmlVersion::Implicit1_0).ok())
                        .map(|v| v.to_string())
                };
                if connection.is_some() {
                    path.push(name);
                } else if name == "container" {
                    containers.push((attr("type").as_deref() == Some("folder")).then(|| attr("name").unwrap_or_default()));
                } else if name == "connection" && attr("type").as_deref() == Some("PuTTY") {
                    connection = Some(std::collections::HashMap::new());
                    path.clear();
                }
            }
            Ok(Event::Text(t)) => {
                if let (Some(values), Some(_)) = (connection.as_mut(), path.last()) {
                    values.entry(path.join("/")).or_default().push_str(&t.xml_content(quick_xml::XmlVersion::Implicit1_0));
                }
            }
            // &amp; and the like come on their own, between the text around them
            Ok(Event::GeneralRef(r)) => {
                if let (Some(values), Some(_)) = (connection.as_mut(), path.last()) {
                    let named = r.xml_content(quick_xml::XmlVersion::Implicit1_0);
                    let character = match named.as_ref() {
                        "amp" => "&",
                        "lt" => "<",
                        "gt" => ">",
                        "quot" => "\"",
                        "apos" => "'",
                        _ => "",
                    };
                    values.entry(path.join("/")).or_default().push_str(character);
                }
            }
            Ok(Event::End(e)) => {
                let name = e.name().as_ref().to_string();
                if connection.is_some() && !path.is_empty() {
                    path.pop();
                } else if let Some(values) = connection.take().filter(|_| name == "connection") {
                    let get = |k: &str| values.get(k).map(|v| v.trim().to_string()).unwrap_or_default();
                    let protocol = protocol_named(&get("connection_info/protocol"));
                    let folder: Vec<String> = containers.iter().flatten().filter(|n| !n.is_empty()).cloned().collect();
                    found.push(Found {
                        folder: folder.join("/"),
                        name: get("connection_info/name"),
                        protocol,
                        host: get("connection_info/host"),
                        port: get("connection_info/port").parse().unwrap_or(0),
                        username: get("login/login"),
                        icon: if protocol == "SSH" { "computer" } else { "application_xp_terminal" },
                        profile: get("connection_info/session"),
                        ..Found::default()
                    });
                } else if name == "container" {
                    containers.pop();
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(format!("Not a PuTTY CM export: {e}")),
            _ => {}
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_decoded_as_putty_stores_them() {
        assert_eq!(url_decode("Default%20Settings"), "Default Settings");
        assert_eq!(url_decode("core%2Fsw1"), "core/sw1");
        assert_eq!(url_decode("50%"), "50%");
        assert_eq!(url_decode("caf%C3%A9"), "café");
    }

    #[test]
    fn reads_putty_session_files_of_both_kinds() {
        let dir = std::env::temp_dir().join(format!("sxt-putty-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // PuTTY Portable / portable KiTTY: Name\Value\ lines
        std::fs::write(dir.join("lab%20switch"), "HostName\\10.0.0.1\\\r\nPortNumber\\2222\\\r\nProtocol\\ssh\\\r\nUserName\\admin\\\r\n").unwrap();
        // PuTTY on the Mac and Linux: Name=Value lines
        std::fs::write(dir.join("console"), "HostName=\nProtocol=serial\nSerialLine=/dev/cu.usbserial\n").unwrap();
        std::fs::write(dir.join("router"), "HostName=r1\nProtocol=telnet\nPortNumber=23\n").unwrap();
        let found = read_putty_folder(&dir, "").unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["console", "lab switch", "router"]);
        let lab = &found[1];
        assert_eq!((lab.protocol, lab.host.as_str(), lab.port, lab.username.as_str(), lab.profile.as_str()), ("SSH", "10.0.0.1", 2222, "admin", "lab switch"));
        assert_eq!((found[0].protocol, found[0].host.as_str(), found[0].port), ("Serial", "/dev/cu.usbserial", 0));
        assert_eq!((found[2].protocol, found[2].port), ("Telnet", 23));
    }

    #[test]
    fn reads_a_putty_cm_export_with_its_folders() {
        let xml = r#"<?xml version="1.0"?>
<root type="database" name="db">
  <container type="folder" name="Lab &amp; Test">
    <container type="folder" name="Core">
      <connection type="PuTTY" name="sw1">
        <connection_info><name>sw1</name><protocol>SSH</protocol><host>10.1.1.1</host><port>22</port><session>Default Settings</session></connection_info>
        <login><login>netadmin</login><password /></login>
      </connection>
    </container>
    <connection type="RDP" name="skipped"><connection_info><name>skipped</name></connection_info></connection>
  </container>
  <connection type="PuTTY" name="top">
    <connection_info><name>top</name><protocol>Telnet</protocol><host>t1</host><port>23</port><session>lab</session></connection_info>
    <login><login></login></login>
  </connection>
</root>"#;
        let found = read_puttycm(xml).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].folder, "Lab & Test/Core");
        assert_eq!((found[0].name.as_str(), found[0].host.as_str(), found[0].port, found[0].username.as_str()), ("sw1", "10.1.1.1", 22, "netadmin"));
        assert_eq!((found[1].folder.as_str(), found[1].protocol, found[1].profile.as_str()), ("", "Telnet", "lab"));
    }

    #[test]
    fn registry_ports_read_as_hex() {
        let values = std::collections::HashMap::from([("hostname".to_string(), "h".to_string()), ("portnumber".to_string(), "0x16".to_string())]);
        assert_eq!(putty_session("s", "", &values).port, 22);
    }
}
