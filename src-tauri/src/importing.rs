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
//!
//! Nothing is changed until the user has seen the plan's summary: [`Plan`] is worked out first, then [`apply`]
//! writes each file once.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::editing::replace_password;
use crate::sessions::{self, Store, StoredSession};

/// What a `-pw` password is exported as
pub const MASKED_PASSWORD: &str = "********";

/// One CSV column: the header written, the headers read (normalized), and the session attribute
struct Column {
    header: &'static str,
    names: &'static [&'static str],
    attribute: &'static str,
    /// Leading and trailing spaces are dropped on import
    trim: bool,
}

/// Every column after Source, Folder and Name, in the order exported (SessionCsv.Columns)
const COLUMNS: &[Column] = &[
    Column { header: "Protocol", names: &["protocol", "proto", "type", "connectiontype"], attribute: "Proto", trim: true },
    Column { header: "Host", names: &["host", "hostname", "address", "ip"], attribute: "Host", trim: true },
    Column { header: "Port", names: &["port"], attribute: "Port", trim: true },
    Column { header: "Username", names: &["username", "user", "login"], attribute: "Username", trim: true },
    Column { header: "PuTTY profile", names: &["puttyprofile", "puttysession", "profile"], attribute: "PuttySession", trim: false },
    Column { header: "Extra args", names: &["extraargs", "extraarguments", "arguments"], attribute: "ExtraArgs", trim: false },
    Column { header: "Icon", names: &["icon", "imagekey"], attribute: "ImageKey", trim: true },
    Column { header: "Notes", names: &["notes", "note"], attribute: "Note", trim: false },
    Column { header: "SPSL script", names: &["spslscript", "spslfilename", "spsl", "script"], attribute: "SPSLFileName", trim: false },
    Column { header: "RDP options", names: &["rdpoptions"], attribute: "RdpOptions", trim: false },
    Column { header: "Local path", names: &["localpath"], attribute: "LocalPath", trim: false },
    Column { header: "Remote path", names: &["remotepath"], attribute: "RemotePath", trim: false },
    Column { header: "Jump host", names: &["jumphost", "bastion", "proxyjump"], attribute: "JumpHost", trim: true },
    Column { header: "Highlighting", names: &["highlighting", "highlightrules", "rules"], attribute: "HighlightRules", trim: true },
    Column { header: "Colour", names: &["colour", "color", "environmentcolour", "environmentcolor"], attribute: "Color", trim: true },
];

/// SuperXPuTTY's connection types, as the Proto attribute writes them
const PROTOCOLS: &[&str] = &["SSH", "Telnet", "Rlogin", "Raw", "Serial", "VNC", "RDP", "Mintty", "Cygterm", "WSL", "WINCMD", "PS"];

/// A header for matching: lower case, without spaces, `_` and `-`
fn normalize(header: &str) -> String {
    header.chars().filter(|c| !c.is_whitespace() && *c != '_' && *c != '-').collect::<String>().to_lowercase()
}

fn source_of(store: Store) -> &'static str {
    match store {
        Store::Saved => "Saved",
        Store::Private => "Private",
        Store::Database => "Database",
    }
}

/// A session's name as its id has it (the last part), which is what places it in the tree
fn name_of(session: &StoredSession) -> &str {
    session.id().rsplit('/').next().unwrap_or("")
}

/// Extra arguments with any `-pw` password written as [`MASKED_PASSWORD`]
pub fn mask_password(args: &str) -> String {
    replace_password(args, |_| MASKED_PASSWORD.to_string())
}

/// Whether the arguments have a `-pw` password
pub fn has_password(args: &str) -> bool {
    let mut found = false;
    replace_password(args, |p| {
        found |= !p.is_empty();
        p.to_string()
    });
    found
}

