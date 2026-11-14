//! PuTTY's saved sessions as profiles, from a `reg export` of its registry key (Settings > Profiles, Import…):
//!
//! ```text
//! reg export HKCU\Software\SimonTatham\PuTTY\Sessions putty.reg
//! ```
//!
//! A SuperXPuTTY session names a PuTTY saved session as its profile (PuttySession), so each saved session becomes a
//! profile of that name, with what SuperXTerminal's profiles hold: the host and login, keep-alives, tunnels, agent and
//! X11 forwarding, compression, the keyboard, font, scrollback, character set, cursor, bell and colours. What profiles
//! have no place for (proxies, PuTTY's own algorithm lists, window size) is left out. KiTTY's sessions, under
//! 9bis.com\KiTTY, are read the same way.
//!
//! regedit writes UTF-16 (with a byte order mark, "Windows Registry Editor Version 5.00"), REGEDIT4 files are ANSI;
//! both are read. PuTTY's own Default Settings comes in as "PuTTY Default Settings", so SuperXTerminal's isn't touched.

use crate::profiles::{Forward, Profile};

/// What PuTTY's own "Default Settings" comes in as
const PUTTY_DEFAULTS: &str = "PuTTY Default Settings";

/// The profiles in a .reg file's PuTTY (or KiTTY) sessions, in the file's order; an error when it isn't a registry
/// export or holds none
pub fn read(bytes: &[u8]) -> Result<Vec<Profile>, String> {
    let text = decode(bytes);
    if !text.trim_start().starts_with("Windows Registry Editor") && !text.trim_start().starts_with("REGEDIT4") {
        return Err("Not a registry export: make one with reg export HKCU\\Software\\SimonTatham\\PuTTY\\Sessions putty.reg".into());
    }
    let mut profiles = Vec::new();
    let mut current: Option<(String, Vec<(String, Value)>)> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some(key) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if let Some((name, values)) = current.take() {
                profiles.push(profile_of(&name, &values));
            }
            current = session_name(key).map(|name| (name, Vec::new()));
        } else if let Some((_, values)) = current.as_mut()
            && let Some(value) = value_of(line)
        {
            values.push(value);
        }
    }
    if let Some((name, values)) = current {
        profiles.push(profile_of(&name, &values));
    }
    if profiles.is_empty() {
        return Err("There are no PuTTY sessions in the file: export HKCU\\Software\\SimonTatham\\PuTTY\\Sessions.".into());
    }
    Ok(profiles)
}

/// The file's text: UTF-16 little-endian after its byte order mark (regedit's), else UTF-8 or ANSI (REGEDIT4)
fn decode(bytes: &[u8]) -> String {
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        return String::from_utf16_lossy(&units);
    }
    let bytes = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes);
    String::from_utf8(bytes.to_vec()).unwrap_or_else(|_| bytes.iter().map(|b| char::from(*b)).collect())
}

/// A saved session's name from its key (…\SimonTatham\PuTTY\Sessions\Lab%20switch), with PuTTY's %XX escapes
/// undone; None for any other key (the Sessions key itself, SshHostKeys, Jumplist...)
fn session_name(key: &str) -> Option<String> {
    let lower = key.to_ascii_lowercase();
    let at = ["\\simontatham\\putty\\sessions\\", "\\9bis.com\\kitty\\sessions\\"].iter().find_map(|k| lower.find(k).map(|i| i + k.len()))?;
    let escaped = &key[at..];
    if escaped.is_empty() || escaped.contains('\\') {
        return None;
    }
    let mut name = Vec::new();
    let bytes = escaped.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok()).and_then(|h| u8::from_str_radix(h, 16).ok());
        if bytes[i] == b'%' && let Some(b) = hex {
            name.push(b);
            i += 3;
        } else {
            name.push(bytes[i]);
            i += 1;
        }
    }
    let name = String::from_utf8_lossy(&name).trim().to_string();
    Some(if name.eq_ignore_ascii_case(crate::profiles::DEFAULT_PROFILE) { PUTTY_DEFAULTS.to_string() } else { name })
}

/// A value in a session's key: a string or a number
#[derive(Clone, Debug, PartialEq)]
enum Value {
    Text(String),
    Number(u32),
}

/// One `"Name"="text"` or `"Name"=dword:0000000a` line; None for others (hex values, their continuation lines)
fn value_of(line: &str) -> Option<(String, Value)> {
    let rest = line.strip_prefix('"')?;
    let end = rest.find("\"=")?;
    let name = rest[..end].to_string();
    let data = &rest[end + 2..];
    if let Some(hex) = data.strip_prefix("dword:") {
        return Some((name, Value::Number(u32::from_str_radix(hex.trim(), 16).ok()?)));
    }
    let quoted = data.strip_prefix('"')?.strip_suffix('"')?;
    // regedit escapes \ and " in strings
    let mut text = String::with_capacity(quoted.len());
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        text.push(if c == '\\' { chars.next().unwrap_or('\\') } else { c });
    }
    Some((name, Value::Text(text)))
}

