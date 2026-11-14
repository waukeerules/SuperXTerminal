//! Folders: their settings (Folders.xml / PrivateFolders.xml) and the changes that move sessions with them.
//!
//! Folders aren't stored on their own: a session's id ("Lab/Core/sw1") says which folder it's in, so renaming or
//! moving a folder renames every session below it, and a new folder exists only in the tree until a session goes
//! in it, as in SuperXPuTTY. A folder's own settings (colour, and username, profile, port, jump host and
//! highlighting defaults for the sessions below) are kept by path in the store's folders file, and move with it.

use std::collections::{HashMap, HashSet};
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
            if let Some(path) = get(setting, "Path").map(|p| p.trim_matches('/').to_string()) && within(&path, from) {
                    set(setting, "Path", &moved(&path, from, to));
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

/// What [`move_many`] did
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Moved {
    /// The folders that moved, as they were asked for
    pub folders: Vec<String>,
    /// How many sessions moved on their own (not with a folder)
    pub sessions: usize,
    /// What didn't move and why ("Lab: There's already a session called Site A/Lab/sw1."), for the tree to show
    pub problems: Vec<String>,
    /// Every session's id before and after, those in the folders too, for Favorites, Recent, the local overrides and
    /// the sessions' own passwords to follow. Not for the window.
    #[serde(skip)]
    pub renamed: Vec<(String, String)>,
}

/// `name` in `folder` as an id ("Lab/sw1"; just the name at the top)
fn join(folder: &str, name: &str) -> String {
    if folder.is_empty() { name.to_string() } else { format!("{folder}/{name}") }
}

/// The folder part of a session id in a file ("" at the top) and its name
fn split(id: &str) -> (&str, &str) {
    id.rsplit_once('/').unwrap_or(("", id))
}

/// Move a selection into the folder `to` ("" for the top): each of `folders` with all below it, as [`move_folder`]
/// to `to/<its name>`, and each of `sessions` (ids) as [`move_session`], one whose name is taken there getting a free
/// one ("sw1-1"), as SuperXPuTTY's UniqueName. A folder isn't moved into itself, and what's already in `to` is left.
///
/// All of it is worked out on one read of the file and saved with one backup and one write, and Folders.xml the
/// same. One [`move_folder`] or [`move_session`] for each item reads, backs up and writes the whole file each time:
/// with thousands of sessions that's slow, and on a network drive, where each is a round trip, a selection of a few
/// dozen folders took minutes. It also filled the 20 backups kept with the steps of one move.
///
/// An item that can't move (its name is taken) is told in [`Moved::problems`] and the rest still move. `others` are
/// the session ids in the other store, which must not clash. Nothing is written when nothing moved.
pub fn move_many(sessions_file: &Path, folders: &[String], sessions: &[String], to: &str, others: &[String]) -> Result<Moved, String> {
    let to = to.trim_matches('/');
    let mut done = Moved::default();
    // the folders that moved, from and to, for their settings to follow
    let mut folder_moves: Vec<(String, String)> = Vec::new();
    // update_file writes unless its change fails; this says the "failure" was only that there's nothing to write
    let mut nothing = false;
    let saved = sessions::update_file(sessions_file, |list| {
        let was: Vec<String> = list.iter().map(|s| s.id().to_string()).collect();
        // each session's id as the moves so far leave it, by its place in the file; where each id is; and the ids
        // taken (lowercase: ids differing in case are one), here and in the other store. Looked up, not searched for,
        // as every move of thousands would otherwise go through thousands
        let mut ids = was.clone();
        let mut at: HashMap<String, usize> = ids.iter().enumerate().map(|(i, id)| (id.clone(), i)).collect();
        let mut taken: HashSet<String> = ids.iter().chain(others).map(|id| id.to_ascii_lowercase()).collect();
        // the sessions moved on their own, whose SessionName is written again (it may have changed)
        let mut named = vec![false; ids.len()];

        for asked in folders {
            let from = asked.trim_matches('/');
            if from.is_empty() {
                done.problems.push("A root can't be moved.".into());
                continue;
            }
            let dest = join(to, split(from).1);
            if within(to, from) || dest.eq_ignore_ascii_case(from) {
                continue;
            }
            let inside: Vec<usize> = (0..ids.len()).filter(|&i| within(split(&ids[i]).0, from)).collect();
            let new: Vec<String> = inside.iter().map(|&i| moved(&ids[i], from, &dest)).collect();
            // the folder's sessions leave their ids, then take the new ones; one that's taken, and the folder stays
            for &i in &inside {
                taken.remove(&ids[i].to_ascii_lowercase());
            }
            let mut placed: Vec<String> = Vec::with_capacity(new.len());
            let mut clash = None;
            for id in &new {
                let key = id.to_ascii_lowercase();
                if !taken.insert(key.clone()) {
                    clash = Some(id);
                    break;
                }
                placed.push(key);
            }
            if let Some(id) = clash {
                for key in placed {
                    taken.remove(&key);
                }
                for &i in &inside {
                    taken.insert(ids[i].to_ascii_lowercase());
                }
                done.problems.push(format!("{from}: There's already a session called {id}."));
                continue;
            }
            for (&i, id) in inside.iter().zip(new) {
                at.remove(&ids[i]);
                at.insert(id.clone(), i);
                ids[i] = id;
            }
            folder_moves.push((from.to_string(), dest));
            done.folders.push(asked.clone());
        }

        for id in sessions {
            let Some(&i) = at.get(id.as_str()) else {
                done.problems.push(format!("{}: The session is no longer in its file.", split(id).1));
                continue;
            };
            let (folder, name) = split(&ids[i]);
            if folder.eq_ignore_ascii_case(to) {
                continue;
            }
            let mut new = join(to, name);
            let mut n = 1;
            while taken.contains(&new.to_ascii_lowercase()) {
                new = join(to, &format!("{name}-{n}"));
                n += 1;
            }
            taken.remove(&ids[i].to_ascii_lowercase());
            taken.insert(new.to_ascii_lowercase());
            at.remove(&ids[i]);
            at.insert(new.clone(), i);
            ids[i] = new;
            named[i] = true;
            done.sessions += 1;
        }

        if ids == was {
            nothing = true;
            return Err(String::new());
        }
        // those that moved are taken out and put back where SuperXPuTTY's sort has them, the others left in file order
        let mut going = Vec::new();
        let mut staying = Vec::with_capacity(list.len());
        for (i, mut session) in list.drain(..).enumerate() {
            if ids[i] == was[i] {
                staying.push(session);
                continue;
            }
            session.set("SessionId", &ids[i]);
            if named[i] {
                session.set("SessionName", split(&ids[i]).1);
            }
            done.renamed.push((was[i].clone(), ids[i].clone()));
            going.push(session);
        }
        *list = staying;
        sessions::insert_all_sorted(list, going);
        Ok(())
    });
    if !nothing {
        saved?;
    }
    if !folder_moves.is_empty() {
        update_folders(&folders_file(sessions_file), |settings| {
            for setting in settings.iter_mut() {
                let Some(mut path) = get(setting, "Path").map(|p| p.trim_matches('/').to_string()) else { continue };
                for (from, dest) in &folder_moves {
                    if within(&path, from) {
                        path = moved(&path, from, dest);
                    }
                }
                set(setting, "Path", &path);
            }
        })?;
    }
    Ok(done)
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

/// What [`delete_many`] did
#[derive(Debug, Default)]
pub struct Deleted {
    /// The ids of the sessions deleted, those in the folders too, for Favorites, Recent, the local overrides and the
    /// sessions' own passwords to forget
    pub gone: Vec<String>,
    /// What wasn't deleted and why ("sw1: The session is no longer in its file."), for the tree to show
    pub problems: Vec<String>,
}

/// Delete a selection: each of `folders` with its sessions at every level and its settings, as [`delete_folder`],
/// and each of `sessions` (ids), as `editing::delete`. As [`move_many`], it's one read, one backup and one write of
/// the file however many there are, where one call for each wrote the whole file over each time. A session that's no
/// longer there is told in [`Deleted::problems`] and the rest still go. Nothing is written when nothing was deleted.
pub fn delete_many(sessions_file: &Path, folders: &[String], sessions: &[String]) -> Result<Deleted, String> {
    let mut done = Deleted::default();
    let mut paths: Vec<&str> = Vec::with_capacity(folders.len());
    for folder in folders {
        match folder.trim_matches('/') {
            "" => done.problems.push("A root can't be deleted.".into()),
            path => paths.push(path),
        }
    }
    let ids: HashSet<&str> = sessions.iter().map(String::as_str).collect();
    // update_file writes unless its change fails; this says the "failure" was only that there's nothing to write
    let mut nothing = false;
    let saved = sessions::update_file(sessions_file, |list| {
        let mut found: HashSet<&str> = HashSet::new();
        list.retain(|s| {
            let asked = ids.get(s.id());
            if asked.is_none() && !paths.iter().any(|f| within(s.folder(), f)) {
                return true;
            }
            found.extend(asked);
            done.gone.push(s.id().to_string());
            false
        });
        for id in sessions.iter().filter(|id| !found.contains(id.as_str())) {
            done.problems.push(format!("{}: The session is no longer in its file.", split(id).1));
        }
        if done.gone.is_empty() {
            nothing = true;
            return Err(String::new());
        }
        Ok(())
    });
    if !nothing {
        saved?;
    }
    if !paths.is_empty() {
        update_folders(&folders_file(sessions_file), |settings| {
            settings.retain(|s| !get(s, "Path").is_some_and(|p| paths.iter().any(|f| within(p.trim_matches('/'), f))));
        })?;
    }
    Ok(done)
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
/// A file that doesn't exist and would stay empty isn't created, and one the change leaves as it was isn't written
/// again (most folders have no settings, and moving one would otherwise back up and write the file for nothing: a
/// round trip each on a network drive).
fn update_folders(path: &Path, change: impl FnOnce(&mut Vec<Vec<(String, String)>>)) -> Result<(), String> {
    let mut settings = sessions::read_elements(path, "FolderSetting").map_err(|e| e.unwrap_or_default())?;
    let before = settings.clone();
    change(&mut settings);
    if settings == before || (settings.is_empty() && !path.exists()) {
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
    crate::saving::save(path, "xml.saving", out)
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

    /// The backups of Sessions.xml in `dir` ("Sessions.<stamp>.xml"), one for each time it was written over
    fn backups(dir: &Path) -> usize {
        std::fs::read_dir(dir).unwrap().flatten().filter(|e| e.file_name().to_string_lossy().starts_with("Sessions.2")).count()
    }

    #[test]
    fn a_selection_of_folders_and_sessions_moves_in_one_write() {
        let dir = std::env::temp_dir().join(format!("sxt-folders-many-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Sessions.xml");
        // written as a file, not saved one by one: each save would leave a backup of its own
        let sessions: Vec<StoredSession> = ["Imported/Core/sw1", "Imported/Core/sw2", "Imported/Edge/r1", "Imported/Lab/x", "Imported/r1", "Imported/solo", "Lab/x", "r1"]
            .iter()
            .map(|id| StoredSession { attributes: vec![("SessionId".into(), id.to_string()), ("SessionName".into(), split(id).1.into()), ("Host".into(), "h".into())] })
            .collect();
        std::fs::write(&file, sessions::write_sessions(&sessions)).unwrap();
        std::fs::write(dir.join("Folders.xml"), "<Folders>\r\n  <FolderSetting Path=\"Imported/Core\" Color=\"#C62828\" />\r\n</Folders>").unwrap();

        // Imported's folders and sessions, selected together, to the top
        let folders = ["Imported/Core", "Imported/Edge", "Imported/Lab"].map(String::from);
        let ids = ["Imported/r1", "Imported/solo", "Imported/gone"].map(String::from);
        let done = move_many(&file, &folders, &ids, "", &["solo".to_string()]).unwrap();

        // Lab/x is taken, so Imported/Lab stays, and says so; the session that's gone too. The rest moved
        assert_eq!(done.folders, ["Imported/Core", "Imported/Edge"]);
        assert_eq!(done.sessions, 2);
        assert_eq!(done.problems, ["Imported/Lab: There's already a session called Lab/x.", "gone: The session is no longer in its file."]);
        let mut problems = Vec::new();
        let now: Vec<String> = sessions::read_sessions_file(&file, &mut problems).iter().map(|s| s.id().to_string()).collect();
        // "r1" is taken at the top, and "solo" in the other store: each gets a free name
        assert_eq!(now, ["Core/sw1", "Core/sw2", "Edge/r1", "Imported/Lab/x", "Lab/x", "r1", "r1-1", "solo-1"]);
        assert!(std::fs::read_to_string(&file).unwrap().contains("SessionId=\"r1-1\" SessionName=\"r1-1\""));
        let mut renamed = done.renamed.clone();
        renamed.sort();
        let pair = |a: &str, b: &str| (a.to_string(), b.to_string());
        assert_eq!(renamed, [pair("Imported/Core/sw1", "Core/sw1"), pair("Imported/Core/sw2", "Core/sw2"), pair("Imported/Edge/r1", "Edge/r1"), pair("Imported/r1", "r1-1"), pair("Imported/solo", "solo-1")]);
        assert!(std::fs::read_to_string(dir.join("Folders.xml")).unwrap().contains("<FolderSetting Path=\"Core\" Color=\"#C62828\" />"));
        // one backup for the whole move
        assert_eq!(backups(&dir), 1);

        // nothing to move: nothing written, so no backup either
        let done = move_many(&file, &["Core".to_string()], &["r1".to_string()], "", &[]).unwrap();
        assert!(done.folders.is_empty() && done.sessions == 0 && done.problems.is_empty(), "{done:?}");
        assert_eq!(backups(&dir), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_selection_of_folders_and_sessions_is_deleted_in_one_write() {
        let dir = std::env::temp_dir().join(format!("sxt-folders-delete-many-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Sessions.xml");
        // written as a file, not saved one by one: each save would leave a backup of its own
        let sessions: Vec<StoredSession> = ["Imported/Core/sw1", "Imported/core/Deep/sw2", "Imported/Edge/r1", "Imported/r1", "Lab/x", "r1"]
            .iter()
            .map(|id| StoredSession { attributes: vec![("SessionId".into(), id.to_string()), ("SessionName".into(), split(id).1.into()), ("Host".into(), "h".into())] })
            .collect();
        std::fs::write(&file, sessions::write_sessions(&sessions)).unwrap();
        let settings = "<Folders>\r\n  <FolderSetting Path=\"Imported/Core/Deep\" Color=\"#C62828\" />\r\n  <FolderSetting Path=\"Lab\" Color=\"#1565C0\" />\r\n</Folders>";
        std::fs::write(dir.join("Folders.xml"), settings).unwrap();

        let folders = ["Imported/Core", "Imported/Edge"].map(String::from);
        let done = delete_many(&file, &folders, &["Imported/r1", "Lab/gone", "r1"].map(String::from)).unwrap();

        // the folders' sessions at every level and in any spelling of the folder, and the sessions asked for
        assert_eq!(done.gone, ["Imported/Core/sw1", "Imported/core/Deep/sw2", "Imported/Edge/r1", "Imported/r1", "r1"]);
        assert_eq!(done.problems, ["gone: The session is no longer in its file."]);
        let mut problems = Vec::new();
        let left: Vec<String> = sessions::read_sessions_file(&file, &mut problems).iter().map(|s| s.id().to_string()).collect();
        assert_eq!(left, ["Lab/x"]);
        let settings = std::fs::read_to_string(dir.join("Folders.xml")).unwrap();
        assert!(!settings.contains("Imported") && settings.contains("<FolderSetting Path=\"Lab\" Color=\"#1565C0\" />"), "{settings}");
        // one backup for the whole of it
        assert_eq!(backups(&dir), 1);

        // nothing there to delete: nothing written, so no backup either
        let done = delete_many(&file, &["Imported".to_string()], &["r1".to_string()]).unwrap();
        assert!(done.gone.is_empty());
        assert_eq!(done.problems, ["r1: The session is no longer in its file."]);
        assert_eq!(backups(&dir), 1);
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
