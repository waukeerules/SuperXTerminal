//! Import from `~/.ssh/config`: each `Host` that names one computer becomes an SSH session, with its HostName,
//! User, Port, IdentityFile, ProxyJump and forwards, and the files its `Include` lines name read too. Patterns
//! (`Host *`, `Host *.lab`) aren't sessions and are left out.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::{add_arg, Found};

/// Sessions from an OpenSSH config: one per name in a `Host` line without wildcards, with its HostName, Port, User,
/// ProxyJump (the jump host), IdentityFile (`-i`) and forwards (`-L`, `-R`, `-D`). As ssh, the first value given for
/// an option is the one used; `Include` is followed (relative to ~/.ssh, `*` and `?` in the file name); `Match`
/// blocks and `Host *` defaults are left to ssh itself, which still reads the file when a session opens.
pub(super) fn read_ssh_config(path: &Path, ssh_folder: &Path, depth: u32) -> Vec<Found> {
    let mut found = Vec::new();
    let Ok(text) = std::fs::read_to_string(path) else { return found };
    let mut current: Vec<Found> = Vec::new();
    let mut have: HashSet<String> = HashSet::new();
    let mut in_match = false;
    let finish = |current: &mut Vec<Found>, found: &mut Vec<Found>| {
        for mut f in current.drain(..) {
            if f.host.is_empty() {
                f.host = f.name.clone();
            }
            found.push(f);
        }
    };
    for raw in text.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // "Key value", "Key=value" or "Key = value"
        let (key, value) = match line.find(|c: char| c.is_whitespace() || c == '=') {
            Some(at) => (&line[..at], line[at..].trim_start_matches(|c: char| c.is_whitespace() || c == '=').trim()),
            None => (line, ""),
        };
        let value = value.trim_matches('"');
        match key.to_ascii_lowercase().as_str() {
            "host" => {
                finish(&mut current, &mut found);
                have.clear();
                in_match = false;
                for name in value.split_whitespace() {
                    if !name.contains(['*', '?', '!']) {
                        let note = format!("From {}", path.display());
                        current.push(Found { name: name.to_string(), protocol: "SSH", icon: "computer", port: 0, note, ..Found::default() });
                    }
                }
            }
            "match" => {
                finish(&mut current, &mut found);
                in_match = true;
            }
            "include" if current.is_empty() && !in_match && depth < 4 => {
                for pattern in value.split_whitespace() {
                    for file in expand_include(pattern, ssh_folder) {
                        found.extend(read_ssh_config(&file, ssh_folder, depth + 1));
                    }
                }
            }
            other if !current.is_empty() => {
                let first = have.insert(other.to_string());
                for f in current.iter_mut() {
                    match other {
                        "hostname" if first => f.host = value.to_string(),
                        "port" if first => f.port = value.parse().unwrap_or(0),
                        "user" if first => f.username = value.to_string(),
                        "proxyjump" if first && !value.eq_ignore_ascii_case("none") => f.jump_host = value.to_string(),
                        // several identity files and forwards all apply
                        "identityfile" => add_arg(&mut f.extra_args, "-i", value),
                        "localforward" => add_arg(&mut f.extra_args, "-L", &value.split_whitespace().collect::<Vec<_>>().join(":")),
                        "remoteforward" => add_arg(&mut f.extra_args, "-R", &value.split_whitespace().collect::<Vec<_>>().join(":")),
                        "dynamicforward" => add_arg(&mut f.extra_args, "-D", value),
                        _ => {}
                    }
                }
            }
            _ => {}
        }
    }
    finish(&mut current, &mut found);
    found
}

/// The files an Include names: ~ expanded, relative to ~/.ssh, with `*` and `?` matched in the file name
pub(crate) fn expand_include(pattern: &str, ssh_folder: &Path) -> Vec<PathBuf> {
    let expanded = match pattern.strip_prefix("~/") {
        Some(rest) => dirs::home_dir().unwrap_or_default().join(rest),
        None => PathBuf::from(pattern),
    };
    let full = if expanded.is_absolute() { expanded } else { ssh_folder.join(expanded) };
    let Some(name) = full.file_name().and_then(|n| n.to_str()).map(str::to_string) else { return Vec::new() };
    if !name.contains(['*', '?']) {
        return if full.is_file() { vec![full] } else { Vec::new() };
    }
    let Some(dir) = full.parent() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut files: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file() && p.file_name().and_then(|n| n.to_str()).is_some_and(|n| wildcard(&name, n)))
        .collect();
    files.sort();
    files
}

/// Whether `text` matches a pattern with `*` (any run) and `?` (any one character)
pub(crate) fn wildcard(pattern: &str, text: &str) -> bool {
    let (p, t): (Vec<char>, Vec<char>) = (pattern.chars().collect(), text.chars().collect());
    let (mut pi, mut ti, mut star, mut mark) = (0, 0, None, 0);
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            mark = ti;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            mark += 1;
            ti = mark;
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

/// The default ~/.ssh/config
pub fn ssh_config_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".ssh/config")
}
