//! A session's own password, kept in the Keychain (Credential Manager on Windows) under "session:<id>", never in
//! Sessions.xml or the Database table: Edit Session's Password. It's typed at the first password prompt as a
//! profile's is, and comes before the session's -pw and its profile's (launch.rs). It's this computer's only:
//! SuperXPuTTY and other computers reading the same sessions still use the -pw, if the session has one.
//!
//! The Keychain can't list what it holds, so SessionPasswords.json in the user's own folder lists the sessions that
//! have one (their ids, no passwords), for Edit Session to say so without reading the Keychain, and for a renamed,
//! moved or deleted session or folder to take its passwords along or drop them, as local overrides do.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::secrets;

/// The list of sessions with a password of their own, in the user's folder
fn file(user_folder: &Path) -> PathBuf {
    user_folder.join("SessionPasswords.json")
}

/// The Keychain account for a session's password
fn account(id: &str) -> String {
    format!("session:{id}")
}

/// The ids of the sessions with a password of their own
fn ids(user_folder: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(file(user_folder)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Write the list, making the user folder if it isn't there
fn save_ids(user_folder: &Path, ids: &BTreeSet<String>) -> Result<(), String> {
    std::fs::create_dir_all(user_folder).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(ids).map_err(|e| e.to_string())?;
    std::fs::write(file(user_folder), text).map_err(|e| e.to_string())
}

/// A session's own password, if it has one
pub fn get(id: &str) -> Option<String> {
    secrets::get(&account(id))
}

/// Whether a session has a password of its own, from the list (the Keychain isn't read)
pub fn has(user_folder: &Path, id: &str) -> bool {
    ids(user_folder).contains(id)
}

/// Store a session's own password, or forget it when `password` is empty
pub fn set(user_folder: &Path, id: &str, password: &str) -> Result<(), String> {
    secrets::set(&account(id), password)?;
    let mut all = ids(user_folder);
    let listed = if password.is_empty() { all.remove(id) } else { all.insert(id.to_string()) };
    if listed {
        save_ids(user_folder, &all)?;
    }
    Ok(())
}

/// Store one password for many sessions, or forget each one's when `password` is empty: Edit sessions… and Edit
/// Sessions as Table…, for a site whose devices share a login. The list is written once, not once a session. Returns
/// how many were set or forgotten; the first the Keychain refuses stops it, saying which, those before it done
pub fn set_many(user_folder: &Path, ids: &[String], password: &str) -> Result<usize, String> {
    let mut all = self::ids(user_folder);
    let before = all.clone();
    let mut done = 0;
    let mut failed = None;
    for id in ids {
        if let Err(e) = secrets::set(&account(id), password) {
            failed = Some(format!("{id}: {e}"));
            break;
        }
        if password.is_empty() {
            all.remove(id);
        } else {
            all.insert(id.clone());
        }
        done += 1;
    }
    if all != before {
        save_ids(user_folder, &all)?;
    }
    failed.map_or(Ok(done), Err)
}

/// The ids of every session with a password of its own, for Edit Sessions as Table… to say which have one without
/// reading the Keychain
pub fn listed(user_folder: &Path) -> BTreeSet<String> {
    ids(user_folder)
}

/// Follow sessions renamed or moved: `rename` maps an old id to its new one, or None to leave it
pub fn follow(user_folder: &Path, rename: impl Fn(&str) -> Option<String>) {
    for id in ids(user_folder) {
        let Some(new) = rename(&id).filter(|new| *new != id) else { continue };
        if let Some(password) = get(&id) {
            if set(user_folder, &new, &password).is_ok() {
                let _ = set(user_folder, &id, "");
            }
        }
    }
}

/// Drop deleted sessions' passwords
pub fn forget(user_folder: &Path, gone: impl Fn(&str) -> bool) {
    for id in ids(user_folder).into_iter().filter(|id| gone(id)) {
        let _ = set(user_folder, &id, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A user folder of its own for a test
    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("sxt-session-passwords-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn a_password_is_kept_listed_and_forgotten() {
        let dir = folder("kept");
        set(&dir, "Lab/sw1", "secret").unwrap();
        assert_eq!(get("Lab/sw1").as_deref(), Some("secret"));
        assert!(has(&dir, "Lab/sw1"));
        // the list holds ids only
        assert!(!std::fs::read_to_string(file(&dir)).unwrap().contains("secret"));
        set(&dir, "Lab/sw1", "").unwrap();
        assert_eq!(get("Lab/sw1"), None);
        assert!(!has(&dir, "Lab/sw1"));
    }

    #[test]
    fn one_password_is_set_on_many_sessions_and_forgotten_from_them() {
        let dir = folder("many");
        let ids = ["Site/sw1".to_string(), "Site/sw2".to_string(), "@db/Site/sw3".to_string()];
        assert_eq!(set_many(&dir, &ids, "shared").unwrap(), 3);
        for id in &ids {
            assert_eq!(get(id).as_deref(), Some("shared"), "{id}");
        }
        assert_eq!(listed(&dir), ids.iter().cloned().collect());
        assert!(!std::fs::read_to_string(file(&dir)).unwrap().contains("shared"));
        // forgotten from two, the third keeps it
        assert_eq!(set_many(&dir, &ids[..2], "").unwrap(), 2);
        assert_eq!(get("Site/sw1"), None);
        assert_eq!(listed(&dir), ["@db/Site/sw3".to_string()].into_iter().collect());
        set_many(&dir, &ids[2..], "").unwrap();
    }

    #[test]
    fn a_password_follows_its_session_renamed_moved_or_deleted() {
        let dir = folder("follow");
        set(&dir, "Old/sw2", "two").unwrap();
        set(&dir, "@db/Old/sw3", "three").unwrap();
        set(&dir, "Other/sw4", "four").unwrap();
        // a folder renamed: every session below it
        follow(&dir, |id| id.strip_prefix("Old/").map(|rest| format!("New/{rest}")));
        assert_eq!(get("New/sw2").as_deref(), Some("two"));
        assert_eq!(get("Old/sw2"), None);
        assert!(has(&dir, "New/sw2") && !has(&dir, "Old/sw2"));
        // a Database session's id keeps its @db/
        assert!(has(&dir, "@db/Old/sw3"));
        forget(&dir, |id| id == "Other/sw4");
        assert_eq!(get("Other/sw4"), None);
        assert!(!has(&dir, "Other/sw4"));
    }
}
