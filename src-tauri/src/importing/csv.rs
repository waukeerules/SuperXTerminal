//! Sessions as CSV: File > Export Sessions to CSV, its template, and reading a CSV for an import or for Edit
//! Sessions as Table.
//!
//! It is SuperXPuTTY's `SessionCsv`: the same columns and headers (those of the Database sessions table, so a table
//! exported from Postgres imports too), a `-pw` password written as `********` and kept on re-import, a leading `'`
//! before a value a spreadsheet would take for a formula, and `;` or tab separators read as well as commas. A row is
//! matched to a session by its Source, Folder and Name: an existing Saved or Private session is updated, otherwise
//! one is added; Database rows are skipped.

use std::collections::HashSet;
use std::path::Path;

use serde::Serialize;

use super::{read_text, Existing, Plan, Row, PROTOCOLS};
use crate::editing::replace_password;
use crate::sessions::{Store, StoredSession};

/// What a `-pw` password is exported as
pub const MASKED_PASSWORD: &str = "********";

/// One CSV column: the header written, the headers read (normalized), and the session attribute
pub(crate) struct Column {
    header: &'static str,
    names: &'static [&'static str],
    attribute: &'static str,
    /// Leading and trailing spaces are dropped on import
    trim: bool,
}

/// Every column after Source, Folder and Name, in the order exported (SessionCsv.Columns)
pub(crate) const COLUMNS: &[Column] = &[
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
    // SuperXTerminal's own, as the table's ssh_algorithms column
    Column { header: "SSH algorithms", names: &["sshalgorithms", "algorithms"], attribute: "SshAlgorithms", trim: true },
    Column { header: "Button bar", names: &["buttonbar", "bar"], attribute: "ButtonBar", trim: true },
];

/// A header for matching: lower case, without spaces, `_` and `-`
pub(crate) fn normalize(header: &str) -> String {
    header.chars().filter(|c| !c.is_whitespace() && *c != '_' && *c != '-').collect::<String>().to_lowercase()
}

/// The Source column's word for a store, as SuperXPuTTY's CSV has it
pub(crate) fn source_of(store: Store) -> &'static str {
    match store {
        Store::Saved => "Saved",
        Store::Private => "Private",
        Store::Database => "Database",
    }
}

