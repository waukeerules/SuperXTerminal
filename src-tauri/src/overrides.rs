//! The user's local overrides, as SuperXPuTTY's SessionOverrides: their own host, port, username, profile, extra
//! arguments or jump host for a session, used instead of the stored value whenever it opens, and their own note for it.
//! For a Database session whose field is locked, or a shared Sessions.xml session, where an edit isn't possible or
//! would change it for everyone. Kept in SessionOverrides.json in the user's own folder, by session id, in
//! SuperXPuTTY's format, and never written into the session itself. Renames, moves and deletions are followed.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::sessions::StoredSession;

/// Set on a launch copy when the jump host is overridden to none, so the folders' and profile's don't apply
pub const NO_JUMP_HOST: &str = "SxtNoJumpHost";

/// One session's overrides; None (or a port of 0) isn't overridden
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Override {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    pub port: u16,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub username: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub putty_profile: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_args: Option<String>,
    /// "" for none, overriding the session's or folder's
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jump_host: Option<String>,
    /// The user's own note, shown instead of the stored one ("" for none)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Turned off (Local overrides' "Use these overrides", or the session's menu): the values are kept but the
    /// session opens as stored, e.g. through CyberArk rather than with the user's own login. Written only when off,
    /// so an entry that's on reads as SuperXPuTTY writes it.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
}

impl Override {
    /// Whether it holds no values; one turned off still holds its own
    pub fn is_empty(&self) -> bool {
        Override { disabled: false, ..self.clone() } == Override::default()
    }
}

fn presets_file(user_folder: &Path) -> PathBuf {
    user_folder.join("OverridePresets.json")
}

/// The user's named sets of overrides ("Local AD login", say), for filling in Local overrides; by name
pub fn presets(user_folder: &Path) -> BTreeMap<String, Override> {
    std::fs::read_to_string(presets_file(user_folder)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Save a preset under its name (replacing one of that name), or remove it (None); returns them all
pub fn set_preset(user_folder: &Path, name: &str, value: Option<Override>) -> Result<BTreeMap<String, Override>, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A preset needs a name.".into());
    }
    let mut all = presets(user_folder);
    match value {
        // a preset is only values: whether overrides are on is each session's own
        Some(value) => all.insert(name.to_string(), Override { disabled: false, ..value }),
        None => all.remove(name),
    };
    std::fs::create_dir_all(user_folder).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&all).map_err(|e| e.to_string())?;
    std::fs::write(presets_file(user_folder), text).map_err(|e| e.to_string())?;
    Ok(all)
}

fn file(user_folder: &Path) -> PathBuf {
    user_folder.join("SessionOverrides.json")
}

