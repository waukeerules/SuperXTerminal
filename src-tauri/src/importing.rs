//! Sessions to and from other formats, as SuperXPuTTY's File > Import sessions and Export sessions to CSV.
//!
//! CSV is SuperXPuTTY's `SessionCsv`: the same columns and headers (those of the Database sessions table, so a table
//! exported from Postgres imports too), a `-pw` password written as `********` and kept on re-import, a leading `'`
//! before a value a spreadsheet would take for a formula, and `;` or tab separators read as well as commas. A row is
//! matched to a session by its Source, Folder and Name: an existing Saved or Private session is updated, otherwise
//! one is added; Database rows are skipped.
//!
//! MobaXterm (`.mxtsessions`), Remote Desktop (`.rdp`) files and `~/.ssh/config` add new sessions in a folder of
//! their own ("ImportedFromMobaXterm", ...), as SuperXPuTTY's `ImportSessions`, numbered when a name is taken.
//! SecureCRT's Sessions folder (SuperXTerminal's own; SuperXPuTTY has none) does the same, into ImportedFromSecureCRT.
//!
//! Nothing is changed until the user has seen the plan's summary: [`Plan`] is worked out first, then [`apply`]
//! writes each file once.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;

pub mod csv;
mod putty;
mod securecrt;

use csv::{keep_password, plan_csv, source_of};
// the registry key the import window names, which only Windows has
#[cfg(windows)]
use putty::PUTTY_SESSIONS;
use crate::sessions::{self, Store, StoredSession};

/// SuperXPuTTY's connection types, as the Proto attribute writes them
const PROTOCOLS: &[&str] = &["SSH", "Telnet", "Rlogin", "Raw", "Serial", "VNC", "RDP", "Mintty", "Cygterm", "WSL", "WINCMD", "PS"];

// ---- Export

// ---- Reading CSV

// ---- Plans

/// One row of an import and what it will do
#[derive(Clone, Debug)]
pub struct Row {
    /// The row's line in the file (the header is 1), or its place in the list for other imports
    pub line: usize,
    /// Why it's left out; None to import it
    pub skip: Option<String>,
    pub store: Store,
    pub id: String,
    /// It changes the session already there (CSV); otherwise it's added
    pub update: bool,
    /// Added with a number after its name when the name is taken, rather than changing that session
    pub unique: bool,
    /// The attributes to set, in order
    pub values: Vec<(String, String)>,
}

/// What an import will do, worked out before anything changes
#[derive(Debug, Default)]
pub struct Plan {
    pub rows: Vec<Row>,
    /// Why nothing at all can be imported
    pub error: Option<String>,
    /// Columns in the file that aren't used
    pub ignored: Vec<String>,
}

/// The plan for the window: counts, the first rows left out and why, and where new sessions go
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub adds: usize,
    pub updates: usize,
    pub skips: usize,
    /// "Line 4: no name" (CSV), or "ImportedFromSecureCRT/modem: a TAPI session..." (another app's), the first eight
    pub skipped: Vec<String>,
    pub ignored: Vec<String>,
    pub error: Option<String>,
    /// The first sessions it adds, then changes, "New Private: ImportedFromMobaXterm/sw1"
    pub examples: Vec<String>,
}

impl Plan {
    /// What the plan would do, counted: for the import's confirmation
    pub fn summary(&self) -> Summary {
        let go = || self.rows.iter().filter(|r| r.skip.is_none());
        Summary {
            adds: go().filter(|r| !r.update).count(),
            updates: go().filter(|r| r.update).count(),
            skips: self.rows.iter().filter(|r| r.skip.is_some()).count(),
            skipped: self
                .rows
                .iter()
                // a CSV row by its line; another app's session (added, numbered when taken) by where it would have gone
                .filter_map(|r| r.skip.as_ref().map(|why| if r.unique && !r.id.is_empty() { format!("{}: {why}", r.id) } else { format!("Line {}: {why}", r.line) }))
                .take(8)
                .collect(),
            ignored: self.ignored.clone(),
            error: self.error.clone(),
            examples: go()
                .filter(|r| !r.update)
                .chain(go().filter(|r| r.update))
                .take(6)
                .map(|r| format!("{} {}: {}", if r.update { "Updated" } else { "New" }, source_of(r.store), r.id))
                .collect(),
        }
    }
}

