//! Folders: their settings (Folders.xml / PrivateFolders.xml) and the changes that move sessions with them.
//!
//! Folders aren't stored on their own: a session's id ("Lab/Core/sw1") says which folder it's in, so renaming or
//! moving a folder renames every session below it, and a new folder exists only in the tree until a session goes
//! in it, as in SuperXPuTTY. A folder's own settings (colour, and username, profile, port, jump host and
//! highlighting defaults for the sessions below) are kept by path in the store's folders file, and move with it.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::sessions::{self, insert_sorted, StoredSession};

/// A folder's settings as the window edits them; empty fields aren't set
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FolderFields {
    pub path: String,
    pub color: String,
    pub username: String,
    pub putty_profile: String,
    pub port: u16,
    pub jump_host: String,
    pub highlight_rules: String,
}

/// SuperXPuTTY's FolderSetting attributes, in its order
const ORDER: [&str; 7] = ["Path", "Color", "Username", "PuttyProfile", "Port", "JumpHost", "HighlightRules"];

/// The folders file beside a sessions file: Folders.xml for Sessions.xml, PrivateFolders.xml for PrivateSessions.xml
pub fn folders_file(sessions_file: &Path) -> PathBuf {
    let name = if sessions_file.file_name().and_then(|n| n.to_str()) == Some("PrivateSessions.xml") {
        "PrivateFolders.xml"
    } else {
        "Folders.xml"
    };
    sessions_file.with_file_name(name)
}

/// The part of `path` after `folder` ("" for the folder itself, "/Core" below it), ignoring case as the tree does
/// ("test" and "Test" are one folder, as SuperXPuTTY's); None when it isn't in it
pub fn under<'a>(path: &'a str, folder: &str) -> Option<&'a str> {
    let head = path.get(..folder.len())?;
    let rest = &path[folder.len()..];
    (head.eq_ignore_ascii_case(folder) && (rest.is_empty() || rest.starts_with('/'))).then_some(rest)
}

/// Whether `path` is `folder` or inside it, ignoring case
fn within(path: &str, folder: &str) -> bool {
    under(path, folder).is_some()
}

/// `path` with its leading `from` replaced by `to`
fn moved(path: &str, from: &str, to: &str) -> String {
    let rest = &path[from.len()..];
    if to.is_empty() { rest.trim_start_matches('/').to_string() } else { format!("{to}{rest}") }
}

/// Check a folder or session name: not empty, no "/"
pub fn check_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() {
        Err("A name can't be empty.".into())
    } else if name.contains('/') {
        Err("A name can't contain \"/\": that separates folders.".into())
    } else {
        Ok(())
    }
}

/// Move (or rename) a folder: every session below gets the new path, and the folder settings go with it.
/// `others` are the session ids in the other store, which must not clash.
pub fn move_folder(sessions_file: &Path, from: &str, to: &str, others: &[String]) -> Result<(), String> {
    let (from, to) = (from.trim_matches('/'), to.trim_matches('/'));
    if from.is_empty() {
        return Err("A root can't be moved.".into());
    }
    if within(to, from) {
        return Err("A folder can't go inside itself.".into());
    }
    sessions::update_file(sessions_file, |list| {
        let (inside, rest): (Vec<StoredSession>, Vec<StoredSession>) = list.drain(..).partition(|s| within(s.folder(), from));
        *list = rest;
        for mut session in inside {
            let id = moved(session.id(), from, to);
            if list.iter().any(|s| s.id().eq_ignore_ascii_case(&id)) || others.iter().any(|o| o.eq_ignore_ascii_case(&id)) {
                return Err(format!("There's already a session called {id}."));
            }
            session.set("SessionId", &id);
            insert_sorted(list, session);
        }
        Ok(())
    })?;
    update_folders(&folders_file(sessions_file), |settings| {
        for setting in settings.iter_mut() {
            if let Some(path) = get(setting, "Path").map(|p| p.trim_matches('/').to_string()) {
                if within(&path, from) {
                    set(setting, "Path", &moved(&path, from, to));
                }
            }
        }
    })
}

