//! Saved sessions while the settings folder can't be reached: a copy kept on this computer.
//!
//! The settings folder is usually a shared or network drive, and a laptop off the network (or a VPN that dropped)
//! loses it. Without this, Sessions.xml then reads as "no file", and Saved sessions left the tree a few seconds
//! later, though they're exactly what's needed to get back on. So every good read of Sessions.xml and Folders.xml is
//! kept in the user's own folder (`SavedSessions.cache.xml`, `SavedFolders.cache.xml`, and whose and when in
//! `SavedSessions.cache.json`), and shown, as "Saved sessions (offline, cached 09:41)", while the folder is away;
//! the tree's check of the files reads the real ones again once it's back. It's the files' counterpart of Database
//! sessions' `DatabaseSessions.cache.xml` ([`crate::db::read_cache`]).
//!
//! Offline, Saved sessions open as usual but can't be changed (`store_file` in lib.rs refuses): a change has nowhere
//! to go, and a save that found "no file" and then could write would replace the team's sessions with the one saved.
//! The copy holds what Sessions.xml holds, a `-pw` in a session's extra arguments included, in the folder
//! PrivateSessions.xml is in.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::sessions::{self, FolderSetting, Sessions, Store, StoredSession};

/// The copy of Sessions.xml, in the user's folder
const SESSIONS_FILE: &str = "SavedSessions.cache.xml";
/// The copy of Folders.xml, in the user's folder
const FOLDERS_FILE: &str = "SavedFolders.cache.xml";
/// Whose copy it is and when it was read ([`About`]), in the user's folder
const ABOUT_FILE: &str = "SavedSessions.cache.json";
/// How [`About::read_at`] is written: the date too, as a copy can be days old
const READ_AT: &str = "%Y-%m-%d %H:%M";

/// What the copy is of
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct About {
    /// The settings folder it was read from: another folder's sessions aren't shown as this one's
    source: String,
    /// When it was last read there, as [`READ_AT`]
    read_at: String,
}

/// The session files as read for the tree, by [`load`]
pub struct Loaded {
    /// Saved sessions (the copy's, when [`Loaded::saved_offline`]) and Private sessions, with their folders' settings
    pub sessions: Sessions,
    /// The settings folder can't be reached
    pub saved_offline: bool,
    /// When the copy shown was read, "09:41" today or "2026-11-10 09:41" before; "" when not offline, or there's no
    /// copy of this folder's sessions to show
    pub saved_cached_at: String,
}

/// Whether the settings folder can be reached. A folder that's there with no Sessions.xml in it is a team with no
/// sessions yet; one that isn't there at all is a drive that's away.
pub fn reachable(settings_folder: &Path) -> bool {
    std::fs::metadata(settings_folder).is_ok_and(|m| m.is_dir())
}

/// [`load_with`] reading the files straight from the disk, for the tests
#[cfg(test)]
pub fn load(settings_folder: Option<&Path>, user_folder: &Path, problems: &mut Vec<String>) -> Loaded {
    load_with(settings_folder, user_folder, problems, &sessions::read_text)
}

