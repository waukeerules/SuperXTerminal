//! Saved and Private sessions, read from SuperXPuTTY's own files.
//!
//! `Sessions.xml` (in the settings folder, often shared by a team) and `PrivateSessions.xml` (per user) are
//! SuperXPuTTY's `ArrayOfSessionData`: one `<SessionData>` element per session, every field an attribute. Each session
//! keeps its attributes in their original order, unknown ones included, so a session saved here keeps fields that a
//! newer SuperXPuTTY added (docs/PLAN.md §3). Folder settings (colours, defaults) are in `Folders.xml` and
//! `PrivateFolders.xml` beside them.
//!
//! The UI gets a [`SessionView`] per session, never the stored attributes: passwords in `ExtraArgs` (`-pw`) stay in
//! the Rust core, which builds the connection itself (`launch.rs`).

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

/// Which file a session comes from
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Store {
    /// `Sessions.xml` in the settings folder
    Saved,
    /// `PrivateSessions.xml` in the user's own folder
    Private,
    /// The Database sessions table (`db.rs`), as last read or cached
    Database,
}

/// One session as stored: its attributes in file order
#[derive(Clone, Debug, Default)]
pub struct StoredSession {
    pub attributes: Vec<(String, String)>,
}

impl StoredSession {
    /// An attribute's value, or "" when it's missing
    pub fn get(&self, name: &str) -> &str {
        self.attributes.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str()).unwrap_or("")
    }

    /// The session's id: its folder path and name, e.g. "Test/cmd Test"
    pub fn id(&self) -> &str {
        self.get("SessionId")
    }

    /// The folder part of the id ("" at the top), as SuperXPuTTY's `GetSessionParentId`; for a Database session
    /// (id "@db/<folder>/<name>"), the table's own folder
    pub fn folder(&self) -> &str {
        let id = self.id().strip_prefix("@db/").unwrap_or(self.id());
        id.rsplit_once('/').map(|(folder, _)| folder).unwrap_or("")
    }

    /// The port, or 0 when it isn't set or isn't a number
    pub fn port(&self) -> u16 {
        self.get("Port").trim().parse().unwrap_or(0)
    }
}

/// One folder's own settings (SuperXPuTTY's `FolderSetting`), inherited by the sessions and folders under it
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderSetting {
    pub path: String,
    pub color: String,
    pub username: String,
    pub putty_profile: String,
    pub port: u16,
    pub jump_host: String,
    pub highlight_rules: String,
}

/// What the UI shows for a session: everything but secrets (and, for Database sessions, who changed it last and
/// whether it's locked)
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub id: String,
    pub store: Store,
    pub name: String,
    pub folder: String,
    pub protocol: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub profile: String,
    pub icon: String,
    pub color: String,
    pub note: String,
    /// The extra arguments with any -pw value masked, and the SPSL script file: for the search (Search in…)
    pub extra_args: String,
    pub script: String,
    /// SuperXTerminal's own: the button bar shown while its tab is active ("" for its profile's, else the tab icon's)
    pub button_bar: String,
    pub updated_by: String,
    pub locked: bool,
    /// Database sessions: the columns locked in the table, comma-separated ("host,port")
    pub locked_fields: String,
    /// Database sessions: a change not in the table yet (waiting, or refused), and what became of it
    pub pending: bool,
    pub pending_note: String,
}

/// Every session and folder setting, as last read
#[derive(Default)]
pub struct Sessions {
    pub saved: Vec<StoredSession>,
    pub private: Vec<StoredSession>,
    pub database: Vec<StoredSession>,
    /// Each store's folder settings, by folder path in lower case
    pub folders: HashMap<Store, HashMap<String, FolderSetting>>,
}

/// What `sessions_load` tells the UI: the sessions, the folders' settings, and any file that couldn't be read
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionsLoaded {
    pub sessions: Vec<SessionView>,
    pub folders: Vec<(Store, FolderSetting)>,
    pub problems: Vec<String>,
    pub saved_file: Option<String>,
    pub private_file: String,
    /// Saved sessions are turned off (Settings > General > Use Sessions.xml)
    pub saved_off: bool,
}