/// Imported extra arguments with a masked `-pw` (as exported) given the session's real password back, or dropped for
/// a session that has none, so re-importing an export never sets the password to asterisks
fn keep_password(imported: &str, current: &str) -> String {
    let mut masked = false;
    replace_password(imported, |p| {
        masked |= p == MASKED_PASSWORD;
        p.to_string()
    });
    if !masked {
        return imported.to_string();
    }
    let mut password = None;
    replace_password(current, |p| {
        password = Some(p.to_string());
        p.to_string()
    });
    match password.filter(|p| !p.is_empty()) {
        Some(p) => replace_password(imported, |_| p.clone()),
        None => {
            let words: Vec<&str> = imported.split_whitespace().collect();
            let mut kept = Vec::new();
            let mut i = 0;
            while i < words.len() {
                if words[i] == "-pw" && words.get(i + 1) == Some(&MASKED_PASSWORD) {
                    i += 2;
                    continue;
                }
                kept.push(words[i]);
                i += 1;
            }
            kept.join(" ")
        }
    }
}

// ---- Export

/// The sessions as CSV text: a header, then a row per session ordered by source, folder and name, CRLF line ends
pub fn to_csv(list: &[(Store, &StoredSession)]) -> String {
    let mut sorted: Vec<&(Store, &StoredSession)> = list.iter().collect();
    sorted.sort_by_key(|(store, s)| (source_of(*store), s.folder().to_lowercase(), name_of(s).to_lowercase()));
    let mut csv = String::new();
    let header: Vec<&str> = ["Source", "Folder", "Name"].into_iter().chain(COLUMNS.iter().map(|c| c.header)).collect();
    write_row(&mut csv, header.iter().map(|h| h.to_string()));
    for (store, session) in sorted {
        let values = [source_of(*store).to_string(), session.folder().to_string(), name_of(session).to_string()].into_iter().chain(
            COLUMNS.iter().map(|c| if c.attribute == "ExtraArgs" { mask_password(session.get(c.attribute)) } else { session.get(c.attribute).to_string() }),
        );
        write_row(&mut csv, values);
    }
    csv
}

/// Write sessions to a CSV file, as UTF-8 with a byte order mark so Excel reads accents and symbols right
pub fn export(list: &[(Store, &StoredSession)], path: &Path) -> Result<(), String> {
    std::fs::write(path, format!("\u{feff}{}", to_csv(list))).map_err(|e| e.to_string())
}

fn write_row(csv: &mut String, values: impl Iterator<Item = String>) {
    csv.push_str(&values.map(|v| quote(&v)).collect::<Vec<_>>().join(","));
    csv.push_str("\r\n");
}

/// One value for CSV: quoted when it holds a separator, quote or line break, with a `'` before a value starting
/// `= + - @` so a spreadsheet shows it as text rather than running it as a formula
fn quote(value: &str) -> String {
    let value = if value.starts_with(['=', '+', '-', '@']) { format!("'{value}") } else { value.to_string() };
    if value.contains([',', '"', '\r', '\n', ';', '\t']) { format!("\"{}\"", value.replace('"', "\"\"")) } else { value }
}

/// A value as read: without the `'` that [`quote`] put before a formula-like value
fn unquote(value: &str) -> &str {
    match value.strip_prefix('\'') {
        Some(rest) if rest.starts_with(['=', '+', '-', '@']) => rest,
        _ => value,
    }
}

// ---- Reading CSV