/// Read the session files, as [`Sessions::load_with`], keeping a copy of the Saved ones; when the settings folder
/// can't be reached, its Saved sessions and folder settings are the copy's. A Sessions.xml or Folders.xml that's
/// there but doesn't read (it's in `problems`) leaves the copy as it was, so the last good one is kept. The session
/// files' text is got with `read_text`, which the tree's reading uses to keep each file's hash
/// ([`crate::file_watch::Reading`]); the copy itself, when it's what's shown, is read from this computer as ever.
pub fn load_with(settings_folder: Option<&Path>, user_folder: &Path, problems: &mut Vec<String>, read_text: sessions::ReadText) -> Loaded {
    let Some(folder) = settings_folder else {
        return Loaded { sessions: Sessions::load_with(None, user_folder, problems, read_text), saved_offline: false, saved_cached_at: String::new() };
    };
    if reachable(folder) {
        let sessions = Sessions::load_with(Some(folder), user_folder, problems, read_text);
        // the files' problems start with their paths
        let within = folder.display().to_string();
        if !problems.iter().any(|p| p.starts_with(&within)) {
            let folders: Vec<&FolderSetting> = sessions.folders.get(&Store::Saved).map(|f| f.values().collect()).unwrap_or_default();
            if let Err(e) = keep(user_folder, folder, &sessions.saved, &folders) {
                crate::applog::warn(format!("Couldn't keep a copy of Saved sessions for when the settings folder is away: {e}"));
            }
        }
        return Loaded { sessions, saved_offline: false, saved_cached_at: String::new() };
    }
    let mut sessions = Sessions::load_with(None, user_folder, problems, read_text);
    let mut saved_cached_at = String::new();
    if let Some(kept) = read(user_folder, folder) {
        sessions.saved = kept.sessions;
        sessions.folders.insert(Store::Saved, kept.folders);
        saved_cached_at = shown_time(&kept.read_at, &chrono::Local::now().format(READ_AT).to_string());
    }
    Loaded { sessions, saved_offline: true, saved_cached_at }
}

/// The copy as read back
struct Kept {
    sessions: Vec<StoredSession>,
    /// By folder path in lower case, as [`Sessions::folders`]
    folders: HashMap<String, FolderSetting>,
    /// As [`READ_AT`]
    read_at: String,
}

/// Keep a copy of the Saved sessions and folder settings just read from `settings_folder`. The two files are only
/// written when they'd differ (a read follows every change and every check that finds one, and the sessions seldom
/// differ); when it was read is written each time.
fn keep(user_folder: &Path, settings_folder: &Path, saved: &[StoredSession], folders: &[&FolderSetting]) -> Result<(), String> {
    std::fs::create_dir_all(user_folder).map_err(|e| e.to_string())?;
    for (name, text) in [(SESSIONS_FILE, sessions::write_sessions(saved)), (FOLDERS_FILE, folders_text(folders))] {
        let path = user_folder.join(name);
        if std::fs::read_to_string(&path).is_ok_and(|now| now == text) {
            continue;
        }
        crate::saving::save(&path, "saving", text)?;
    }
    let about = About { source: settings_folder.display().to_string(), read_at: chrono::Local::now().format(READ_AT).to_string() };
    crate::saving::save(&user_folder.join(ABOUT_FILE), "saving", serde_json::to_string(&about).map_err(|e| e.to_string())?)
}

/// The copy of `settings_folder`'s Saved sessions; None when there's none, it's another folder's, or it doesn't read
fn read(user_folder: &Path, settings_folder: &Path) -> Option<Kept> {
    let about: About = serde_json::from_str(&std::fs::read_to_string(user_folder.join(ABOUT_FILE)).ok()?).ok()?;
    if about.source != settings_folder.display().to_string() {
        return None;
    }
    let mut problems = Vec::new();
    let sessions = sessions::read_sessions_file(&user_folder.join(SESSIONS_FILE), &mut problems);
    let folders = sessions::read_folder_settings(&user_folder.join(FOLDERS_FILE), &mut problems);
    if !problems.is_empty() {
        return None;
    }
    Some(Kept { sessions, folders: folders.into_iter().map(|f| (f.path.to_lowercase(), f)).collect(), read_at: about.read_at })
}

/// Folder settings as a Folders.xml, sorted by path so the same settings always give the same text (they're kept in a
/// map, whose order changes from read to read)
fn folders_text(folders: &[&FolderSetting]) -> String {
    let mut sorted = folders.to_vec();
    sorted.sort_by_key(|f| f.path.to_lowercase());
    let mut out = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\r\n<Folders>\r\n");
    for folder in sorted {
        let port = if folder.port > 0 { folder.port.to_string() } else { String::new() };
        out.push_str("  <FolderSetting");
        for (name, value) in [
            ("Path", folder.path.as_str()),
            ("Color", folder.color.as_str()),
            ("Username", folder.username.as_str()),
            ("PuttyProfile", folder.putty_profile.as_str()),
            ("Port", port.as_str()),
            ("JumpHost", folder.jump_host.as_str()),
            ("HighlightRules", folder.highlight_rules.as_str()),
        ] {
            if !value.is_empty() {
                out.push_str(&format!(" {name}=\"{}\"", sessions::escape(value)));
            }
        }
        out.push_str(" />\r\n");
    }
    out.push_str("</Folders>");
    out
}