/// A saved session's values as a profile of its name; what PuTTY has at its default is left empty, so it falls back
/// to Default Settings as any profile's does
fn profile_of(name: &str, values: &[(String, Value)]) -> Profile {
    let text = |key: &str| values.iter().find_map(|(k, v)| match v {
        Value::Text(t) if k.eq_ignore_ascii_case(key) => Some(t.trim().to_string()),
        _ => None,
    }).unwrap_or_default();
    let number = |key: &str| values.iter().find_map(|(k, v)| match v {
        Value::Number(n) if k.eq_ignore_ascii_case(key) => Some(*n),
        _ => None,
    });
    let on = |key: &str| (number(key) == Some(1)).then_some(true);
    let port = number("PortNumber").unwrap_or(0);
    let keepalive = number("PingIntervalSecs").filter(|s| *s > 0).or_else(|| number("PingInterval").map(|m| m * 60)).unwrap_or(0);
    let mut profile = Profile {
        name: name.to_string(),
        host: text("HostName"),
        username: text("UserName"),
        port: if port == 22 { 0 } else { u16::try_from(port).unwrap_or(0) },
        key_file: text("PublicKeyFile"),
        keepalive,
        address_family: match number("AddressFamily") {
            Some(1) => "ipv4".into(),
            Some(2) => "ipv6".into(),
            _ => String::new(),
        },
        forwards: forwards(&text("PortForwardings")),
        agent_forwarding: on("AgentFwd"),
        x11_forwarding: on("X11Forward"),
        compression: on("Compression"),
        // PuTTY's BackspaceIsDelete is 1 by default (Control-?); 0 is Control-H
        key_backspace: if number("BackspaceIsDelete") == Some(0) { "bs".into() } else { String::new() },
        key_home_end: if number("RXVTHomeEnd") == Some(1) { "rxvt".into() } else { String::new() },
        key_function: match number("FunctionKeys") {
            Some(1) => "linux".into(),
            Some(2) => "xtermr6".into(),
            Some(4) => "vt100".into(),
            Some(5) => "sco".into(),
            // ESC[n~, VT400 (the same), and Xterm 216+, which SuperXTerminal doesn't have
            _ => String::new(),
        },
        font_family: text("Font"),
        font_size: number("FontHeight").unwrap_or(0),
        // PuTTY's 2000 is its own default, fewer than Settings > Terminal's: only a number chosen comes in
        scrollback: number("ScrollbackLines").filter(|l| *l != 2000).unwrap_or(0),
        charset: charset(&text("LineCodePage")),
        cursor: match number("CurType") {
            Some(1) => "underline".into(),
            Some(2) => "bar".into(),
            _ => String::new(),
        },
        cursor_blink: on("BlinkCur"),
        bell: match number("Beep") {
            Some(0) => "none".into(),
            Some(2) => "visual".into(),
            Some(1 | 3 | 4) => "sound".into(),
            _ => String::new(),
        },
        ..Profile::default()
    };
    // PuTTY's foreground, background and cursor (Colour0, 2 and 5), when they aren't its defaults
    let colour = |key: &str, default: &str| {
        let value = text(key);
        let parts: Vec<u8> = value.split(',').filter_map(|p| p.trim().parse().ok()).collect();
        (parts.len() == 3 && value.replace(' ', "") != default).then(|| format!("#{:02x}{:02x}{:02x}", parts[0], parts[1], parts[2]))
    };
    let (fg, bg, cursor) = (colour("Colour0", "187,187,187"), colour("Colour2", "0,0,0"), colour("Colour5", "0,255,0"));
    if fg.is_some() || bg.is_some() || cursor.is_some() {
        profile.colours = "custom".into();
        profile.foreground = fg.unwrap_or_default();
        profile.background = bg.unwrap_or_default();
        profile.cursor_color = cursor.unwrap_or_default();
    }
    profile
}

/// PuTTY's PortForwardings ("L8080=localhost:80,4R9000=localhost:22,D1080") as tunnels; an address-family prefix (4
/// or 6) is dropped, and an entry that isn't a tunnel is left out
fn forwards(text: &str) -> Vec<Forward> {
    text.split(',')
        .filter_map(|entry| {
            let entry = entry.trim().trim_start_matches(['4', '6']);
            let (kind, rest) = (entry.chars().next()?, entry.get(1..)?);
            match kind {
                'L' | 'R' => {
                    let (listen, to) = rest.split_once('=')?;
                    Some(Forward { kind: if kind == 'L' { "local" } else { "remote" }.into(), listen: listen.into(), to: to.into() })
                }
                'D' => Some(Forward { kind: "dynamic".into(), listen: rest.split('=').next()?.into(), to: String::new() }),
                _ => None,
            }
        })
        .filter(|f| !f.listen.is_empty())
        .collect()
}