impl Sessions {
    /// Read both session files and both folder files; a missing file is no sessions, an unreadable one a problem
    pub fn load(settings_folder: Option<&Path>, user_folder: &Path, problems: &mut Vec<String>) -> Sessions {
        let mut sessions = Sessions::default();
        if let Some(folder) = settings_folder {
            sessions.saved = read_sessions(&folder.join("Sessions.xml"), problems);
            sessions.folders.insert(Store::Saved, read_folders(&folder.join("Folders.xml"), problems));
        }
        sessions.private = read_sessions(&user_folder.join("PrivateSessions.xml"), problems);
        sessions.folders.insert(Store::Private, read_folders(&user_folder.join("PrivateFolders.xml"), problems));
        sessions
    }

    /// A session by store and id
    pub fn find(&self, store: Store, id: &str) -> Option<&StoredSession> {
        let list = match store {
            Store::Saved => &self.saved,
            Store::Private => &self.private,
            Store::Database => &self.database,
        };
        list.iter().find(|s| s.id() == id)
    }

    /// The folder settings along a folder's path, nearest first, as SuperXPuTTY's `FolderSettings.Along`
    pub fn along(&self, store: Store, folder: &str) -> Vec<&FolderSetting> {
        let mut found = Vec::new();
        let Some(settings) = self.folders.get(&store) else { return found };
        let mut current = folder.trim_matches('/');
        while !current.is_empty() {
            if let Some(setting) = settings.get(&current.to_lowercase()) {
                found.push(setting);
            }
            current = current.rsplit_once('/').map(|(parent, _)| parent).unwrap_or("");
        }
        found
    }

    /// The UI's view of every session
    pub fn views(&self) -> Vec<SessionView> {
        let saved = self.saved.iter().map(|s| view(s, Store::Saved));
        let private = self.private.iter().map(|s| view(s, Store::Private));
        let database = self.database.iter().map(|s| view(s, Store::Database));
        saved.chain(private).chain(database).collect()
    }
}

/// The UI's view of one session
fn view(session: &StoredSession, store: Store) -> SessionView {
    let id = session.id().to_string();
    let name = match session.get("SessionName") {
        "" => id.rsplit('/').next().unwrap_or("").to_string(),
        name => name.to_string(),
    };
    SessionView {
        store,
        name,
        folder: session.folder().to_string(),
        protocol: session.get("Proto").to_string(),
        host: session.get("Host").to_string(),
        port: session.port(),
        username: session.get("Username").to_string(),
        profile: session.get("PuttySession").to_string(),
        icon: session.get("ImageKey").to_string(),
        color: session.get("Color").to_string(),
        note: session.get("Note").to_string(),
        extra_args: masked_password(session.get("ExtraArgs")),
        script: session.get("SPSLFileName").to_string(),
        button_bar: session.get("ButtonBar").to_string(),
        updated_by: session.get("DatabaseUpdatedBy").to_string(),
        pending: session.get("DatabasePending") == "true",
        pending_note: session.get("DatabasePendingNote").to_string(),
        locked: session.get("DatabaseLocked") == "true",
        locked_fields: session.get("DatabaseLockedFields").to_string(),
        id,
    }
}

/// Extra arguments with the word after -pw masked, so searching them can't find a session by its password
fn masked_password(extra: &str) -> String {
    let mut out = Vec::new();
    let mut hide = false;
    for word in extra.split_whitespace() {
        out.push(if hide { "********" } else { word });
        hide = word == "-pw";
    }
    out.join(" ")
}

/// Read the sessions of a file with `<SessionData>` elements (Sessions.xml, the Database cache)
pub fn read_sessions_file(path: &Path, problems: &mut Vec<String>) -> Vec<StoredSession> {
    read_sessions(path, problems)
}

/// Read the folder settings of a file with `<FolderSetting>` elements (Folders.xml, the Database cache)
pub fn read_folder_settings(path: &Path, problems: &mut Vec<String>) -> Vec<FolderSetting> {
    read_folders(path, problems).into_values().collect()
}

/// Read an `ArrayOfSessionData` file; no file means no sessions
fn read_sessions(path: &Path, problems: &mut Vec<String>) -> Vec<StoredSession> {
    match read_elements(path, "SessionData") {
        Ok(elements) => elements.into_iter().map(|attributes| StoredSession { attributes }).collect(),
        Err(problem) => {
            problems.extend(problem);
            Vec::new()
        }
    }
}