/// The overrides by session id; empty if the file is missing or unreadable
pub fn load(user_folder: &Path) -> BTreeMap<String, Override> {
    let read: BTreeMap<String, Override> =
        std::fs::read_to_string(file(user_folder)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    read.into_iter().filter(|(_, o)| !o.is_empty()).collect()
}

fn save(user_folder: &Path, all: &BTreeMap<String, Override>) -> Result<(), String> {
    std::fs::create_dir_all(user_folder).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(all).map_err(|e| e.to_string())?;
    std::fs::write(file(user_folder), text).map_err(|e| e.to_string())
}

/// Set sessions' overrides, removing those left empty; one write for them all
pub fn set(user_folder: &Path, changes: Vec<(String, Override)>) -> Result<BTreeMap<String, Override>, String> {
    let mut all = load(user_folder);
    for (id, value) in changes {
        if value.is_empty() {
            all.remove(&id);
        } else {
            all.insert(id, value);
        }
    }
    save(user_folder, &all)?;
    Ok(all)
}

/// Follow sessions renamed or moved: `rename` maps an old id to its new one, or None to leave it
pub fn follow(user_folder: &Path, rename: impl Fn(&str) -> Option<String>) {
    let all = load(user_folder);
    if !all.keys().any(|id| rename(id).is_some_and(|new| new != *id)) {
        return;
    }
    let moved: BTreeMap<String, Override> = all.into_iter().map(|(id, o)| (rename(&id).unwrap_or(id), o)).collect();
    let _ = save(user_folder, &moved);
}

/// Drop deleted sessions' overrides
pub fn forget(user_folder: &Path, gone: impl Fn(&str) -> bool) {
    let mut all = load(user_folder);
    let before = all.len();
    all.retain(|id, _| !gone(id));
    if all.len() != before {
        let _ = save(user_folder, &all);
    }
}

/// A copy of a session with its overrides in place, for opening it (never saved). An override beats the session's
/// value and its folders', as SuperXPuTTY applies them after the folders' defaults. `on` is Tools > Use Local
/// Overrides: off, no session's are used; a session's own turned off isn't either.
pub fn applied(user_folder: &Path, session: &StoredSession, on: bool) -> StoredSession {
    let mut copy = session.clone();
    if !on {
        return copy;
    }
    let Some(value) = load(user_folder).remove(session.id()).filter(|v| !v.disabled) else { return copy };
    for (name, field) in [
        ("Host", &value.host),
        ("Username", &value.username),
        ("PuttySession", &value.putty_profile),
        ("ExtraArgs", &value.extra_args),
        ("JumpHost", &value.jump_host),
        ("Note", &value.note),
    ] {
        if let Some(v) = field {
            copy.attributes.retain(|(n, _)| n != name);
            copy.attributes.push((name.to_string(), v.clone()));
        }
    }
    if value.port > 0 {
        copy.attributes.retain(|(n, _)| n != "Port");
        copy.attributes.push(("Port".into(), value.port.to_string()));
    }
    if value.jump_host.as_deref() == Some("") {
        copy.attributes.push((NO_JUMP_HOST.into(), "1".into()));
    }
    copy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_superxputty_files_applies_and_follows() {
        let folder = std::env::temp_dir().join(format!("sxt-overrides-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(
            file(&folder),
            r#"{"Lab/sw1":{"Host":"10.0.0.9","Port":2222,"JumpHost":""},"Lab/sw2":{"Port":0}}"#,
        )
        .unwrap();
        let all = load(&folder);
        assert_eq!(all.len(), 1, "an empty entry is dropped");
        let session = StoredSession {
            attributes: vec![("SessionId".into(), "Lab/sw1".into()), ("Host".into(), "sw1".into()), ("JumpHost".into(), "bastion".into())],
        };
        let copy = applied(&folder, &session, true);
        assert_eq!(copy.get("Host"), "10.0.0.9");
        assert_eq!(copy.get("Port"), "2222");
        assert_eq!(copy.get("JumpHost"), "");
        assert_eq!(copy.get(NO_JUMP_HOST), "1");
        follow(&folder, |id| id.strip_prefix("Lab/").map(|rest| format!("Labs/{rest}")));
        assert!(load(&folder).contains_key("Labs/sw1"));
        forget(&folder, |id| id == "Labs/sw1");
        assert!(load(&folder).is_empty());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn turned_off_they_keep_their_values_and_the_session_opens_as_stored() {
        let folder = std::env::temp_dir().join(format!("sxt-overrides-off-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let session = StoredSession { attributes: vec![("SessionId".into(), "Lab/sw1".into()), ("Username".into(), "{cyberark}".into())] };
        let mine = Override { username: Some("admin".into()), ..Override::default() };
        set(&folder, vec![("Lab/sw1".into(), mine.clone())]).unwrap();
        // on: the own username; written as SuperXPuTTY writes it, with no "Disabled"
        assert_eq!(applied(&folder, &session, true).get("Username"), "admin");
        assert!(!std::fs::read_to_string(file(&folder)).unwrap().contains("Disabled"));
        // the global switch off: as stored, the override kept
        assert_eq!(applied(&folder, &session, false).get("Username"), "{cyberark}");
        // the session's own off: as stored, and its values still there for turning it on again
        set(&folder, vec![("Lab/sw1".into(), Override { disabled: true, ..mine })]).unwrap();
        assert_eq!(applied(&folder, &session, true).get("Username"), "{cyberark}");
        assert_eq!(load(&folder)["Lab/sw1"].username.as_deref(), Some("admin"));
        // off with no values left is nothing to keep
        set(&folder, vec![("Lab/sw1".into(), Override { disabled: true, ..Override::default() })]).unwrap();
        assert!(load(&folder).is_empty());
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn presets_are_kept_by_name_without_being_on_or_off() {
        let folder = std::env::temp_dir().join(format!("sxt-presets-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let local = Override { username: Some("{ad_username}".into()), putty_profile: Some("Default Settings".into()), disabled: true, ..Override::default() };
        let all = set_preset(&folder, " Local AD login ", Some(local)).unwrap();
        assert_eq!(all["Local AD login"].username.as_deref(), Some("{ad_username}"));
        assert!(!all["Local AD login"].disabled);
        assert!(set_preset(&folder, "  ", Some(Override::default())).is_err());
        assert!(set_preset(&folder, "Local AD login", None).unwrap().is_empty());
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
