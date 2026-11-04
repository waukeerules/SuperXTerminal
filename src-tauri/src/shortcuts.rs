//! The user's Favorites and Recent sessions, as SuperXPuTTY's SessionShortcuts: lists of session ids, shown as groups
//! at the top of the Sessions tree, kept in SessionShortcuts.json in the user's own folder (not the shared settings
//! folder, so starring a session doesn't change it for the team). The file also remembers whether each group was
//! left open. Ids of sessions that no longer exist are skipped when shown, and renames and moves are followed.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// How many recently opened sessions are kept, as SuperXPuTTY's RecentCount
const RECENT_COUNT: usize = 10;

/// The file, in SuperXPuTTY's format
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase", default)]
pub struct Shortcuts {
    /// Favourite session ids, in the order they were added
    pub favorites: Vec<String>,
    /// Recently opened session ids, most recent first
    pub recent: Vec<String>,
    pub favorites_expanded: bool,
    pub recent_expanded: bool,
}

/// Where the lists are kept, in the user folder
fn file(user_folder: &Path) -> PathBuf {
    user_folder.join("SessionShortcuts.json")
}

/// The lists; empty if the file is missing or unreadable
pub fn load(user_folder: &Path) -> Shortcuts {
    std::fs::read_to_string(file(user_folder)).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// Change the lists and save them
pub fn edit(user_folder: &Path, change: impl FnOnce(&mut Shortcuts)) -> Result<Shortcuts, String> {
    let mut shortcuts = load(user_folder);
    change(&mut shortcuts);
    std::fs::create_dir_all(user_folder).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&shortcuts).map_err(|e| e.to_string())?;
    std::fs::write(file(user_folder), text).map_err(|e| e.to_string())?;
    Ok(shortcuts)
}

/// Put a session first in Recent, without duplicates, keeping the newest ten (SuperXPuTTY's PushRecent)
pub fn push_recent(recent: &mut Vec<String>, id: &str) {
    recent.retain(|r| r != id);
    recent.insert(0, id.to_string());
    recent.truncate(RECENT_COUNT);
}

/// Follow sessions that were renamed or moved: `rename` maps an old id to its new one, or None to leave it. A new id
/// already in a list just drops the old one (SuperXPuTTY's ReplaceId)
pub fn follow(user_folder: &Path, rename: impl Fn(&str) -> Option<String>) {
    let mut shortcuts = load(user_folder);
    let mut changed = false;
    for list in [&mut shortcuts.favorites, &mut shortcuts.recent] {
        let mut out: Vec<String> = Vec::with_capacity(list.len());
        for id in list.iter() {
            let new = rename(id).unwrap_or_else(|| id.clone());
            changed |= new != *id;
            if !out.contains(&new) {
                out.push(new);
            }
        }
        *list = out;
    }
    if changed {
        let _ = edit(user_folder, |s| *s = shortcuts);
    }
}

/// Forget deleted sessions in both lists
pub fn forget(user_folder: &Path, gone: impl Fn(&str) -> bool) {
    let shortcuts = load(user_folder);
    if shortcuts.favorites.iter().chain(&shortcuts.recent).any(|id| gone(id)) {
        let _ = edit(user_folder, |s| {
            s.favorites.retain(|id| !gone(id));
            s.recent.retain(|id| !gone(id));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recent_keeps_ten_newest_first() {
        let mut recent: Vec<String> = (0..10).map(|i| i.to_string()).collect();
        push_recent(&mut recent, "5");
        assert_eq!(recent[..3], ["5", "0", "1"]);
        push_recent(&mut recent, "new");
        assert_eq!(recent.len(), 10);
        assert_eq!(recent[0], "new");
        assert!(!recent.contains(&"9".to_string()));
    }

    #[test]
    fn follows_renames_and_reads_superxputty_files() {
        let folder = std::env::temp_dir().join(format!("sxt-shortcuts-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(file(&folder), r#"{"Favorites":["Lab/sw1","Lab/sw2"],"Recent":["Lab/sw1"],"FavoritesExpanded":true}"#).unwrap();
        follow(&folder, |id| id.strip_prefix("Lab/").map(|rest| format!("Labs/{rest}")));
        let s = load(&folder);
        assert_eq!(s.favorites, ["Labs/sw1", "Labs/sw2"]);
        assert_eq!(s.recent, ["Labs/sw1"]);
        assert!(s.favorites_expanded && !s.recent_expanded);
        forget(&folder, |id| id == "Labs/sw2");
        assert_eq!(load(&folder).favorites, ["Labs/sw1"]);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
