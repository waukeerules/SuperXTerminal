//! Import of Remote Desktop sessions: a `.rdp` file (its address, port and username, named as the file), and
//! on Windows the recent computers Remote Desktop Connection keeps in the registry.

use std::path::Path;

// the registry is read on Windows only
#[cfg(windows)]
use super::registry_values;
use super::Found;

/// A session from a `.rdp` file: its name, `full address:s:host[:port]` and `username:s:`
pub(crate) fn read_rdp(path: &Path, text: &str) -> Found {
    let mut f = Found {
        name: path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string(),
        protocol: "RDP",
        icon: "monitor",
        port: 3389,
        note: format!("From {}", path.file_name().and_then(|s| s.to_str()).unwrap_or("")),
        ..Found::default()
    };
    for line in text.lines() {
        let line = line.trim().trim_start_matches('\u{feff}');
        let Some((key, value)) = line.split_once(":s:") else { continue };
        match key.to_ascii_lowercase().as_str() {
            "full address" => {
                let value = value.trim();
                // [v6]:port, host:port, or a host alone
                let (host, port) = match value.strip_prefix('[').and_then(|v| v.split_once(']')) {
                    Some((host, rest)) => (host.to_string(), rest.strip_prefix(':').and_then(|p| p.parse().ok())),
                    None => match value.rsplit_once(':') {
                        Some((host, port)) if !host.contains(':') => (host.to_string(), port.parse().ok()),
                        _ => (value.to_string(), None),
                    },
                };
                f.host = host;
                f.port = port.unwrap_or(3389);
            }
            "username" => f.username = value.trim().to_string(),
            _ => {}
        }
    }
    f
}

/// The registry key holding Remote Desktop Connection's recent computers (MRU0, MRU1, ...)
#[cfg(windows)]
pub(crate) const RDP_RECENT: &str = "HKEY_CURRENT_USER\\Software\\Microsoft\\Terminal Server Client\\Default";

/// Remote Desktop Connection's recent computers (MRU0...MRU9), as SuperXPuTTY's RDPDataHelper: one RDP session each,
/// named by its address. Windows only.
pub(crate) fn read_rdp_recent() -> Result<Vec<Found>, String> {
    #[cfg(windows)]
    {
        let mut found: Vec<Found> = registry_values(RDP_RECENT)
            .into_iter()
            .filter(|(key, name, value)| key.is_empty() && name.starts_with("mru") && !value.trim().is_empty())
            .map(|(_, _, value)| {
                let address = value.trim().to_string();
                let (host, port) = match address.rsplit_once(':') {
                    Some((h, p)) if p.parse::<u16>().is_ok() => (h.to_string(), p.parse().unwrap_or(3389)),
                    _ => (address.clone(), 3389),
                };
                // named by the address, without Remote Desktop's own port; another port stays in the name
                let name = if port == 3389 { host.clone() } else { format!("{host} {port}") };
                Found { name, protocol: "RDP", host, port, icon: "monitor", ..Found::default() }
            })
            .collect();
        found.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
        Ok(found)
    }
    #[cfg(not(windows))]
    {
        Err("Remote Desktop Connection's recent computers are only on Windows.".into())
    }
}