/// A session's name as its id has it (the last part), which is what places it in the tree
pub(crate) fn name_of(session: &StoredSession) -> &str {
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
pub(crate) fn keep_password(imported: &str, current: &str) -> String {
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

/// One example row of the template: its Source, Folder and Name, then its session attributes by name
type TemplateRow = (&'static str, &'static str, &'static str, &'static [(&'static str, &'static str)]);

/// The template's example rows, for File > Import Sessions > CSV's Save a Template: Source, Folder, Name, then
/// session attributes by name. Made-up addresses (10.0.0.x, example.test), one of each common kind, so a new user has
/// the columns and what goes in them to copy
pub(crate) const TEMPLATE_ROWS: &[TemplateRow] = &[
    ("Saved", "Lab/Switches", "core-sw1", &[
        ("Proto", "SSH"), ("Host", "10.0.0.1"), ("Port", "22"), ("Username", "admin"), ("PuttySession", "Cisco"),
        ("ImageKey", "switch"), ("Note", "Core switch; its profile (PuTTY profile) holds the login"), ("Color", "#2E7D32"),
    ]),
    ("Private", "Lab/Linux", "web1", &[
        ("Proto", "SSH"), ("Host", "web1.example.test"), ("Username", "root"), ("JumpHost", "admin@10.0.0.254:22"),
        ("Note", "Port left empty: the protocol's own (22). Through a jump host"),
    ]),
    ("Saved", "Lab/Consoles", "console-com3", &[
        ("Proto", "Serial"), ("Host", "COM3"), ("ExtraArgs", "-sercfg 9600,8,n,1,N"), ("Note", "A serial console: its port, then speed and line settings"),
    ]),
    ("Saved", "Lab/Desktops", "desk1", &[
        ("Proto", "RDP"), ("Host", "10.0.0.20"), ("Port", "3389"), ("Username", "user"), ("Note", "Remote Desktop"),
    ]),
];

/// A CSV template: the header the import reads and the export writes, then [`TEMPLATE_ROWS`]
pub fn template() -> String {
    let mut csv = String::new();
    let header: Vec<&str> = ["Source", "Folder", "Name"].into_iter().chain(COLUMNS.iter().map(|c| c.header)).collect();
    write_row(&mut csv, header.iter().map(|h| h.to_string()));
    for (source, folder, name, values) in TEMPLATE_ROWS {
        let value = |attribute: &str| values.iter().find(|(a, _)| *a == attribute).map_or(String::new(), |(_, v)| v.to_string());
        write_row(&mut csv, [source.to_string(), folder.to_string(), name.to_string()].into_iter().chain(COLUMNS.iter().map(|c| value(c.attribute))));
    }
    csv
}

/// Write the template to a file, as an export is written (UTF-8 with a byte order mark, for Excel)
pub fn write_template(path: &Path) -> Result<(), String> {
    std::fs::write(path, format!("\u{feff}{}", template())).map_err(|e| e.to_string())
}

/// Add a row of values to the CSV, each quoted as it needs, ending with CRLF as SuperXPuTTY's
pub(crate) fn write_row(csv: &mut String, values: impl Iterator<Item = String>) {
    csv.push_str(&values.map(|v| quote(&v)).collect::<Vec<_>>().join(","));
    csv.push_str("\r\n");
}

/// One value for CSV: quoted when it holds a separator, quote or line break, with a `'` before a value starting
/// `= + - @` so a spreadsheet shows it as text rather than running it as a formula
pub(crate) fn quote(value: &str) -> String {
    let value = if value.starts_with(['=', '+', '-', '@']) { format!("'{value}") } else { value.to_string() };
    if value.contains([',', '"', '\r', '\n', ';', '\t']) { format!("\"{}\"", value.replace('"', "\"\"")) } else { value }
}

/// A value as read: without the `'` that [`quote`] put before a formula-like value
pub(crate) fn unquote(value: &str) -> &str {
    match value.strip_prefix('\'') {
        Some(rest) if rest.starts_with(['=', '+', '-', '@']) => rest,
        _ => value,
    }
}

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

/// A CSV cell as its session attribute keeps it: trimmed where the column is, a protocol in SuperXPuTTY's spelling
/// (SSH when empty), a port 0 to 65535 (0 when empty), a colour as #RRGGBB; or why it can't be one. Shared by Import
/// Sessions > CSV and Edit Sessions as Table…'s Import CSV…, so both take a file the same way
pub(crate) fn cell_value(column: &Column, raw: &str) -> Result<String, String> {
    let value = if column.trim { raw.trim() } else { raw };
    match column.attribute {
        "Proto" if value.is_empty() => Ok("SSH".to_string()),
        "Proto" => PROTOCOLS.iter().find(|p| p.eq_ignore_ascii_case(value)).map(|p| p.to_string()).ok_or_else(|| format!("unknown protocol \"{raw}\"")),
        "Port" if value.is_empty() => Ok("0".into()),
        "Port" => value.parse::<u16>().map(|port| port.to_string()).map_err(|_| format!("port \"{raw}\" isn't 0 to 65535")),
        "Color" if value.is_empty() => Ok(String::new()),
        "Color" => normalize_color(value).ok_or_else(|| format!("colour \"{raw}\" isn't #RRGGBB")),
        _ => Ok(value.to_string()),
    }
}

/// One CSV row for Edit Sessions as Table…'s Import CSV…: where it says the session is (Source as written, "" for
/// none), its folder and name, the attributes the file has columns for, or why it can't be used
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvSession {
    pub line: usize,
    pub source: String,
    pub folder: String,
    pub name: String,
    pub values: Vec<(String, String)>,
    pub problem: Option<String>,
}

/// A CSV file read for Edit Sessions as Table…: its rows, the columns it has that aren't used, or why none can be
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CsvSessions {
    pub sessions: Vec<CsvSession>,
    pub ignored: Vec<String>,
    pub error: Option<String>,
}

/// Read a CSV of sessions for Edit Sessions as Table…'s Import CSV…, as [`plan_csv`] reads one (the same columns,
/// headers and checks) but without deciding anything: the table matches each row to its sessions, Database ones
/// included, and nothing is saved until its Save
pub fn read_csv(text: &str) -> CsvSessions {
    let mut out = CsvSessions::default();
    let rows = parse_rows(text);
    let Some(header) = rows.first() else {
        out.error = Some("The file is empty.".into());
        return out;
    };
    let headers: Vec<String> = header.iter().map(|h| normalize(h)).collect();
    let find = |names: &[&str]| headers.iter().position(|h| names.contains(&h.as_str()));
    let (source, folder, name) = (find(&["source"]), find(&["folder", "path"]), find(&["name", "sessionname"]));
    let Some(name) = name else {
        out.error = Some("The file has no Name column. The first line must name the columns, e.g. Folder,Name,Host,Username.".into());
        return out;
    };
    let mut used: Vec<(usize, &Column)> = Vec::new();
    for (i, h) in headers.iter().enumerate() {
        match COLUMNS.iter().find(|c| c.names.contains(&h.as_str())) {
            Some(column) if !used.iter().any(|(_, c)| std::ptr::eq(*c, column)) => used.push((i, column)),
            _ if Some(i) != source && Some(i) != folder && i != name && !h.is_empty() => out.ignored.push(header[i].clone()),
            _ => {}
        }
    }
    for (r, row) in rows.iter().enumerate().skip(1) {
        let cell = |i: Option<usize>| i.and_then(|i| row.get(i)).map(|v| unquote(v)).unwrap_or("");
        let mut session = CsvSession {
            line: r + 1,
            source: cell(source).trim().to_string(),
            folder: cell(folder).trim().trim_matches('/').to_string(),
            name: cell(Some(name)).trim().to_string(),
            values: Vec::new(),
            problem: None,
        };
        // a blank line at the end, as spreadsheets leave
        if row.iter().all(|c| unquote(c).trim().is_empty()) {
            continue;
        }
        if session.name.is_empty() || session.name.contains('/') {
            session.problem = Some(if session.name.is_empty() { "no name".into() } else { format!("the name \"{}\" has a /", session.name) });
        } else if !session.source.is_empty() && !["saved", "private", "database"].contains(&session.source.to_lowercase().as_str()) {
            session.problem = Some(format!("Source \"{}\" isn't Saved, Private or Database", session.source));
        } else {
            for (i, column) in &used {
                match cell_value(column, cell(Some(*i))) {
                    Ok(value) => session.values.push((column.attribute.to_string(), value)),
                    Err(why) => {
                        session.problem = Some(why);
                        break;
                    }
                }
            }
        }
        out.sessions.push(session);
    }
    if out.sessions.is_empty() {
        out.error = Some("The file has column names but no sessions.".into());
    }
    out
}

/// Read a CSV file of sessions for Edit Sessions as Table…'s Import CSV… ([`read_csv`])
pub fn read_csv_file(path: &Path) -> Result<CsvSessions, String> {
    Ok(read_csv(&read_text(path)?))
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
            match cell_value(column, cell(Some(*i))) {
                Ok(value) => planned.values.push((column.attribute.to_string(), value)),
                Err(why) => {
                    problem = Some(why);
                    break;
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importing::tests::session;

    #[test]
    fn a_csv_for_the_table_keeps_each_rows_place_and_values_and_says_whats_wrong_with_a_row() {
        let text = "\u{feff}Source,Folder,Name,Protocol,Port,Colour,Extra args,Shoe size\r\n\
                    Saved,Lab/,sw1,ssh,2222,c62828,-pw ********,44\r\n\
                    Database,Branch,rtr1,Telnet,,,,\r\n\
                    ,,nas,SSH,22,,,\r\n\
                    Saved,Lab,sw2,Gopher,22,,,\r\n\
                    Elsewhere,Lab,sw3,SSH,22,,,\r\n\
                    Saved,Lab,,SSH,22,,,\r\n\
                    ,,,,,,,\r\n";
        let read = read_csv(text);
        assert_eq!(read.error, None);
        assert_eq!(read.ignored, ["Shoe size"]);
        let by_name: Vec<(&str, &str, &str, Option<&str>)> =
            read.sessions.iter().map(|s| (s.source.as_str(), s.folder.as_str(), s.name.as_str(), s.problem.as_deref())).collect();
        assert_eq!(by_name, [
            ("Saved", "Lab", "sw1", None),
            ("Database", "Branch", "rtr1", None),
            ("", "", "nas", None),
            ("Saved", "Lab", "sw2", Some("unknown protocol \"Gopher\"")),
            ("Elsewhere", "Lab", "sw3", Some("Source \"Elsewhere\" isn't Saved, Private or Database")),
            ("Saved", "Lab", "", Some("no name")),
        ]);
        // values as the import keeps them: SuperXPuTTY's spelling, the colour in capitals, an empty port 0, the masked
        // -pw as written (the table puts the session's own back)
        let sw1: std::collections::HashMap<&str, &str> = read.sessions[0].values.iter().map(|(a, v)| (a.as_str(), v.as_str())).collect();
        assert_eq!((sw1["Proto"], sw1["Port"], sw1["Color"], sw1["ExtraArgs"]), ("SSH", "2222", "#C62828", "-pw ********"));
        assert_eq!(read.sessions[1].values.iter().find(|(a, _)| a == "Port").map(|(_, v)| v.as_str()), Some("0"));
        assert!(read_csv("Folder,Host\r\nLab,10.0.0.1\r\n").error.unwrap().contains("no Name column"));
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
    fn the_template_has_every_column_and_imports_as_it_is() {
        let csv = template();
        let lines: Vec<&str> = csv.split("\r\n").filter(|l| !l.is_empty()).collect();
        // the export's header, so a template and an export are the same shape
        assert_eq!(lines[0], to_csv(&[]).trim_end());
        assert_eq!(lines.len(), 1 + TEMPLATE_ROWS.len());
        let existing = Existing { saved: &[], private: &[], use_saved: true };
        let plan = plan_csv(&csv, &existing);
        let summary = plan.summary();
        assert_eq!((summary.adds, summary.updates, summary.skips), (TEMPLATE_ROWS.len(), 0, 0), "{:?}", summary.skipped);
        assert!(plan.ignored.is_empty(), "{:?}", plan.ignored);
        let serial = plan.rows.iter().find(|r| r.values.contains(&("Host".into(), "COM3".into()))).unwrap();
        assert!(serial.values.contains(&("ExtraArgs".into(), "-sercfg 9600,8,n,1,N".into())), "{:?}", serial.values);
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
}