/// Read a `Folders` file (`<Folders><FolderSetting Path=... />...`) into settings by folder path
fn read_folders(path: &Path, problems: &mut Vec<String>) -> HashMap<String, FolderSetting> {
    let elements = match read_elements(path, "FolderSetting") {
        Ok(elements) => elements,
        Err(problem) => {
            problems.extend(problem);
            return HashMap::new();
        }
    };
    let mut folders = HashMap::new();
    for attributes in elements {
        let get = |name: &str| attributes.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()).unwrap_or_default();
        let setting = FolderSetting {
            path: get("Path").trim_matches('/').to_string(),
            color: get("Color"),
            username: get("Username"),
            putty_profile: get("PuttyProfile"),
            port: get("Port").parse().unwrap_or(0),
            jump_host: get("JumpHost"),
            highlight_rules: get("HighlightRules"),
        };
        if !setting.path.is_empty() {
            // keyed in lower case: "test" and "Test" are one folder, as SuperXPuTTY's FolderSettings
            folders.insert(setting.path.to_lowercase(), setting);
        }
    }
    folders
}

/// Every element of the given name in an XML file, as its attributes in order
///
/// Returns no elements when the file doesn't exist, and `Err` with a message when it can't be read or parsed.
pub(crate) fn read_elements(path: &Path, element: &str) -> Result<Vec<Vec<(String, String)>>, Option<String>> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(Some(format!("{}: {e}", path.display()))),
    };
    let mut reader = Reader::from_str(text.trim_start_matches('\u{feff}'));
    let mut found = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) if e.name().as_ref() == element => {
                let mut attributes = Vec::new();
                for attribute in e.attributes() {
                    let attribute = attribute.map_err(|err| Some(format!("{}: {err}", path.display())))?;
                    let name = attribute.key.as_ref().to_string();
                    let value = attribute
                        .normalized_value(quick_xml::XmlVersion::Implicit1_0)
                        .map_err(|err| Some(format!("{}: {err}", path.display())))?
                        .to_string();
                    attributes.push((name, value));
                }
                found.push(attributes);
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(Some(format!("{}: {e}", path.display()))),
            _ => {}
        }
    }
    Ok(found)
}