/// What a CSV import is checked against: the Saved and Private sessions now, and whether Saved sessions are in use
pub struct Existing<'a> {
    pub saved: &'a [StoredSession],
    pub private: &'a [StoredSession],
    pub use_saved: bool,
}

impl Existing<'_> {
    /// Which store already has a session with this id, if any (Saved first)
    fn find(&self, id: &str) -> Option<Store> {
        if self.saved.iter().any(|s| s.id() == id) {
            Some(Store::Saved)
        } else if self.private.iter().any(|s| s.id() == id) {
            Some(Store::Private)
        } else {
            None
        }
    }

    /// Whether a session in either store has this id, ignoring case, as the tree does
    fn taken(&self, id: &str) -> bool {
        self.saved.iter().chain(self.private).any(|s| s.id().eq_ignore_ascii_case(id))
    }
}

/// A plan adding sessions found in another app's file to `folder` of `store`, as SuperXPuTTY's ImportSessions
fn plan_added(found: Vec<Found>, folder: &str, store: Store) -> Plan {
    let mut plan = Plan::default();
    for (i, f) in found.into_iter().enumerate() {
        let name = f.name.replace('/', "-").trim().to_string();
        let mut row = Row { line: i + 1, skip: None, store, id: String::new(), update: false, unique: true, values: Vec::new() };
        if name.is_empty() {
            row.skip = Some("no name".into());
            plan.rows.push(row);
            continue;
        }
        let sub = f.folder.trim_matches('/');
        row.id = [folder, sub, &name].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join("/");
        if f.skip.is_some() {
            row.skip = f.skip;
            plan.rows.push(row);
            continue;
        }
        row.values = vec![
            ("SessionName".into(), name),
            ("Proto".into(), f.protocol.into()),
            ("Host".into(), f.host),
            ("Port".into(), f.port.to_string()),
            ("Username".into(), f.username),
            ("ImageKey".into(), f.icon.into()),
            ("ExtraArgs".into(), f.extra_args),
            ("JumpHost".into(), f.jump_host),
            ("Note".into(), f.note),
        ];
        // PuTTY's own sessions name themselves as their profile, as SuperXPuTTY's import does
        if !f.profile.is_empty() {
            row.values.push(("PuttySession".into(), f.profile));
        }
        plan.rows.push(row);
    }
    if plan.rows.is_empty() {
        plan.error = Some("No sessions were found in it.".into());
    }
    plan
}

/// One session found in another app's file
#[derive(Debug, Default, PartialEq)]
struct Found {
    /// A folder inside the import's own, e.g. MobaXterm's "Lab/Core"
    folder: String,
    name: String,
    protocol: &'static str,
    host: String,
    port: u16,
    username: String,
    icon: &'static str,
    extra_args: String,
    jump_host: String,
    note: String,
    /// The PuTTY profile (PuttySession); empty for the default
    profile: String,
    /// Why it isn't imported (a kind that can't be opened here), said in the plan's skipped list; None to import it
    skip: Option<String>,
}

// ---- MobaXterm

