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

    /// The folder part of the id ("" at the top), as SuperXPuTTY's `GetSessionParentId`
    pub fn folder(&self) -> &str {
        self.id().rsplit_once('/').map(|(folder, _)| folder).unwrap_or("")
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

/// What the UI shows for a session: everything but secrets
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
}

/// Every session and folder setting, as last read
#[derive(Default)]
pub struct Sessions {
    pub saved: Vec<StoredSession>,
    pub private: Vec<StoredSession>,
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
        };
        list.iter().find(|s| s.id() == id)
    }

    /// The folder settings along a folder's path, nearest first, as SuperXPuTTY's `FolderSettings.Along`
    pub fn along(&self, store: Store, folder: &str) -> Vec<&FolderSetting> {
        let mut found = Vec::new();
        let Some(settings) = self.folders.get(&store) else { return found };
        let mut current = folder.trim_matches('/');
        while !current.is_empty() {
            if let Some(setting) = settings.get(current) {
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
        saved.chain(private).collect()
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
        id,
    }
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
            folders.insert(setting.path.clone(), setting);
        }
    }
    folders
}

/// Every element of the given name in an XML file, as its attributes in order
///
/// Returns no elements when the file doesn't exist, and `Err` with a message when it can't be read or parsed.
fn read_elements(path: &Path, element: &str) -> Result<Vec<Vec<(String, String)>>, Option<String>> {
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

    /// A file as SuperXPuTTY's XmlSerializer writes it: CRLF, escaped characters, and an attribute this app doesn't know
    const SESSIONS: &str = "<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<ArrayOfSessionData xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\r\n  <SessionData SessionId=\"Lab/Core/sw &amp; rtr\" Color=\"#C62828\" SessionName=\"sw &amp; rtr\" Host=\"10.0.0.1\" Note=\"line one&#xA;line two\" Port=\"0\" Proto=\"SSH\" FutureField=\"kept\" />\r\n</ArrayOfSessionData>\r\n";

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

        assert!(problems.is_empty(), "{problems:?}");
        let session = &sessions.saved[0];
        assert_eq!(session.id(), "Lab/Core/sw & rtr");
        assert_eq!(session.folder(), "Lab/Core");
        assert_eq!(session.get("Note"), "line one\nline two");
        assert_eq!(session.attributes.last().unwrap(), &("FutureField".to_string(), "kept".to_string()));
        let along: Vec<_> = sessions.along(Store::Saved, "Lab/Core").iter().map(|f| f.username.as_str()).collect();
        assert_eq!(along, ["core", "netops"]);
    }
}
