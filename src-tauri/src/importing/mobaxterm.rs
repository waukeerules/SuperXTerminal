//! Import from MobaXterm: its exported `.mxtsessions` file, an INI of `[Bookmarks]` sections (one for each of
//! MobaXterm's folders) whose lines hold a session's name and its `%`-separated fields. The connection type's
//! number says what the session is: Telnet, VNC, a serial line, WSL, a command prompt or PowerShell, and SSH for
//! any other.

use super::Found;

/// Sessions in a MobaXterm sessions file (MobaDataHelper): `[Bookmarks_n]` sections with `SubRep=` (the folder,
/// `\` separated) and `name=#id#...%host%port%user%...` lines; the id is the connection type
pub(crate) fn read_mobaxterm(text: &str) -> Vec<Found> {
    let mut found = Vec::new();
    let mut folder = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("[Bookmarks") {
            folder.clear();
            continue;
        }
        if let Some(sub) = line.strip_prefix("SubRep=") {
            folder = sub.trim().replace('\\', "/");
            continue;
        }
        if line.starts_with("ImgNum=") {
            continue;
        }
        let Some((name, rest)) = line.split_once('=') else { continue };
        let Some(rest) = rest.trim_start().strip_prefix('#') else { continue };
        let Some((kind, rest)) = rest.split_once('#') else { continue };
        let Ok(kind) = kind.parse::<u32>() else { continue };
        // #109#0%host%22%user%...: the first part is a flag, then host, port and username
        let parts: Vec<&str> = rest.split('%').collect();
        let part = |i: usize| parts.get(i).copied().unwrap_or("").trim().to_string();
        let mut f = Found { folder: folder.clone(), name: name.trim().to_string(), host: part(1), port: part(2).parse().unwrap_or(0), username: part(3), ..Found::default() };
        (f.protocol, f.icon) = match kind {
            98 => ("Telnet", "application_xp_terminal"),
            105 => ("WSL", "application_osx_terminal"),
            128 => ("VNC", "map"),
            129 => ("WINCMD", "application_osx_terminal"),
            131 => ("Serial", "telephone"),
            192 => ("PS", "application_osx_terminal"),
            _ => ("SSH", "computer"),
        };
        match kind {
            105 | 129 | 192 => {
                f.host.clear();
                f.username.clear();
                f.port = 0;
            }
            128 if f.username == "-1" => f.username.clear(),
            131 => {
                // the line is the first word of the name ("COM3 (USB Serial)"); the speed is stored plus 1000000
                f.host = f.name.split(' ').next().unwrap_or("").to_string();
                f.username.clear();
                let speed = part(2).parse::<u32>().unwrap_or(0).saturating_sub(1_000_000);
                if speed > 0 {
                    f.extra_args = format!("-sercfg {speed},8,n,1,N");
                }
                f.port = 0;
            }
            _ => {}
        }
        found.push(f);
    }
    found
}