/// The user's own folder: per-user files (PrivateSessions.xml, settings) as in docs/PLAN.md §3
pub fn user_folder() -> PathBuf {
    if cfg!(target_os = "macos") {
        dirs::home_dir().unwrap_or_default().join("Library/Application Support/SuperXTerminal")
    } else {
        dirs::config_dir().unwrap_or_default().join("SuperXTerminal")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_shown_to_the_search_has_its_password_masked() {
        assert_eq!(super::masked_password("-l admin -pw cisco -L 80:x:80"), "-l admin -pw ******** -L 80:x:80");
        assert_eq!(super::masked_password(""), "");
    }

    /// A file as SuperXPuTTY's XmlSerializer writes it: CRLF, escaped characters, and an attribute this app doesn't know
    const SESSIONS: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<ArrayOfSessionData xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\r\n  <SessionData SessionId=\"Lab/Core/sw &amp; rtr\" Color=\"#C62828\" SessionName=\"sw &amp; rtr\" Host=\"10.0.0.1\" Note=\"line one&#xA;line two\" Port=\"0\" Proto=\"SSH\" FutureField=\"kept\" />\r\n</ArrayOfSessionData>\r\n";

    /// The folders' file as SuperXPuTTY writes it, with defaults for a folder and a subfolder
    const FOLDERS: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<Folders>\r\n  <FolderSetting Path=\"Lab\" Color=\"#2E7D32\" Username=\"netops\" Port=\"2222\" />\r\n  <FolderSetting Path=\"Lab/Core\" Username=\"core\" />\r\n</Folders>\r\n";

    #[test]
    fn reads_sessions_and_folder_defaults() {
        let dir = std::env::temp_dir().join(format!("sxt-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("Sessions.xml"), SESSIONS).unwrap();
        std::fs::write(dir.join("Folders.xml"), FOLDERS).unwrap();
        let mut problems = Vec::new();
        let sessions = Sessions::load(Some(&dir), &dir.join("user"), &mut problems);
        std::fs::remove_dir_all(&dir).unwrap();
        // writing it back gives the same text, so an unchanged session is left exactly as it was
        assert_eq!(write_sessions(&sessions.saved), SESSIONS.trim_end());

        assert!(problems.is_empty(), "{problems:?}");
        let session = &sessions.saved[0];
        assert_eq!(session.id(), "Lab/Core/sw & rtr");
        assert_eq!(session.folder(), "Lab/Core");
        assert_eq!(session.get("Note"), "line one\nline two");
        assert_eq!(session.attributes.last().unwrap(), &("FutureField".to_string(), "kept".to_string()));
        let along: Vec<_> = sessions.along(Store::Saved, "Lab/Core").iter().map(|f| f.username.as_str()).collect();
        assert_eq!(along, ["core", "netops"]);
    }

    #[test]
    fn set_keeps_order_and_places_new_attributes_as_superxputty_writes_them() {
        let mut session = StoredSession::new("Lab/sw1");
        session.attributes.push(("FutureField".into(), "kept".into()));
        session.set("Host", "10.0.0.9");
        session.set("Color", "#C62828");
        session.set("HighlightRules", "cisco");
        let names: Vec<_> = session.attributes.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(&names[..5], ["SessionId", "Color", "HighlightRules", "SessionName", "ImageKey"]);
        assert_eq!(*names.last().unwrap(), "FutureField");
        session.set("Color", "");
        assert!(session.attributes.iter().all(|(n, _)| n != "Color"));
        assert_eq!(session.get("Host"), "10.0.0.9");
    }

    /// Real files write back unchanged: `SXT_ROUNDTRIP=/path/to/folder cargo test -- --ignored round_trip`
    #[test]
    #[ignore]
    fn round_trip_real_files() {
        let folder = std::env::var("SXT_ROUNDTRIP").expect("SXT_ROUNDTRIP: a folder holding session files");
        for name in ["Sessions.xml", "PrivateSessions.xml"] {
            let path = Path::new(&folder).join(name);
            let Ok(original) = std::fs::read_to_string(&path) else { continue };
            let mut problems = Vec::new();
            let sessions = read_sessions(&path, &mut problems);
            assert!(problems.is_empty(), "{problems:?}");
            assert_eq!(write_sessions(&sessions), original.trim_start_matches('\u{feff}'), "{name}");
        }
    }
}

/// SuperXPuTTY's attribute order (SessionData's XmlAttribute order), for placing attributes a session didn't have
const ATTRIBUTE_ORDER: [&str; 27] = [
    "SessionId", "DatabaseId", "DatabaseVersion", "DatabaseLocked", "DatabaseLockedFields", "DatabaseManagedBy",
    "DatabaseUpdatedBy", "Color", "JumpHost", "HighlightRules", "SessionName", "ImageKey", "Host", "Note", "Port",
    "Proto", "PuttySession", "Username", "ExtraArgs", "SPSLFileName", "RdpOptions", "SshAlgorithms", "ButtonBar", "CollectionID",
    "CollectionLocation", "RemotePath", "LocalPath",
];

/// Attributes XmlSerializer only writes when they're set (SessionData's ShouldSerialize methods), and
/// SuperXTerminal's own SshAlgorithms (algorithms.rs) and ButtonBar, which SuperXPuTTY doesn't know
const WRITTEN_ONLY_WHEN_SET: [&str; 7] = ["DatabaseLocked", "Color", "JumpHost", "HighlightRules", "RdpOptions", "SshAlgorithms", "ButtonBar"];

impl StoredSession {
    /// A new session with SuperXPuTTY's attributes, as XmlSerializer writes a new SessionData
    pub fn new(id: &str) -> StoredSession {
        let name = id.rsplit('/').next().unwrap_or(id);
        let defaults = [
            ("SessionId", id), ("SessionName", name), ("ImageKey", "computer"), ("Host", ""), ("Note", ""), ("Port", "22"),
            ("Proto", "SSH"), ("PuttySession", "Default Settings"), ("Username", ""), ("ExtraArgs", ""), ("SPSLFileName", ""),
            ("CollectionID", ""), ("CollectionLocation", ""), ("RemotePath", ""), ("LocalPath", ""),
        ];
        StoredSession { attributes: defaults.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect() }
    }

    /// Set an attribute, keeping the others in their order; one it didn't have goes where SuperXPuTTY writes it.
    /// An empty value removes the attributes XmlSerializer leaves out when they're empty (Color, JumpHost, ...).
    pub fn set(&mut self, name: &str, value: &str) {
        let existing = self.attributes.iter().position(|(n, _)| n == name);
        if value.is_empty() && WRITTEN_ONLY_WHEN_SET.contains(&name) {
            if let Some(index) = existing {
                self.attributes.remove(index);
            }
            return;
        }
        if let Some(index) = existing {
            self.attributes[index].1 = value.to_string();
            return;
        }
        let rank = |n: &str| ATTRIBUTE_ORDER.iter().position(|o| *o == n);
        let mine = rank(name).unwrap_or(ATTRIBUTE_ORDER.len());
        let at = self
            .attributes
            .iter()
            .position(|(n, _)| rank(n).is_some_and(|r| r > mine))
            .unwrap_or(self.attributes.len());
        self.attributes.insert(at, (name.to_string(), value.to_string()));
    }
}

/// The path of a store's file: Sessions.xml in the settings folder, PrivateSessions.xml in the user's folder
pub fn file_of(store: Store, settings_folder: Option<&Path>, user_folder: &Path) -> Option<PathBuf> {
    match store {
        Store::Saved => settings_folder.map(|f| f.join("Sessions.xml")),
        Store::Private => Some(user_folder.join("PrivateSessions.xml")),
        Store::Database => None,
    }
}

/// Read a store's file fresh, change it, and write it back, so changes made meanwhile (by SuperXPuTTY on another
/// computer, say) are kept. `change` gets the sessions in file order.
pub fn update_file(path: &Path, change: impl FnOnce(&mut Vec<StoredSession>) -> Result<(), String>) -> Result<(), String> {
    let mut problems = Vec::new();
    let mut sessions = read_sessions(path, &mut problems);
    if let Some(problem) = problems.into_iter().next() {
        return Err(format!("Couldn't read it to save: {problem}"));
    }
    change(&mut sessions)?;
    back_up(path);
    let text = write_sessions(&sessions);
    let temp = path.with_extension("xml.saving");
    std::fs::write(&temp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, path).map_err(|e| e.to_string())
}

/// Put a session where SuperXPuTTY's sort would (by id, ignoring case), leaving the others in file order
pub fn insert_sorted(sessions: &mut Vec<StoredSession>, session: StoredSession) {
    let key = session.id().to_lowercase();
    let at = sessions.iter().position(|s| s.id().to_lowercase() > key).unwrap_or(sessions.len());
    sessions.insert(at, session);
}

/// The file text, as .NET's XmlSerializer writes `List<SessionData>`: CRLF, two-space indent, one element per line
pub fn write_sessions(sessions: &[StoredSession]) -> String {
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n");
    let root = "ArrayOfSessionData xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"";
    if sessions.is_empty() {
        out.push_str(&format!("<{root} />"));
        return out;
    }
    out.push_str(&format!("<{root}>\r\n"));
    for session in sessions {
        out.push_str("  <SessionData");
        for (name, value) in &session.attributes {
            out.push_str(&format!(" {name}=\"{}\"", escape(value)));
        }
        out.push_str(" />\r\n");
    }
    out.push_str("</ArrayOfSessionData>");
    out
}

/// An attribute value escaped as .NET's XmlWriter escapes it
pub(crate) fn escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for c in value.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\n' => out.push_str("&#xA;"),
            '\r' => out.push_str("&#xD;"),
            '\t' => out.push_str("&#x9;"),
            c => out.push(c),
        }
    }
    out
}

/// Keep a copy of the file before it's written, as SuperXPuTTY does: "Sessions.yyyyMMdd_hhmmss.xml" beside it (with
/// .NET's 12-hour "hh"), keeping the newest 20. A failed backup doesn't stop the save.
pub(crate) fn back_up(path: &Path) {
    let (Some(folder), Some(base)) = (path.parent(), path.file_stem().and_then(|s| s.to_str())) else { return };
    if !path.exists() {
        return;
    }
    let stamp = chrono::Local::now().format("%Y%m%d_%I%M%S");
    if std::fs::copy(path, folder.join(format!("{base}.{stamp}.xml"))).is_err() {
        return;
    }
    let Ok(entries) = std::fs::read_dir(folder) else { return };
    let prefix = format!("{base}.");
    let mut old: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.starts_with(&prefix) && n.ends_with(".xml") && n.len() == prefix.len() + 19)
        })
        .collect();
    old.sort();
    old.reverse();
    for extra in old.iter().skip(20) {
        let _ = std::fs::remove_file(extra);
    }
}