/// Move a session into another folder of the same store, as `name` when given (its name is taken there)
pub fn move_session(sessions_file: &Path, id: &str, folder: &str, others: &[String], name: Option<&str>) -> Result<String, String> {
    let name = name.map(str::to_string).unwrap_or_else(|| id.rsplit('/').next().unwrap_or(id).to_string());
    check_name(&name)?;
    let folder = folder.trim_matches('/');
    let new_id = if folder.is_empty() { name.clone() } else { format!("{folder}/{name}") };
    sessions::update_file(sessions_file, |list| {
        let taken = |o: &str| o.eq_ignore_ascii_case(&new_id) && o != id;
        if list.iter().any(|s| taken(s.id())) || others.iter().any(|o| taken(o)) {
            return Err(format!("There's already a session called {new_id}."));
        }
        let index = list.iter().position(|s| s.id() == id).ok_or("The session is no longer in its file.")?;
        let mut session = list.remove(index);
        session.set("SessionId", &new_id);
        session.set("SessionName", &name);
        insert_sorted(list, session);
        Ok(())
    })?;
    Ok(new_id)
}

/// Delete a folder: its sessions, at every level, and its settings
pub fn delete_folder(sessions_file: &Path, folder: &str) -> Result<(), String> {
    let folder = folder.trim_matches('/');
    if folder.is_empty() {
        return Err("A root can't be deleted.".into());
    }
    sessions::update_file(sessions_file, |list| {
        list.retain(|s| !within(s.folder(), folder));
        Ok(())
    })?;
    update_folders(&folders_file(sessions_file), |settings| {
        settings.retain(|s| !get(s, "Path").is_some_and(|p| within(p.trim_matches('/'), folder)));
    })
}

/// Save a folder's settings; all empty, they're removed, as SuperXPuTTY's FolderSettings.Set
pub fn save_settings(sessions_file: &Path, fields: &FolderFields) -> Result<(), String> {
    let path = fields.path.trim_matches('/').to_string();
    if path.is_empty() {
        return Err("A root has no folder settings.".into());
    }
    update_folders(&folders_file(sessions_file), |settings| {
        let index = settings.iter().position(|s| get(s, "Path").is_some_and(|p| p.trim_matches('/').eq_ignore_ascii_case(&path)));
        let mut setting = index.map(|i| settings.remove(i)).unwrap_or_else(|| vec![("Path".into(), path.clone())]);
        let port = if fields.port > 0 { fields.port.to_string() } else { String::new() };
        for (name, value) in [
            ("Color", fields.color.trim()),
            ("Username", fields.username.trim()),
            ("PuttyProfile", fields.putty_profile.trim()),
            ("Port", port.as_str()),
            ("JumpHost", fields.jump_host.trim()),
            ("HighlightRules", fields.highlight_rules.trim()),
        ] {
            set(&mut setting, name, value);
        }
        if setting.iter().any(|(n, v)| n != "Path" && !v.is_empty()) {
            settings.push(setting);
        }
    })
}

/// An attribute of a folder setting
fn get<'a>(setting: &'a [(String, String)], name: &str) -> Option<&'a str> {
    setting.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
}

/// Set an attribute of a folder setting in SuperXPuTTY's order; empty removes it (Port, and strings that aren't set)
fn set(setting: &mut Vec<(String, String)>, name: &str, value: &str) {
    let existing = setting.iter().position(|(n, _)| n == name);
    if value.is_empty() && name != "Path" {
        if let Some(i) = existing {
            setting.remove(i);
        }
        return;
    }
    if let Some(i) = existing {
        setting[i].1 = value.to_string();
        return;
    }
    let rank = |n: &str| ORDER.iter().position(|o| *o == n).unwrap_or(ORDER.len());
    let at = setting.iter().position(|(n, _)| rank(n) > rank(name)).unwrap_or(setting.len());
    setting.insert(at, (name.to_string(), value.to_string()));
}

