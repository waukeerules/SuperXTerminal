//! The saved scripts (`<SettingsFolder>/scripts`, shared by the team), as SuperXPuTTY's ScriptLibrary: `.spsl` files
//! are SPSL scripts, `.rhai` files Rhai scripts (rhai_scripts.rs) and `.txt` files are lines to send. Saved with CRLF line endings, so they open cleanly in Notepad
//! too, and read with LF.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One saved script
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    pub name: String,
    /// "spsl", "rhai" or "lines"
    pub kind: String,
}

/// The folder
pub fn folder(settings_folder: &Path) -> PathBuf {
    settings_folder.join("scripts")
}

/// The kinds of script, each with its file extension: SPSL, Rhai, and plain lines to send
const KINDS: [(&str, &str); 3] = [("spsl", ".spsl"), ("rhai", ".rhai"), ("lines", ".txt")];

/// A script's file extension: `.spsl` for SPSL scripts, `.rhai` for Rhai ones, `.txt` for plain ones
fn extension(kind: &str) -> &'static str {
    KINDS.iter().find(|(k, _)| *k == kind).map_or(".txt", |(_, e)| e)
}

/// Where a script of this name and kind is kept
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
            let (kind, _) = KINDS.iter().find(|(_, e)| lower.ends_with(e))?;
            let name = file[..file.rfind('.').unwrap_or(file.len())].to_string();
            Some(Entry { name, kind: (*kind).into() })
        })
        .collect();
    entries.sort_by_key(|e| e.name.to_lowercase());
    entries
}

/// A script's text, with LF line breaks
pub fn load(settings_folder: &Path, name: &str, kind: &str) -> Result<String, String> {
    std::fs::read_to_string(path_of(settings_folder, name, kind)).map(|t| t.replace("\r\n", "\n")).map_err(|e| e.to_string())
}

/// Save a script with CRLF, whole (saving.rs: tried again while OneDrive or antivirus holds it, as they may in a
/// settings folder in Documents); a script of another kind with the same name goes, so a name is one script
pub fn save(settings_folder: &Path, name: &str, kind: &str, text: &str) -> Result<(), String> {
    if let Some(problem) = name_problem(name) {
        return Err(problem);
    }
    std::fs::create_dir_all(folder(settings_folder)).map_err(|e| e.to_string())?;
    let text = text.replace("\r\n", "\n").replace('\n', "\r\n");
    let path = path_of(settings_folder, name, kind);
    // beside it as name.spsl.saving (or .txt.saving), which the list of scripts doesn't show
    let temp_extension = format!("{}.saving", path.extension().and_then(|e| e.to_str()).unwrap_or("txt"));
    crate::saving::save(&path, &temp_extension, text)?;
    for (other, _) in KINDS.iter().filter(|(k, _)| *k != kind) {
        let other = path_of(settings_folder, name, other);
        if other.exists() {
            std::fs::remove_file(other).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Delete a saved script
pub fn delete(settings_folder: &Path, name: &str, kind: &str) -> Result<(), String> {
    std::fs::remove_file(path_of(settings_folder, name, kind)).map_err(|e| e.to_string())
}

/// A login script file chosen with Choose…, as a session or profile keeps it: relative to the settings folder, with
/// forward slashes ("scripts/cisco-login.spsl"), when it's in there, else the path as chosen. Sessions and profiles
/// in a shared folder or database are opened from the Mac and Windows alike, and a full path ("C:\Scripts\..." or
/// "/Users/...") is found on one of them only; the settings folder is where each looks for a relative one. Both are
/// compared as the system resolves them (case, `..`, links, and Windows' `\\?\`), else as written when one can't be
pub fn as_spec(settings_folder: &Path, chosen: &Path) -> String {
    let resolve = |p: &Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    match resolve(chosen).strip_prefix(resolve(settings_folder)) {
        Ok(inside) if !inside.as_os_str().is_empty() => {
            inside.components().map(|c| c.as_os_str().to_string_lossy().into_owned()).collect::<Vec<_>>().join("/")
        }
        _ => chosen.to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_crlf_and_replaces_a_script_of_another_kind() {
        let dir = std::env::temp_dir().join(format!("sxt-scripts-{}", std::process::id()));
        save(&dir, "Login", "lines", "a\nb").unwrap();
        save(&dir, "Login", "spsl", "WAITFOR x\nSENDLINE y").unwrap();
        assert_eq!(list(&dir), vec![Entry { name: "Login".into(), kind: "spsl".into() }]);
        assert_eq!(std::fs::read_to_string(dir.join("scripts/Login.spsl")).unwrap(), "WAITFOR x\r\nSENDLINE y");
        assert_eq!(load(&dir, "Login", "spsl").unwrap(), "WAITFOR x\nSENDLINE y");
        save(&dir, "Login", "rhai", "wait_for(\"x\");").unwrap();
        assert_eq!(list(&dir), vec![Entry { name: "Login".into(), kind: "rhai".into() }]);
        assert!(!dir.join("scripts/Login.spsl").exists());
        assert!(name_problem("bad/name").is_some());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_script_chosen_in_the_settings_folder_is_kept_relative_and_one_elsewhere_whole() {
        let dir = std::env::temp_dir().join(format!("sxt-script-spec-{}", std::process::id()));
        save(&dir, "Cisco login", "spsl", "SENDLINE x").unwrap();
        let elsewhere = std::env::temp_dir().join(format!("sxt-script-elsewhere-{}.spsl", std::process::id()));
        std::fs::write(&elsewhere, "SENDLINE y").unwrap();
        // forward slashes on Windows too, so the Mac finds it
        assert_eq!(as_spec(&dir, &dir.join("scripts").join("Cisco login.spsl")), "scripts/Cisco login.spsl");
        assert_eq!(as_spec(&dir, &elsewhere), elsewhere.to_string_lossy());
        std::fs::remove_file(&elsewhere).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