/// Split CSV text into rows, with quoted values holding separators, quotes and line breaks; the separator is the one
/// the header line has most of: a comma, or the `;` or tab some spreadsheets save with. Blank lines are left out.
pub fn parse_rows(text: &str) -> Vec<Vec<String>> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let first = text.split('\n').next().unwrap_or("");
    let separator = [',', ';', '\t'].into_iter().rev().max_by_key(|c| first.matches(*c).count()).unwrap_or(',');
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut value = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let add = |rows: &mut Vec<Vec<String>>, row: Vec<String>| {
        if row.iter().any(|v| !v.is_empty()) {
            rows.push(row);
        }
    };
    while let Some(c) = chars.next() {
        if quoted {
            if c == '"' && chars.peek() == Some(&'"') {
                value.push('"');
                chars.next();
            } else if c == '"' {
                quoted = false;
            } else {
                value.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == separator {
            row.push(std::mem::take(&mut value));
        } else if c == '\n' {
            row.push(std::mem::take(&mut value));
            add(&mut rows, std::mem::take(&mut row));
        } else if c != '\r' {
            value.push(c);
        }
    }
    row.push(value);
    add(&mut rows, row);
    rows
}

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
    /// "Line 4: no name", the first eight
    pub skipped: Vec<String>,
    pub ignored: Vec<String>,
    pub error: Option<String>,
    /// The first sessions it adds, then changes, "New Private: ImportedFromMobaXterm/sw1"
    pub examples: Vec<String>,
}

