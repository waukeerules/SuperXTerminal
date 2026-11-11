//! SSH algorithm lists, as PuTTY's Kex, Host keys and Cipher panels, for a profile or a session.
//!
//! A profile (Settings > Profiles) or a session (Edit Session, its SshAlgorithms attribute and the Database table's
//! ssh_algorithms column; SuperXTerminal's own) can give its own ordered list of key exchange, host key, cipher or MAC
//! algorithms, written "kex=a,b;hostkey=c;cipher=d;mac=e" with only the kinds it sets. `launch.rs` passes each kind
//! the session sets, else its profile's, else Default Settings', to ssh as that exact list (`-o KexAlgorithms=a,b`);
//! a kind none of them sets keeps ssh's own list, with the older algorithms added when the profile allows them.
//! `available` tells the editor what this computer's ssh has and its default order.

use serde::Serialize;

/// Each kind: its key in the text, ssh's option, `ssh -Q`'s query, `ssh -G`'s line, and its name for the editor
pub const KINDS: [(&str, &str, &str, &str, &str); 4] = [
    ("kex", "KexAlgorithms", "kex", "kexalgorithms", "Key exchange"),
    ("hostkey", "HostKeyAlgorithms", "key", "hostkeyalgorithms", "Host keys"),
    ("cipher", "Ciphers", "cipher", "ciphers", "Ciphers"),
    ("mac", "MACs", "mac", "macs", "MACs"),
];

/// The lists a text ("kex=a,b;cipher=c", as stored) sets, by kind key, each without blanks or repeats; unknown kinds
/// are left out
pub fn parse(text: &str) -> Vec<(&'static str, Vec<String>)> {
    let mut out: Vec<(&'static str, Vec<String>)> = Vec::new();
    for part in text.split(';') {
        let Some((key, list)) = part.split_once('=') else { continue };
        let Some(kind) = KINDS.iter().map(|k| k.0).find(|k| k.eq_ignore_ascii_case(key.trim())) else { continue };
        let mut names: Vec<String> = Vec::new();
        for name in list.split(',').map(str::trim).filter(|n| !n.is_empty()) {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
        if names.is_empty() || out.iter().any(|(k, _)| *k == kind) {
            continue;
        }
        out.push((kind, names));
    }
    // always in KINDS' order, so the same lists are always the same text
    out.sort_by_key(|(k, _)| KINDS.iter().position(|x| x.0 == *k));
    out
}

/// The text in one form ("kex=a,b;mac=c", KINDS' order), "" when it sets nothing
pub fn normalize(text: &str) -> String {
    parse(text).iter().map(|(k, names)| format!("{k}={}", names.join(","))).collect::<Vec<_>>().join(";")
}

/// The lists that apply, by ssh option: each kind from the first of `texts` that sets it (the session's, its
/// profile's, Default Settings')
pub fn chosen(texts: &[&str]) -> Vec<(&'static str, Vec<String>)> {
    let mut out = Vec::new();
    for (key, option, ..) in KINDS {
        if let Some(names) = texts.iter().find_map(|t| parse(t).into_iter().find(|(k, _)| *k == key).map(|(_, n)| n)) {
            out.push((option, names));
        }
    }
    out
}

/// ssh's arguments for the chosen lists: `-o KexAlgorithms=a,b` and so on, exact lists in the order given
pub fn args(chosen: &[(&'static str, Vec<String>)]) -> Vec<String> {
    chosen.iter().flat_map(|(option, names)| ["-o".to_string(), format!("{option}={}", names.join(","))]).collect()
}

/// One kind for the editor: what this computer's ssh has, and the order it uses when nothing is set
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Available {
    pub key: &'static str,
    pub label: &'static str,
    /// Every algorithm of this kind ssh knows (`ssh -Q`)
    pub supported: Vec<String>,
    /// The ones it offers by default, in its order (`ssh -G`, with no config of the user's)
    pub defaults: Vec<String>,
}

/// What this computer's ssh has, by kind; a kind ssh can't be asked about has empty lists
pub fn available() -> Vec<Available> {
    let defaults = ssh_defaults();
    KINDS
        .iter()
        .map(|(key, _, query, line, label)| Available {
            key,
            label,
            supported: crate::launch::ssh_query(query).unwrap_or_default(),
            defaults: defaults.iter().find(|(l, _)| l == line).map(|(_, v)| v.clone()).unwrap_or_default(),
        })
        .collect()
}

/// ssh's default lists (`ssh -G`, without the user's config so a Host block there doesn't change them), by line name
fn ssh_defaults() -> Vec<(String, Vec<String>)> {
    let mut command = std::process::Command::new(crate::launch::openssh("ssh"));
    crate::appimage::tidy(&mut command);
    command.args(["-F", if cfg!(windows) { "NUL" } else { "/dev/null" }, "-G", "example.invalid"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    let Ok(output) = command.output() else { return Vec::new() };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once(' '))
        .filter(|(name, _)| KINDS.iter().any(|k| k.3 == *name))
        .map(|(name, list)| (name.to_string(), list.split(',').map(str::to_string).collect()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_normalizes() {
        assert_eq!(normalize(" mac = hmac-sha1 , hmac-sha1 ;KEX=curve25519-sha256;bogus=x;cipher="), "kex=curve25519-sha256;mac=hmac-sha1");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn the_session_beats_its_profile_kind_by_kind() {
        let chosen = chosen(&["mac=hmac-sha1", "kex=a;mac=hmac-sha2-256", "cipher=aes128-ctr"]);
        assert_eq!(args(&chosen), ["-o", "KexAlgorithms=a", "-o", "Ciphers=aes128-ctr", "-o", "MACs=hmac-sha1"]);
    }
}