/// Read the folders file, change it, and write it back sorted by path (ignoring case), as SuperXPuTTY saves it.
/// A file that doesn't exist and would stay empty isn't created.
fn update_folders(path: &Path, change: impl FnOnce(&mut Vec<Vec<(String, String)>>)) -> Result<(), String> {
    let mut settings = sessions::read_elements(path, "FolderSetting").map_err(|e| e.unwrap_or_default())?;
    change(&mut settings);
    if settings.is_empty() && !path.exists() {
        return Ok(());
    }
    settings.sort_by_key(|s| get(s, "Path").unwrap_or("").to_lowercase());
    let root = "Folders xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\"";
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n");
    if settings.is_empty() {
        out.push_str(&format!("<{root} />"));
    } else {
        out.push_str(&format!("<{root}>\r\n"));
        for setting in &settings {
            out.push_str("  <FolderSetting");
            for (name, value) in setting {
                out.push_str(&format!(" {name}=\"{}\"", sessions::escape(value)));
            }
            out.push_str(" />\r\n");
        }
        out.push_str("</Folders>");
    }
    sessions::back_up(path);
    let temp = path.with_extension("xml.saving");
    std::fs::write(&temp, out).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, path).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editing::{save, SessionFields};

    #[test]
    fn renaming_a_folder_moves_its_sessions_and_settings() {
        let dir = std::env::temp_dir().join(format!("sxt-folders-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Sessions.xml");
        for (name, folder) in [("sw1", "Lab/Core"), ("sw2", "Lab"), ("rtr", "Other")] {
            let fields = SessionFields { name: name.into(), folder: folder.into(), host: "h".into(), ..Default::default() };
            save(&file, None, &fields, &[]).unwrap();
        }
        save_settings(&file, &FolderFields { path: "Lab/Core".into(), color: "#C62828".into(), ..Default::default() }).unwrap();

        move_folder(&file, "Lab", "Site A/Lab", &[]).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(text.contains("SessionId=\"Site A/Lab/Core/sw1\"") && text.contains("SessionId=\"Site A/Lab/sw2\""), "{text}");
        assert!(text.contains("SessionId=\"Other/rtr\""));
        let folders = std::fs::read_to_string(dir.join("Folders.xml")).unwrap();
        assert!(folders.contains("<FolderSetting Path=\"Site A/Lab/Core\" Color=\"#C62828\" />"), "{folders}");

        assert!(move_folder(&file, "Site A", "Site A/Lab/x", &[]).is_err());
        assert_eq!(move_session(&file, "Other/rtr", "Site A", &[], None).unwrap(), "Site A/rtr");
        delete_folder(&file, "Site A").unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        assert!(!text.contains("SessionId="), "{text}");
        assert!(!std::fs::read_to_string(dir.join("Folders.xml")).unwrap().contains("FolderSetting "));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn folders_differing_in_case_are_one_and_a_moved_session_can_be_renamed() {
        let dir = std::env::temp_dir().join(format!("sxt-folders-case-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Sessions.xml");
        for (name, folder) in [("a", "test"), ("b", "Test"), ("c", "TEST/sub"), ("a", "Lab")] {
            let fields = SessionFields { name: name.into(), folder: folder.into(), host: "h".into(), ..Default::default() };
            save(&file, None, &fields, &[]).unwrap();
        }
        // renaming the folder the tree shows as one takes every spelling
        move_folder(&file, "Test", "Tests", &[]).unwrap();
        let text = std::fs::read_to_string(&file).unwrap();
        for id in ["Tests/a", "Tests/b", "Tests/sub/c"] {
            assert!(text.contains(&format!("SessionId=\"{id}\"")), "{text}");
        }
        // "a" is taken in Tests: moved as "a-1"
        assert!(move_session(&file, "Lab/a", "Tests", &[], None).is_err());
        assert_eq!(move_session(&file, "Lab/a", "Tests", &[], Some("a-1")).unwrap(), "Tests/a-1");
        assert!(std::fs::read_to_string(&file).unwrap().contains("SessionId=\"Tests/a-1\" SessionName=\"a-1\""));
        assert_eq!(under("Tests/sub/c", "tests"), Some("/sub/c"));
        assert_eq!(under("Testsx", "tests"), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
