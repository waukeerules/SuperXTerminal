//! The saved scripts (`<SettingsFolder>/scripts`, shared by the team), as SuperXPuTTY's ScriptLibrary: `.spsl` files
//! are SPSL scripts and `.txt` files are lines to send. Saved with CRLF line endings, so they open cleanly in Notepad
//! too, and read with LF.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One saved script
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    /// "spsl" or "lines"
    pub kind: String,
}

/// The folder
pub fn folder(settings_folder: &Path) -> PathBuf {
    settings_folder.join("scripts")
}

fn extension(kind: &str) -> &'static str {
    if kind == "spsl" { ".spsl" } else { ".txt" }
}

fn path_of(settings_folder: &Path, name: &str, kind: &str) -> PathBuf {
    folder(settings_folder).join(format!("{}{}", name.trim(), extension(kind)))
}

/// Why a name can't be used, or None: 1 to 60 letters, digits, spaces, dots, dashes or underscores
pub fn name_problem(name: &str) -> Option<String> {
    let name = name.trim();
    let ok = (1..=60).contains(&name.chars().count()) && name.chars().all(|c| c.is_ascii_alphanumeric() || " ._-".contains(c));
    (!ok).then(|| "Use 1 to 60 letters, digits, spaces, dots, dashes or underscores.".to_string())
}

/// The saved scripts, by name ignoring case
pub fn list(settings_folder: &Path) -> Vec<Entry> {
    let Ok(read) = std::fs::read_dir(folder(settings_folder)) else { return Vec::new() };
    let mut entries: Vec<Entry> = read
        .filter_map(|e| e.ok())
        .filter_map(|e| {
            let file = e.file_name().to_string_lossy().to_string();
            let lower = file.to_lowercase();
            let kind = if lower.ends_with(".spsl") { "spsl" } else if lower.ends_with(".txt") { "lines" } else { return None };
            let name = file[..file.rfind('.').unwrap_or(file.len())].to_string();
            Some(Entry { name, kind: kind.into() })
        })
        .collect();
    entries.sort_by_key(|e| e.name.to_lowercase());
    entries
}

/// A script's text, with LF line breaks
pub fn load(settings_folder: &Path, name: &str, kind: &str) -> Result<String, String> {
    std::fs::read_to_string(path_of(settings_folder, name, kind)).map(|t| t.replace("\r\n", "\n")).map_err(|e| e.to_string())
}

/// Save a script with CRLF; a script of the other kind with the same name goes
pub fn save(settings_folder: &Path, name: &str, kind: &str, text: &str) -> Result<(), String> {
    if let Some(problem) = name_problem(name) {
        return Err(problem);
    }
    std::fs::create_dir_all(folder(settings_folder)).map_err(|e| e.to_string())?;
    let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
    std::fs::write(path_of(settings_folder, name, kind), text).map_err(|e| e.to_string())?;
    let other = path_of(settings_folder, name, if kind == "spsl" { "lines" } else { "spsl" });
    if other.exists() {
        std::fs::remove_file(other).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Delete a saved script
pub fn delete(settings_folder: &Path, name: &str, kind: &str) -> Result<(), String> {
    std::fs::remove_file(path_of(settings_folder, name, kind)).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_crlf_and_replaces_the_other_kind() {
        let dir = std::env::temp_dir().join(format!("sxt-scripts-{}", std::process::id()));
        save(&dir, "Login", "lines", "a\nb").unwrap();
        save(&dir, "Login", "spsl", "WAITFOR x\nSENDLINE y").unwrap();
        assert_eq!(list(&dir), vec![Entry { name: "Login".into(), kind: "spsl".into() }]);
        assert_eq!(std::fs::read_to_string(dir.join("scripts/Login.spsl")).unwrap(), "WAITFOR x\r\nSENDLINE y");
        assert_eq!(load(&dir, "Login", "spsl").unwrap(), "WAITFOR x\nSENDLINE y");
        assert!(name_problem("bad/name").is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