impl Plan {
    pub fn summary(&self) -> Summary {
        let go = || self.rows.iter().filter(|r| r.skip.is_none());
        Summary {
            adds: go().filter(|r| !r.update).count(),
            updates: go().filter(|r| r.update).count(),
            skips: self.rows.iter().filter(|r| r.skip.is_some()).count(),
            skipped: self
                .rows
                .iter()
                .filter_map(|r| r.skip.as_ref().map(|why| format!("Line {}: {why}", r.line)))
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
    fn find(&self, id: &str) -> Option<Store> {
        if self.saved.iter().any(|s| s.id() == id) {
            Some(Store::Saved)
        } else if self.private.iter().any(|s| s.id() == id) {
            Some(Store::Private)
        } else {
            None
        }
    }

    fn taken(&self, id: &str) -> bool {
        self.saved.iter().chain(self.private).any(|s| s.id().eq_ignore_ascii_case(id))
    }
}

/// Work out what importing CSV text would do (SessionCsv.Plan). Columns the file doesn't have are left as they are,
/// or their defaults for new sessions.
pub fn plan_csv(text: &str, existing: &Existing) -> Plan {
    let mut plan = Plan::default();
    let rows = parse_rows(text);
    let Some(header) = rows.first() else {
        plan.error = Some("The file is empty.".into());
        return plan;
    };
    let headers: Vec<String> = header.iter().map(|h| normalize(h)).collect();
    let find = |names: &[&str]| headers.iter().position(|h| names.contains(&h.as_str()));
    let (source, folder, name) = (find(&["source"]), find(&["folder", "path"]), find(&["name", "sessionname"]));
    let Some(name) = name else {
        plan.error = Some("The file has no Name column. The first line must name the columns, e.g. Folder,Name,Host,Username.".into());
        return plan;
    };
    let mut used: Vec<(usize, &Column)> = Vec::new();
    for (i, h) in headers.iter().enumerate() {
        match COLUMNS.iter().find(|c| c.names.contains(&h.as_str())) {
            Some(column) if !used.iter().any(|(_, c)| std::ptr::eq(*c, column)) => used.push((i, column)),
            _ if Some(i) != source && Some(i) != folder && i != name && !h.is_empty() => plan.ignored.push(header[i].clone()),
            _ => {}
        }
    }

    let mut seen = HashSet::new();
    for (r, row) in rows.iter().enumerate().skip(1) {
        let cell = |i: Option<usize>| i.and_then(|i| row.get(i)).map(|v| unquote(v)).unwrap_or("");
        let mut planned = Row { line: r + 1, skip: None, store: Store::Private, id: String::new(), update: false, unique: false, values: Vec::new() };
        let kind = cell(source).trim();
        let session_name = cell(Some(name)).trim();
        let path = cell(folder).trim().trim_matches('/');
        let skip = |mut planned: Row, why: String| {
            planned.skip = Some(why);
            planned
        };
        if kind.eq_ignore_ascii_case("Database") {
            plan.rows.push(skip(planned, "a Database session; change the table with SQL, or in the tree".into()));
            continue;
        }
        if !kind.is_empty() && !kind.eq_ignore_ascii_case("Saved") && !kind.eq_ignore_ascii_case("Private") {
            plan.rows.push(skip(planned, format!("Source \"{kind}\" isn't Saved, Private or Database")));
            continue;
        }
        if session_name.is_empty() || session_name.contains('/') {
            let why = if session_name.is_empty() { "no name".into() } else { format!("the name \"{session_name}\" has a /") };
            plan.rows.push(skip(planned, why));
            continue;
        }
        // with Saved sessions turned off, Saved rows go to Private sessions
        planned.store = if kind.eq_ignore_ascii_case("Private") || !existing.use_saved { Store::Private } else { Store::Saved };
        planned.id = if path.is_empty() { session_name.to_string() } else { format!("{path}/{session_name}") };
        if !seen.insert(planned.id.clone()) {
            let why = format!("\"{}\" is in the file twice; the first one is used", planned.id);
            plan.rows.push(skip(planned, why));
            continue;
        }
        match existing.find(&planned.id) {
            Some(store) if store != planned.store => {
                let why = format!("\"{}\" is already a {} session", planned.id, source_of(store).to_lowercase());
                plan.rows.push(skip(planned, why));
                continue;
            }
            Some(_) => planned.update = true,
            None if existing.taken(&planned.id) => {
                let why = format!("\"{}\" differs only in capitals from a session there", planned.id);
                plan.rows.push(skip(planned, why));
                continue;
            }
            None => {
                planned.values.push(("SessionName".into(), session_name.to_string()));
                planned.values.push(("Port".into(), "0".into()));
            }
        }
        let mut problem = None;
        for (i, column) in &used {
            let raw = cell(Some(*i));
            let value = if column.trim { raw.trim() } else { raw };
            let value = match column.attribute {
                "Proto" if value.is_empty() => "SSH".to_string(),
                "Proto" => match PROTOCOLS.iter().find(|p| p.eq_ignore_ascii_case(value)) {
                    Some(p) => p.to_string(),
                    None => {
                        problem = Some(format!("unknown protocol \"{raw}\""));
                        break;
                    }
                },
                "Port" if value.is_empty() => "0".into(),
                "Port" => match value.parse::<u16>() {
                    Ok(port) => port.to_string(),
                    Err(_) => {
                        problem = Some(format!("port \"{raw}\" isn't 0 to 65535"));
                        break;
                    }
                },
                "Color" if value.is_empty() => String::new(),
                "Color" => match normalize_color(value) {
                    Some(color) => color,
                    None => {
                        problem = Some(format!("colour \"{raw}\" isn't #RRGGBB"));
                        break;
                    }
                },
                _ => value.to_string(),
            };
            planned.values.push((column.attribute.to_string(), value));
        }
        if let Some(why) = problem {
            plan.rows.push(skip(planned, why));
            continue;
        }
        plan.rows.push(planned);
    }
    if plan.rows.is_empty() {
        plan.error = Some("The file has column names but no sessions.".into());
    }
    plan
}

/// "#c62828", "C62828" or "#C62828" as SuperXPuTTY stores colours, "#C62828"; None if it isn't one
pub fn normalize_color(value: &str) -> Option<String> {
    let hex = value.trim().trim_start_matches('#');
    (hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit())).then(|| format!("#{}", hex.to_uppercase()))
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

// ---- Planning by kind

/// The default ~/.ssh/config
pub fn ssh_config_path() -> PathBuf {
    dirs::home_dir().unwrap_or_default().join(".ssh/config")
}

/// Plan an import: `kind` is "csv", "mobaxterm", "rdp" (one or more files) or "ssh"; `store` is where new sessions
/// from other apps go (CSV rows say their own)
pub fn plan(kind: &str, paths: &[PathBuf], store: Store, existing: &Existing) -> Result<Plan, String> {
    let first = paths.first().ok_or("Choose a file.")?;
    match kind {
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
            for row in rows {
                let taken = |list: &Vec<StoredSession>, id: &str| {
                    list.iter().any(|s| s.id().eq_ignore_ascii_case(id)) || elsewhere.iter().any(|o| o.eq_ignore_ascii_case(id))
                };
                let mut id = row.id.clone();
                if row.unique {
                    // as SuperXPuTTY's MakeUniqueSessionId: "name-1", "name-2", ...
                    let mut n = 1;
                    while taken(list, &id) && n < 1000 {
                        id = format!("{}-{n}", row.id);
                        n += 1;
                    }
                }
                if let Some(session) = list.iter_mut().find(|s| s.id() == id) {
                    let current = session.get("ExtraArgs").to_string();
                    for (name, value) in &row.values {
                        let value = if name == "ExtraArgs" { keep_password(value, &current) } else { value.clone() };
                        session.set(name, &value);
                    }
                    updated += 1;
                    continue;
                }
                if taken(list, &id) {
                    continue;
                }
                let mut session = StoredSession::new(&id);
                for (name, value) in &row.values {
                    let value = if name == "ExtraArgs" { keep_password(value, "") } else { value.clone() };
                    session.set(name, &value);
                }
                session.set("SessionName", id.rsplit('/').next().unwrap_or(&id));
                sessions::insert_sorted(list, session);
                added += 1;
            }
            Ok(())
        })?;
    }
    Ok((added, updated))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(id: &str, attributes: &[(&str, &str)]) -> StoredSession {
        let mut s = StoredSession::new(id);
        for (n, v) in attributes {
            s.set(n, v);
        }
        s
    }

