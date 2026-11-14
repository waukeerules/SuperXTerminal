//! What to open from outside: command-line arguments and links, as SuperXPuTTY's CommandLineOptions.
//!
//! Arguments are SuperXPuTTY's: `-session <id>`, `-layout <name>`, a host (`host`, `host:port`, `user@host`, or a URL
//! such as `ssh://host:2020`) with `-ssh` `-telnet` `-rlogin` `-raw` `-serial` `-vnc` `-rdp`, `-P port`, `-l user`,
//! `-pw password` and `-load profile`. Links are `superxterminal://session/<id>`, `superxterminal://layout/<name>`,
//! SuperXPuTTY's form (`superxterminal://-session%20Lab/sw1/`, its arguments URL-encoded), and `ssh://` and
//! `telnet://` links, opened as Quick connect opens a host.

use serde::Serialize;

/// Something to open
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Request {
    /// A stored session, by its id ("Lab/sw1"; a Database one's as the tree shows it, or "@db/...")
    Session { id: String },
    /// A layout, by name
    Layout { name: String },
    /// A host, as Quick connect: the protocol, the host (with its port), the username, password and profile
    Connect { protocol: String, host: String, username: String, password: String, profile: String },
}

/// SuperXPuTTY's protocol switches and the protocol each names
const PROTOCOLS: &[(&str, &str)] = &[
    ("-ssh", "SSH"), ("-telnet", "Telnet"), ("-rlogin", "Rlogin"), ("-raw", "Raw"), ("-serial", "Serial"),
    ("-cygterm", "Cygterm"), ("-vnc", "VNC"), ("-rdp", "RDP"),
];

/// What a command line asks to open; None when it asks for nothing (no arguments, or only macOS's own)
pub fn from_args(args: &[String]) -> Option<Request> {
    // macOS adds -psn_... when Finder starts an app; one argument that's a link is a link
    let args: Vec<&String> = args.iter().filter(|a| !a.starts_with("-psn_")).collect();
    if let [only] = args.as_slice() && only.contains("://") {
            return from_url(only);
        }
    let mut session = None;
    let mut layout = None;
    let mut host = None;
    let (mut protocol, mut port, mut username, mut password, mut profile) = (None, None, String::new(), String::new(), String::new());
    let mut queue = args.into_iter();
    while let Some(arg) = queue.next() {
        match arg.as_str() {
            "-session" => session = queue.next().cloned(),
            "-layout" => layout = queue.next().cloned(),
            "-P" => port = queue.next().and_then(|p| p.parse::<u16>().ok()),
            "-l" => username = queue.next().cloned().unwrap_or_default(),
            "-pw" => password = queue.next().cloned().unwrap_or_default(),
            "-load" => profile = queue.next().cloned().unwrap_or_default(),
            // SuperXPuTTY's -scp opened the file transfer instead; the SFTP browser isn't here yet
            "-scp" | "--help" => {}
            other => match PROTOCOLS.iter().find(|(switch, _)| *switch == other) {
                Some((_, name)) => protocol = Some(name.to_string()),
                None if !other.starts_with('-') => host = Some(other.to_string()),
                None => {}
            },
        }
    }
    if let Some(id) = session {
        return Some(Request::Session { id });
    }
    if let Some(name) = layout {
        return Some(Request::Layout { name });
    }
    let mut host = host?;
    // a port given apart goes with the host, unless the host has its own
    if let Some(port) = port {
        let has_port = host.rsplit_once(':').is_some_and(|(_, p)| p.trim_end_matches('/').parse::<u16>().is_ok());
        if !has_port {
            host = format!("{}:{port}", host.trim_end_matches('/'));
        }
    }
    Some(Request::Connect { protocol: protocol.unwrap_or_else(|| "SSH".into()), host, username, password, profile })
}

/// What a link asks to open
pub fn from_url(url: &str) -> Option<Request> {
    let (scheme, rest) = url.split_once("://")?;
    match scheme.to_ascii_lowercase().as_str() {
        "superxterminal" | "superputty" | "superxputty" => {
            let rest = rest.trim_end_matches('/');
            if let Some(id) = rest.strip_prefix("session/") {
                return Some(Request::Session { id: decode(id) });
            }
            if let Some(name) = rest.strip_prefix("layout/") {
                return Some(Request::Layout { name: decode(name) });
            }
            // SuperXPuTTY's: the command line, URL-encoded
            let args: Vec<String> = decode(rest).split(' ').filter(|a| !a.is_empty()).map(str::to_string).collect();
            from_args(&args).filter(|r| !matches!(r, Request::Connect { host, .. } if host.contains("://")))
        }
        // ssh://user@host:port, telnet://host:port: as Quick connect opens them (it reads the scheme and user)
        "ssh" | "telnet" | "rlogin" | "raw" => Some(Request::Connect {
            protocol: scheme.to_ascii_uppercase(),
            host: decode(url),
            username: String::new(),
            password: String::new(),
            profile: String::new(),
        }),
        _ => None,
    }
}

/// %XX escapes decoded (and + as a space, as SuperXPuTTY's UrlDecode)
fn decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                match u8::from_str_radix(std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or(""), 16) {
                    Ok(b) => {
                        out.push(b);
                        i += 3;
                        continue;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            b'+' => out.push(b' '),
            b => out.push(b),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A command line's arguments, split at each space
    fn args(text: &str) -> Vec<String> {
        text.split(' ').map(str::to_string).collect()
    }

    /// The connection a command line asks for, with no password or profile
    fn connect(protocol: &str, host: &str, username: &str) -> Request {
        Request::Connect { protocol: protocol.into(), host: host.into(), username: username.into(), password: String::new(), profile: String::new() }
    }

    #[test]
    fn reads_superxputty_command_lines() {
        assert_eq!(from_args(&args("-session Lab/sw1")), Some(Request::Session { id: "Lab/sw1".into() }));
        assert_eq!(from_args(&args("-layout Core")), Some(Request::Layout { name: "Core".into() }));
        assert_eq!(from_args(&args("-telnet -P 2323 -l admin 10.0.0.1")), Some(connect("Telnet", "10.0.0.1:2323", "admin")));
        assert_eq!(from_args(&args("ssh://localhost:2020")), Some(connect("SSH", "ssh://localhost:2020", "")));
        assert_eq!(from_args(&args("-psn_0_12345")), None, "Finder's own argument is nothing to open");
        assert_eq!(from_args(&[]), None);
        let Some(Request::Connect { password, profile, .. }) = from_args(&args("-pw secret -load root host1")) else { panic!() };
        assert_eq!((password.as_str(), profile.as_str()), ("secret", "root"));
    }

    #[test]
    fn reads_links() {
        assert_eq!(from_url("superxterminal://session/Lab%20A/sw1"), Some(Request::Session { id: "Lab A/sw1".into() }));
        assert_eq!(from_url("superxterminal://layout/Core%20switches/"), Some(Request::Layout { name: "Core switches".into() }));
        assert_eq!(from_url("superxterminal://-session%20Lab/sw1/"), Some(Request::Session { id: "Lab/sw1".into() }));
        assert_eq!(from_url("ssh://admin@10.0.0.1:2222"), Some(connect("SSH", "ssh://admin@10.0.0.1:2222", "")));
        assert_eq!(from_url("telnet://switch"), Some(connect("TELNET", "telnet://switch", "")));
        assert_eq!(from_url("http://example.com"), None);
    }
}