/// PuTTY's Remote character set (LineCodePage, "ISO-8859-1:1998 (Latin-1, West Europe)") as a profile's; "" for
/// UTF-8 or one SuperXTerminal doesn't have
fn charset(text: &str) -> String {
    let lower = text.to_ascii_lowercase();
    if lower.contains("8859-15") {
        "latin9"
    } else if lower.contains("8859-1") {
        "latin1"
    } else if lower.contains("1252") {
        "windows1252"
    } else if lower.contains("437") {
        "cp437"
    } else {
        ""
    }
    .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A reg export as regedit writes it: UTF-16 little-endian, with its byte order mark and CRLF lines
    fn utf16(text: &str) -> Vec<u8> {
        let mut bytes = vec![0xff, 0xfe];
        bytes.extend(text.replace('\n', "\r\n").encode_utf16().flat_map(|u| u.to_le_bytes()));
        bytes
    }

    const EXPORT: &str = r#"Windows Registry Editor Version 5.00

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions]

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\Default%20Settings]
"HostName"=""
"PortNumber"=dword:00000016
"BackspaceIsDelete"=dword:00000001

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\Sessions\Lab%20switches]
"HostName"="10.0.0.5"
"UserName"="admin"
"PortNumber"=dword:00000016
"PublicKeyFile"="C:\\Users\\me\\.ssh\\lab.ppk"
"PingIntervalSecs"=dword:0000001e
"PortForwardings"="L8080=localhost:80,4R9000=localhost:22,D1080"
"AgentFwd"=dword:00000001
"X11Forward"=dword:00000000
"BackspaceIsDelete"=dword:00000000
"FunctionKeys"=dword:00000004
"Font"="Consolas"
"FontHeight"=dword:0000000c
"ScrollbackLines"=dword:00002710
"LineCodePage"="ISO-8859-1:1998 (Latin-1, West Europe)"
"Colour0"="255,255,255"
"Colour2"="0,0,0"
"Colour5"="0,255,0"
"KEX"=hex(7):65,00,63,00,\
  64,00,68,00

[HKEY_CURRENT_USER\Software\SimonTatham\PuTTY\SshHostKeys]
"ssh-ed25519@22:10.0.0.5"="0x1234"
"#;

    #[test]
    fn a_reg_export_of_puttys_sessions_becomes_a_profile_for_each_with_its_settings() {
        let profiles = read(&utf16(EXPORT)).unwrap();
        assert_eq!(profiles.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["PuTTY Default Settings", "Lab switches"]);
        let lab = &profiles[1];
        assert_eq!((lab.host.as_str(), lab.username.as_str(), lab.port, lab.key_file.as_str()), ("10.0.0.5", "admin", 0, "C:\\Users\\me\\.ssh\\lab.ppk"));
        assert_eq!(lab.keepalive, 30);
        assert_eq!(
            lab.forwards.iter().map(|f| (f.kind.as_str(), f.listen.as_str(), f.to.as_str())).collect::<Vec<_>>(),
            [("local", "8080", "localhost:80"), ("remote", "9000", "localhost:22"), ("dynamic", "1080", "")]
        );
        assert_eq!((lab.agent_forwarding, lab.x11_forwarding), (Some(true), None));
        assert_eq!((lab.key_backspace.as_str(), lab.key_function.as_str()), ("bs", "vt100"));
        assert_eq!((lab.font_family.as_str(), lab.font_size, lab.scrollback, lab.charset.as_str()), ("Consolas", 12, 10000, "latin1"));
        // only the foreground differs from PuTTY's colours
        assert_eq!((lab.colours.as_str(), lab.foreground.as_str(), lab.background.as_str()), ("custom", "#ffffff", ""));
        // PuTTY's defaults stay empty, to fall back to Default Settings
        let defaults = &profiles[0];
        assert_eq!((defaults.port, defaults.key_backspace.as_str(), defaults.colours.as_str()), (0, "", ""));
    }

    #[test]
    fn an_ansi_regedit4_file_and_kittys_sessions_are_read_too_and_anything_else_is_refused() {
        let kitty = "REGEDIT4\r\n\r\n[HKEY_CURRENT_USER\\Software\\9bis.com\\KiTTY\\Sessions\\core%2Dsw1]\r\n\"HostName\"=\"10.0.0.6\"\r\n";
        assert_eq!(read(kitty.as_bytes()).unwrap()[0].name, "core-sw1");
        assert!(read(b"{\"profiles\":[]}").unwrap_err().starts_with("Not a registry export"));
        assert!(read(&utf16("Windows Registry Editor Version 5.00\n\n[HKEY_CURRENT_USER\\Software\\Other]\n")).unwrap_err().starts_with("There are no PuTTY sessions"));
    }
}