    #[test]
    fn exports_and_reimports_csv() {
        let sw1 = session("Lab/sw1", &[("Host", "10.0.0.1"), ("ExtraArgs", "-pw cisco -L 8080:localhost:80"), ("Note", "core, \"main\"")]);
        let db = session("@db/Sites/rtr", &[("Host", "10.1.0.1")]);
        let csv = to_csv(&[(Store::Saved, &sw1), (Store::Database, &db)]);
        let lines: Vec<&str> = csv.split("\r\n").collect();
        assert!(lines[0].starts_with("Source,Folder,Name,Protocol,Host,Port"));
        assert!(lines[1].starts_with("Database,Sites,rtr,SSH,10.1.0.1,22"));
        assert!(lines[2].contains("'-pw ******** -L 8080:localhost:80"), "{}", lines[2]);
        assert!(lines[2].contains("\"core, \"\"main\"\"\""));

        // the export back: the Database row skipped, the session updated with its password kept
        let saved = vec![sw1.clone()];
        let existing = Existing { saved: &saved, private: &[], use_saved: true };
        let plan = plan_csv(&csv, &existing);
        let summary = plan.summary();
        assert_eq!((summary.adds, summary.updates, summary.skips), (0, 1, 1));
        let row = plan.rows.iter().find(|r| r.skip.is_none()).unwrap();
        let args = &row.values.iter().find(|(n, _)| n == "ExtraArgs").unwrap().1;
        assert_eq!(keep_password(args, sw1.get("ExtraArgs")), "-pw cisco -L 8080:localhost:80");
        assert_eq!(keep_password(args, ""), "-L 8080:localhost:80");
    }

    #[test]
    fn reads_semicolons_and_checks_rows() {
        let text = "Folder;Name;Port;Colour;Owner\nLab;a;22;c62828;me\nLab;b;99999;;\n;;;;\nLab;a;;;\n;c/d;;;\n";
        let existing = Existing { saved: &[], private: &[], use_saved: false };
        let plan = plan_csv(text, &existing);
        assert_eq!(plan.ignored, vec!["Owner".to_string()]);
        let s = plan.summary();
        assert_eq!((s.adds, s.updates, s.skips), (1, 0, 3));
        assert_eq!(plan.rows[0].store, Store::Private, "Saved off: rows go to Private");
        assert!(plan.rows[0].values.contains(&("Color".into(), "#C62828".into())));
        assert!(s.skipped[0].contains("isn't 0 to 65535"));
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
}