/// When a copy was read, for the root's label: the time alone when that was today ("09:41", as Database sessions'
/// "offline, cached 09:41"), with its date when it wasn't. Both as [`READ_AT`].
fn shown_time(read_at: &str, now: &str) -> String {
    match (read_at.split_once(' '), now.split_once(' ')) {
        (Some((day, time)), Some((today, _))) if day == today => time.to_string(),
        _ => read_at.to_string(),
    }
}

/// The copy's files in `user_folder`, for a test to look at
#[cfg(test)]
fn files(user_folder: &Path) -> [std::path::PathBuf; 3] {
    [SESSIONS_FILE, FOLDERS_FILE, ABOUT_FILE].map(|name| user_folder.join(name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    /// A settings folder and a user's folder of their own for one test, both there and empty
    fn folders(name: &str) -> (PathBuf, PathBuf) {
        let base = std::env::temp_dir().join(format!("sxt-saved-cache-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (team, user) = (base.join("team"), base.join("user"));
        std::fs::create_dir_all(&team).unwrap();
        std::fs::create_dir_all(&user).unwrap();
        (team, user)
    }

    /// A Sessions.xml with these ids
    fn sessions_xml(ids: &[&str]) -> String {
        let list: Vec<StoredSession> =
            ids.iter().map(|id| StoredSession { attributes: vec![("SessionId".into(), id.to_string()), ("Host".into(), "h & co".into())] }).collect();
        sessions::write_sessions(&list)
    }

    /// The ids of the Saved sessions read
    fn saved_ids(loaded: &Loaded) -> Vec<&str> {
        loaded.sessions.saved.iter().map(|s| s.id()).collect()
    }

    #[test]
    fn saved_sessions_stay_from_the_copy_kept_while_the_settings_folder_is_away() {
        let (team, user) = folders("away");
        std::fs::write(team.join("Sessions.xml"), sessions_xml(&["Lab/sw1", "Lab/sw2"])).unwrap();
        std::fs::write(team.join("Folders.xml"), "<Folders>\r\n  <FolderSetting Path=\"Lab\" Color=\"#C62828\" Port=\"2222\" />\r\n</Folders>").unwrap();
        std::fs::write(user.join("PrivateSessions.xml"), sessions_xml(&["Home/nas"])).unwrap();

        // the folder there: read from it, not offline
        let mut problems = Vec::new();
        let there = load(Some(&team), &user, &mut problems);
        assert_eq!((saved_ids(&there), there.saved_offline, there.saved_cached_at.as_str()), (vec!["Lab/sw1", "Lab/sw2"], false, ""));

        // the drive goes: the same sessions and folder settings, from the copy, and Private sessions as ever
        let parked = team.with_file_name("team-away");
        std::fs::rename(&team, &parked).unwrap();
        let away = load(Some(&team), &user, &mut problems);
        assert!(away.saved_offline);
        assert_eq!(saved_ids(&away), ["Lab/sw1", "Lab/sw2"]);
        assert_eq!(away.sessions.saved[0].get("Host"), "h & co");
        let lab = &away.sessions.folders[&Store::Saved]["lab"];
        assert_eq!((lab.path.as_str(), lab.color.as_str(), lab.port), ("Lab", "#C62828", 2222));
        assert_eq!(away.sessions.private.len(), 1);
        // read today: the time alone, as Database sessions' "offline, cached 09:41"
        assert_eq!(away.saved_cached_at.len(), 5, "{}", away.saved_cached_at);
        assert_eq!(problems, Vec::<String>::new());

        // and back, changed meanwhile by a teammate: the folder's own again
        std::fs::rename(&parked, &team).unwrap();
        std::fs::write(team.join("Sessions.xml"), sessions_xml(&["Lab/sw1"])).unwrap();
        let back = load(Some(&team), &user, &mut problems);
        assert_eq!((saved_ids(&back), back.saved_offline), (vec!["Lab/sw1"], false));
        std::fs::remove_dir_all(team.parent().unwrap()).unwrap();
    }

    #[test]
    fn another_folders_copy_isnt_shown_and_a_file_that_doesnt_read_leaves_the_copy() {
        let (team, user) = folders("other");
        std::fs::write(team.join("Sessions.xml"), sessions_xml(&["Lab/sw1"])).unwrap();
        let mut problems = Vec::new();
        load(Some(&team), &user, &mut problems);

        // a settings folder chosen since, and away: offline, with nothing of the first folder's
        let other = load(Some(&team.with_file_name("elsewhere")), &user, &mut problems);
        assert!(other.saved_offline && other.sessions.saved.is_empty() && other.saved_cached_at.is_empty());

        // Sessions.xml broken by an editor: told as a problem, and the copy still holds the last good read
        std::fs::write(team.join("Sessions.xml"), "<ArrayOfSessionData><SessionData SessionId=\"Lab/sw1").unwrap();
        let broken = load(Some(&team), &user, &mut problems);
        assert!(!broken.saved_offline && broken.sessions.saved.is_empty());
        assert_eq!(problems.len(), 1, "{problems:?}");
        std::fs::remove_dir_all(&team).unwrap();
        assert_eq!(saved_ids(&load(Some(&team), &user, &mut Vec::new())), ["Lab/sw1"]);
        std::fs::remove_dir_all(team.parent().unwrap()).unwrap();
    }

    #[test]
    fn each_session_file_is_read_once_for_the_tree() {
        let (team, user) = folders("once");
        std::fs::write(team.join("Sessions.xml"), sessions_xml(&["Lab/sw1"])).unwrap();
        std::fs::write(user.join("PrivateSessions.xml"), sessions_xml(&["Home/nas"])).unwrap();
        let read = std::cell::RefCell::new(Vec::new());
        let noting = |path: &Path| {
            read.borrow_mut().push(path.file_name().unwrap().to_string_lossy().into_owned());
            sessions::read_text(path)
        };
        let loaded = load_with(Some(&team), &user, &mut Vec::new(), &noting);
        assert_eq!((saved_ids(&loaded), loaded.sessions.private.len()), (vec!["Lab/sw1"], 1));
        let mut names = read.into_inner();
        names.sort();
        assert_eq!(names, ["Folders.xml", "PrivateFolders.xml", "PrivateSessions.xml", "Sessions.xml"]);
        std::fs::remove_dir_all(team.parent().unwrap()).unwrap();
    }

    #[test]
    fn the_copy_is_written_again_only_when_the_sessions_differ() {
        let (team, user) = folders("unchanged");
        std::fs::write(team.join("Sessions.xml"), sessions_xml(&["Lab/sw1"])).unwrap();
        load(Some(&team), &user, &mut Vec::new());
        // read again over a second later (file times can be that coarse): the two files are as they were
        let [sessions_copy, folders_copy, _] = files(&user);
        let before = (std::fs::metadata(&sessions_copy).unwrap().modified().unwrap(), std::fs::metadata(&folders_copy).unwrap().modified().unwrap());
        std::thread::sleep(std::time::Duration::from_millis(1100));
        load(Some(&team), &user, &mut Vec::new());
        let after = (std::fs::metadata(&sessions_copy).unwrap().modified().unwrap(), std::fs::metadata(&folders_copy).unwrap().modified().unwrap());
        assert_eq!(before, after);
        std::fs::remove_dir_all(team.parent().unwrap()).unwrap();
    }

    #[test]
    fn a_copy_read_today_shows_its_time_and_an_older_one_its_date_too() {
        assert_eq!(shown_time("2026-11-11 09:41", "2026-11-11 17:05"), "09:41");
        assert_eq!(shown_time("2026-11-10 09:41", "2026-11-11 17:05"), "2026-11-10 09:41");
        assert!(reachable(&std::env::temp_dir()));
        assert!(!reachable(&std::env::temp_dir().join("sxt-saved-cache-not-there")));
    }
}
