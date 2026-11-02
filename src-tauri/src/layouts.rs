//! Layouts, as SuperXPuTTY's: the tabs open (each tab's session and name), how they're split, and the panes, saved
//! by name and opened again later or at start (the default layout).
//!
//! SuperXPuTTY saves DockPanelSuite XML in `<SettingsFolder>/layouts`, which the Mac can't read; Mac layouts are JSON
//! in `<SettingsFolder>/layouts-mac` (docs/PLAN.md), so the two never mix. The layout itself is the UI's (the tab
//! area's own JSON and the panes); this keeps the files. "<Auto Restore>" is the user's own, in their folder: saved
//! as tabs change, and opened at start when it's the default layout.

use std::path::{Path, PathBuf};

/// The name of the layout saved as the tabs change, as SuperXPuTTY's LayoutData.AutoRestore
pub const AUTO_RESTORE: &str = "<Auto Restore>";

/// The folder of a team's Mac layouts
pub fn folder(shared_folder: &Path) -> PathBuf {
    shared_folder.join("layouts-mac")
}

/// The file of a layout: the auto-restore one in the user's folder, the others in the shared folder
fn path_of(shared_folder: &Path, user_folder: &Path, name: &str) -> PathBuf {
    if name == AUTO_RESTORE {
        user_folder.join("AutoRestoreLayout-mac.json")
    } else {
        folder(shared_folder).join(format!("{}.json", name.trim()))
    }
}

/// Why a name can't be used, or None: 1 to 60 letters, digits, spaces, dots, dashes or underscores
pub fn name_problem(name: &str) -> Option<String> {
    let name = name.trim();
    let ok = (1..=60).contains(&name.chars().count()) && name.chars().all(|c| c.is_ascii_alphanumeric() || " ._-()".contains(c));
    (!ok).then(|| "Use 1 to 60 letters, digits, spaces, dots, dashes, underscores or brackets.".to_string())
}

/// The layouts: "<Auto Restore>" first, then the saved ones by name ignoring case
pub fn list(shared_folder: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder(shared_folder))
        .map(|read| {
            read.flatten()
                .filter_map(|e| {
                    let path = e.path();
                    (path.extension().and_then(|x| x.to_str()) == Some("json")).then(|| path.file_stem()?.to_str().map(str::to_string)).flatten()
                })
                .collect()
        })
        .unwrap_or_default();
    names.sort_by_key(|n| n.to_lowercase());
    std::iter::once(AUTO_RESTORE.to_string()).chain(names).collect()
}

/// A layout's text; None when it has never been saved
pub fn read(shared_folder: &Path, user_folder: &Path, name: &str) -> Option<String> {
    std::fs::read_to_string(path_of(shared_folder, user_folder, name)).ok()
}

/// Save a layout, replacing one of that name
pub fn save(shared_folder: &Path, user_folder: &Path, name: &str, text: &str) -> Result<(), String> {
    if name != AUTO_RESTORE {
        if let Some(problem) = name_problem(name) {
            return Err(problem);
        }
    }
    let path = path_of(shared_folder, user_folder, name);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let temp = path.with_extension("json.saving");
    std::fs::write(&temp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&temp, &path).map_err(|e| e.to_string())
}

/// Delete a saved layout (not the auto-restore one)
pub fn delete(shared_folder: &Path, name: &str) -> Result<(), String> {
    if name == AUTO_RESTORE {
        return Err("The automatic layout can't be deleted.".into());
    }
    std::fs::remove_file(folder(shared_folder).join(format!("{}.json", name.trim()))).map_err(|e| e.to_string())
}

/// Rename a saved layout
pub fn rename(shared_folder: &Path, from: &str, to: &str) -> Result<(), String> {
    if from == AUTO_RESTORE {
        return Err("The automatic layout can't be renamed.".into());
    }
    if let Some(problem) = name_problem(to) {
        return Err(problem);
    }
    let dir = folder(shared_folder);
    let target = dir.join(format!("{}.json", to.trim()));
    if target.exists() && !from.trim().eq_ignore_ascii_case(to.trim()) {
        return Err(format!("There's already a layout called {}.", to.trim()));
    }
    std::fs::rename(dir.join(format!("{}.json", from.trim())), target).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_lists_renames_and_deletes() {
        let dir = std::env::temp_dir().join(format!("sxt-layouts-{}", std::process::id()));
        let (shared, user) = (dir.join("shared"), dir.join("user"));
        save(&shared, &user, "Core switches", "{\"a\":1}").unwrap();
        save(&shared, &user, AUTO_RESTORE, "{\"auto\":1}").unwrap();
        assert!(save(&shared, &user, "bad/name", "{}").is_err());
        assert_eq!(list(&shared), [AUTO_RESTORE, "Core switches"]);
        assert_eq!(read(&shared, &user, AUTO_RESTORE).unwrap(), "{\"auto\":1}");
        assert!(user.join("AutoRestoreLayout-mac.json").exists(), "the automatic layout is the user's own");
        rename(&shared, "Core switches", "Core").unwrap();
        assert_eq!(read(&shared, &user, "Core").unwrap(), "{\"a\":1}");
        delete(&shared, "Core").unwrap();
        assert_eq!(list(&shared), [AUTO_RESTORE]);
        assert!(delete(&shared, AUTO_RESTORE).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