/// Sessions in a MobaXterm sessions file (MobaDataHelper): `[Bookmarks_n]` sections with `SubRep=` (the folder,
/// `\` separated) and `name=#id#...%host%port%user%...` lines; the id is the connection type
fn read_mobaxterm(text: &str) -> Vec<Found> {
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

// ---- SecureCRT

// ---- Remote Desktop files

/// A session from a `.rdp` file: its name, `full address:s:host[:port]` and `username:s:`
fn read_rdp(path: &Path, text: &str) -> Found {
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

/// .rdp files are UTF-16 when Windows saves them; plain UTF-8 otherwise
fn read_text(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let units: Vec<u16> = bytes[2..].as_chunks::<2>().0.iter().map(|c| u16::from_le_bytes(*c)).collect();
        return Ok(String::from_utf16_lossy(&units));
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let units: Vec<u16> = bytes[2..].as_chunks::<2>().0.iter().map(|c| u16::from_be_bytes(*c)).collect();
        return Ok(String::from_utf16_lossy(&units));
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

// ---- ~/.ssh/config

/// Sessions from an OpenSSH config: one per name in a `Host` line without wildcards, with its HostName, Port, User,
/// ProxyJump (the jump host), IdentityFile (`-i`) and forwards (`-L`, `-R`, `-D`). As ssh, the first value given for
/// an option is the one used; `Include` is followed (relative to ~/.ssh, `*` and `?` in the file name); `Match`
/// blocks and `Host *` defaults are left to ssh itself, which still reads the file when a session opens.
fn read_ssh_config(path: &Path, ssh_folder: &Path, depth: u32) -> Vec<Found> {
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

/// Add `switch value` to a command line's arguments, unless the value is empty
fn add_arg(args: &mut String, switch: &str, value: &str) {
    if value.is_empty() {
        return;
    }
    if !args.is_empty() {
        args.push(' ');
    }
    let value = if value.contains(' ') { format!("\"{value}\"") } else { value.to_string() };
    args.push_str(&format!("{switch} {value}"));
}

/// The files an Include names: ~ expanded, relative to ~/.ssh, with `*` and `?` matched in the file name
fn expand_include(pattern: &str, ssh_folder: &Path) -> Vec<PathBuf> {
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
fn wildcard(pattern: &str, text: &str) -> bool {
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

// ---- Another Sessions.xml

/// A plan adding every session of another Sessions.xml (SuperPuTTY's or SuperXPuTTY's) to `folder` with all its
/// settings, as SuperXPuTTY's ImportSessionsFromFile ("Imported/<its id>")
fn plan_stored(found: Vec<StoredSession>, folder: &str, store: Store) -> Plan {
    let mut plan = Plan::default();
    for (i, session) in found.into_iter().enumerate() {
        let id = session.id().trim_matches('/').to_string();
        let mut row = Row { line: i + 1, skip: None, store, id: String::new(), update: false, unique: true, values: Vec::new() };
        if id.is_empty() {
            row.skip = Some("no SessionId".into());
        } else {
            row.id = format!("{folder}/{id}");
            // the id comes from where it goes; the name and the rest are the session's as they were (its name can be
            // more than its id's last part: "sw1 - C9300 - 10.0.0.1" kept whole)
            row.values = session.attributes.into_iter().filter(|(name, _)| name != "SessionId").collect();
        }
        plan.rows.push(row);
    }
    if plan.rows.is_empty() {
        plan.error = Some("No sessions were found in it.".into());
    }
    plan
}

// ---- PuTTY and KiTTY

/// One of SuperXPuTTY's connection types by PuTTY's name for it ("ssh", "telnet", "rlogin", "raw", "serial"); SSH
/// for any other
fn protocol_named(name: &str) -> &'static str {
    PROTOCOLS.iter().find(|p| p.eq_ignore_ascii_case(name.trim())).copied().unwrap_or("SSH")
}

/// Every key under a registry key with its values, read through reg.exe ("reg query KEY /s"), as
/// (subkey name, lower-cased value name, value); empty when the key isn't there
#[cfg(windows)]
fn registry_values(key: &str) -> Vec<(String, String, String)> {
    use std::os::windows::process::CommandExt;
    /// Run reg without a console window flashing up
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let Ok(output) = std::process::Command::new("reg").args(["query", key, "/s"]).creation_flags(CREATE_NO_WINDOW).output() else {
        return Vec::new();
    };
    let text = String::from_utf8_lossy(&output.stdout).to_string();
    let mut values = Vec::new();
    let mut current = String::new();
    for line in text.lines() {
        if line.starts_with("HKEY_") {
            // the key's own line: the part after the key asked for is the subkey's name
            current = line.get(key.len()..).unwrap_or("").trim_start_matches('\\').to_string();
            continue;
        }
        // "    Name    REG_SZ    value"
        let Some((name, rest)) = line.trim_start().split_once("    REG_") else { continue };
        let value = rest.split_once("    ").map(|(_, v)| v).unwrap_or("");
        values.push((current.clone(), name.trim().to_ascii_lowercase(), value.to_string()));
    }
    values
}

/// The registry key holding Remote Desktop Connection's recent computers (MRU0, MRU1, ...)
#[cfg(windows)]
const RDP_RECENT: &str = "HKEY_CURRENT_USER\\Software\\Microsoft\\Terminal Server Client\\Default";

/// Where Import from PuTTY/KiTTY and from Remote Desktop's recent computers read, for the import window
pub fn source_of_import(kind: &str) -> Option<String> {
    match kind {
        "ssh" => Some(ssh_config_path().display().to_string()),
        // SecureCRT's Sessions folder where it has one; otherwise the window waits for one to be chosen
        "securecrt" => Some(securecrt::securecrt_folder()).filter(|f| f.is_dir()).map(|f| f.display().to_string()),
        #[cfg(windows)]
        "putty" => Some(format!("{PUTTY_SESSIONS} and KiTTY's")),
        #[cfg(not(windows))]
        "putty" => Some(dirs::home_dir().unwrap_or_default().join(".putty/sessions").display().to_string()),
        #[cfg(windows)]
        "rdpRegistry" => Some(RDP_RECENT.to_string()),
        _ => None,
    }
}

/// Remote Desktop Connection's recent computers (MRU0...MRU9), as SuperXPuTTY's RDPDataHelper: one RDP session each,
/// named by its address. Windows only.
fn read_rdp_recent() -> Result<Vec<Found>, String> {
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

// ---- Planning by kind

/// The default ~/.ssh/config
pub fn ssh_config_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".ssh/config")
}

/// Plan an import: `kind` is "csv", "mobaxterm", "rdp" (one or more files), "ssh", "superputty" (a Sessions.xml),
/// "puttycm" (a PuTTY CM export), "puttyFolder" (a folder of PuTTY session files), "securecrt" (SecureCRT's Sessions
/// folder, or a folder in it), "putty" (PuTTY's and KiTTY's
/// saved sessions) or "rdpRegistry" (Remote Desktop's recent computers); `store` is where new sessions from other
/// apps go (CSV rows say their own). The last two read no file.
pub fn plan(kind: &str, paths: &[PathBuf], store: Store, existing: &Existing) -> Result<Plan, String> {
    match kind {
        "putty" => return Ok(plan_added(putty::read_putty_saved()?, "ImportedFromPuTTY", store)),
        "rdpRegistry" => return Ok(plan_added(read_rdp_recent()?, "ImportedFromRDPRecent", store)),
        _ => {}
    }
    let first = paths.first().ok_or("Choose a file.")?;
    match kind {
        "superputty" => {
            let mut problems = Vec::new();
            let found = sessions::read_sessions_file(first, &mut problems);
            if found.is_empty() {
                return Err(problems.pop().unwrap_or_else(|| "No sessions were found in it.".into()));
            }
            Ok(plan_stored(found, "Imported", store))
        }
        "puttycm" => Ok(plan_added(putty::read_puttycm(&read_text(first)?)?, "ImportedFromPuTTYCM", store)),
        "puttyFolder" => Ok(plan_added(putty::read_putty_folder(first, "")?, "ImportedFromPortablePuTTY", store)),
        "securecrt" => Ok(plan_added(securecrt::read_securecrt_folder(first)?, "ImportedFromSecureCRT", store)),
        "csv" => Ok(plan_csv(&read_text(first)?, existing)),
        "mobaxterm" => Ok(plan_added(read_mobaxterm(&read_text(first)?), "ImportedFromMobaXterm", store)),
        "rdp" => {
            let mut found = Vec::new();
            for path in paths {
                found.push(read_rdp(path, &read_text(path)?));
            }
            Ok(plan_added(found, "ImportedFromRDP", store))
        }
        "ssh" => {
            if !first.is_file() {
                return Err(format!("{} isn't there.", first.display()));
            }
            let folder = first.parent().map(Path::to_path_buf).unwrap_or_default();
            Ok(plan_added(read_ssh_config(first, &folder, 0), "ImportedFromSshConfig", store))
        }
        _ => Err(format!("Unknown import \"{kind}\"")),
    }
}

/// Carry out a plan: each store's file read fresh, changed and written once. Returns how many were added and updated.
pub fn apply(plan: &Plan, file_of: impl Fn(Store) -> Result<PathBuf, String>, ids_elsewhere: impl Fn(Store) -> Vec<String>) -> Result<(usize, usize), String> {
    let (mut added, mut updated) = (0, 0);
    for store in [Store::Saved, Store::Private] {
        let rows: Vec<&Row> = plan.rows.iter().filter(|r| r.skip.is_none() && r.store == store).collect();
        if rows.is_empty() {
            continue;
        }
        let path = file_of(store)?;
        let elsewhere = ids_elsewhere(store);
        sessions::update_file(&path, |list| {
            // The ids taken (lowercase: ids differing in case are one), here and in the other store, and where the
            // session with each id is: in the file, or among those this import adds, which go into the file together
            // at the end. Looked up, not searched for: looking through the file for every row, and again to put each
            // new session in its place, held the window up for many seconds on 10,000 rows
            let mut taken: HashSet<String> = list.iter().map(|s| s.id()).chain(elsewhere.iter().map(String::as_str)).map(str::to_ascii_lowercase).collect();
            let mut place: HashMap<String, (bool, usize)> = HashMap::new();
            for (i, session) in list.iter().enumerate() {
                place.entry(session.id().to_string()).or_insert((false, i));
            }
            let mut new: Vec<StoredSession> = Vec::new();
            for row in rows {
                let mut id = row.id.clone();
                if row.unique {
                    // as SuperXPuTTY's MakeUniqueSessionId: "name-1", "name-2", ...
                    let mut n = 1;
                    while taken.contains(&id.to_ascii_lowercase()) && n < 1000 {
                        id = format!("{}-{n}", row.id);
                        n += 1;
                    }
                }
                if let Some(&(added_now, i)) = place.get(&id) {
                    let session = if added_now { &mut new[i] } else { &mut list[i] };
                    let current = session.get("ExtraArgs").to_string();
                    for (name, value) in &row.values {
                        let value = if name == "ExtraArgs" { keep_password(value, &current) } else { value.clone() };
                        session.set(name, &value);
                    }
                    updated += 1;
                    continue;
                }
                if !taken.insert(id.to_ascii_lowercase()) {
                    continue;
                }
                let mut session = StoredSession::new(&id);
                for (name, value) in &row.values {
                    let value = if name == "ExtraArgs" { keep_password(value, "") } else { value.clone() };
                    session.set(name, &value);
                }
                // the name the import gives (a Sessions.xml's own, a CSV's), else the id's last part; with the "-1" the
                // id was given to be unique, so the two copies' names differ too
                let suffix = &id[row.id.len()..];
                let given = row.values.iter().find(|(name, value)| name == "SessionName" && !value.trim().is_empty());
                let name = match given {
                    Some((_, given)) => format!("{}{suffix}", given.trim()),
                    None => id.rsplit('/').next().unwrap_or(&id).to_string(),
                };
                session.set("SessionName", &name);
                place.insert(id, (true, new.len()));
                new.push(session);
                added += 1;
            }
            sessions::insert_all_sorted(list, new);
            Ok(())
        })?;
    }
    Ok((added, updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A session with these attributes set
    pub(super) fn session(id: &str, attributes: &[(&str, &str)]) -> StoredSession {
        let mut s = StoredSession::new(id);
        for (n, v) in attributes {
            s.set(n, v);
        }
        s
    }

    #[test]
    fn reads_mobaxterm_rdp_and_ssh_config() {
        let moba = "[Bookmarks]\nSubRep=\nImgNum=42\nsw1=#109#0%10.0.0.1%22%admin%%-1%-1%%%22%%0%0%0%%%-1%0%0%0%%1080%%0%0%1#MobaFont%10%0%0%-1%15%236,236,236%30,30,30%180,180,180%0%-1%0%%xterm%-1%-1%_Std_Colors_0_%80%24%0%1%-1%<none>%%0%0%-1#0# #-1\n[Bookmarks_1]\nSubRep=Lab\\Core\nImgNum=41\nCOM3 (USB)=#131#2%COM3%1009600%0%0%\n";
        let found = read_mobaxterm(moba);
        assert_eq!(found.len(), 2);
        assert_eq!((found[0].host.as_str(), found[0].port, found[0].username.as_str()), ("10.0.0.1", 22, "admin"));
        assert_eq!((found[1].folder.as_str(), found[1].protocol, found[1].host.as_str()), ("Lab/Core", "Serial", "COM3"));
        assert_eq!(found[1].extra_args, "-sercfg 9600,8,n,1,N");

        let rdp = read_rdp(Path::new("/x/File Server.rdp"), "screen mode id:i:2\r\nfull address:s:fs01.corp:3390\r\nusername:s:CORP\\me\r\n");
        assert_eq!((rdp.name.as_str(), rdp.host.as_str(), rdp.port, rdp.username.as_str()), ("File Server", "fs01.corp", 3390, "CORP\\me"));

        let dir = std::env::temp_dir().join(format!("sxt-ssh-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("conf.d")).unwrap();
        std::fs::write(
            dir.join("config"),
            "Include conf.d/*.conf\nHost *\n  User nobody\nHost bastion\n  HostName 203.0.113.5\n  User me\nHost sw1 sw2\n  HostName=10.0.0.1\n  Port 2222\n  ProxyJump bastion\n  IdentityFile ~/.ssh/lab key\n  LocalForward 8080 localhost:80\n",
        )
        .unwrap();
        std::fs::write(dir.join("conf.d/extra.conf"), "Host extra\n  User x\n").unwrap();
        let found = read_ssh_config(&dir.join("config"), &dir, 0);
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(names, ["extra", "bastion", "sw1", "sw2"]);
        assert_eq!(found[0].host, "extra", "no HostName: the alias");
        assert_eq!((found[2].port, found[2].jump_host.as_str()), (2222, "bastion"));
        assert_eq!(found[2].extra_args, "-i \"~/.ssh/lab key\" -L 8080:localhost:80");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn imports_get_their_own_folder_and_unique_names() {
        let dir = std::env::temp_dir().join(format!("sxt-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("PrivateSessions.xml");
        std::fs::write(&path, sessions::write_sessions(&[session("ImportedFromRDP/fs", &[])])).unwrap();
        let found = vec![Found { name: "fs".into(), protocol: "RDP", host: "fs01".into(), port: 3389, icon: "monitor", ..Found::default() }];
        let plan = plan_added(found, "ImportedFromRDP", Store::Private);
        let (added, updated) = apply(&plan, |_| Ok(path.clone()), |_| Vec::new()).unwrap();
        assert_eq!((added, updated), (1, 0));
        let read = sessions::read_sessions_file(&path, &mut Vec::new());
        let new = read.iter().find(|s| s.id() == "ImportedFromRDP/fs-1").unwrap();
        assert_eq!((new.get("Proto"), new.get("Host"), new.get("SessionName")), ("RDP", "fs01", "fs-1"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn another_sessions_file_comes_in_whole_under_imported() {
        let others = vec![session("Lab/sw1", &[("Host", "10.0.0.1"), ("Proto", "SSH"), ("ImageKey", "switch"), ("Color", "#C62828")])];
        let plan = plan_stored(others, "Imported", Store::Private);
        assert_eq!(plan.rows[0].id, "Imported/Lab/sw1");
        let values: Vec<&str> = plan.rows[0].values.iter().map(|(name, _)| name.as_str()).collect();
        assert!(values.contains(&"ImageKey") && values.contains(&"Color") && !values.contains(&"SessionId"));
    }

    #[test]
    fn an_imported_sessions_name_is_kept_whole_and_made_unique_with_its_id() {
        let dir = std::env::temp_dir().join(format!("sxt-import-names-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("PrivateSessions.xml");
        std::fs::write(&path, sessions::write_sessions(&[])).unwrap();
        // a name that's more than its id's last part, dashes and all
        let others = || vec![session("test", &[("SessionName", "test - model example - 10.10.10.10"), ("Host", "192.168.0.20")])];
        let file = |_: Store| Ok(path.clone());
        apply(&plan_stored(others(), "Imported", Store::Private), file, |_| Vec::new()).unwrap();
        apply(&plan_stored(others(), "Imported", Store::Private), file, |_| Vec::new()).unwrap();
        let read = sessions::read_sessions_file(&path, &mut Vec::new());
        let names: Vec<(&str, &str)> = read.iter().map(|s| (s.id(), s.get("SessionName"))).collect();
        assert_eq!(names, [("Imported/test", "test - model example - 10.10.10.10"), ("Imported/test-1", "test - model example - 10.10.10.10-1")]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
