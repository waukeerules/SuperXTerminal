//! Files and links the page asks for by path: reading and writing a text file (the Script Editor's Open and Save
//! as), keeping a file dropped on a tab, opening a file in its app or a web link in the browser, and whether a
//! folder is there and can be written to (Settings > General's settings folder). Opening with the system's tools
//! is `crate::opening`.

use std::path::PathBuf;

use crate::{opening, saved_cache, saving};

/// A text file the user chose (Script Editor's Open file...), with LF line breaks
#[tauri::command]
pub(crate) fn text_file_read(path: PathBuf) -> Result<String, String> {
    std::fs::read_to_string(path).map(|t| t.replace("\r\n", "\n")).map_err(|e| e.to_string())
}

/// Write a text file the user chose (Script Editor's Save to file...), with CRLF as SuperXPuTTY does
#[tauri::command]
pub(crate) fn text_file_write(path: PathBuf, text: String) -> Result<(), String> {
    std::fs::write(path, text.replace("\r\n", "\n").replace('\n', "\r\n")).map_err(|e| e.to_string())
}

/// Where a dropped file is saved: a folder of its own under `base` (so two files of one name don't meet), with the
/// file's own name only (a name with folders in it, or .., can't reach outside it)
pub(crate) fn dropped_file_path(base: &std::path::Path, name: &str, unique: &str) -> PathBuf {
    let own = std::path::Path::new(name).file_name().and_then(|n| n.to_str()).filter(|n| !n.is_empty() && *n != "..").unwrap_or("dropped");
    base.join(unique).join(own)
}

/// A file dropped on a dialog's drop zone (Import Sessions'): the browser gives its contents but not its path, so it's
/// saved in the temporary folder for the dialog to read like a chosen file. Its path, under its own name
#[tauri::command]
pub(crate) fn dropped_file_save(name: String, data: String) -> Result<PathBuf, String> {
    use base64::Engine;
    let bytes = base64::engine::general_purpose::STANDARD.decode(data).map_err(|e| e.to_string())?;
    let unique = format!("{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
    let path = dropped_file_path(&std::env::temp_dir().join("SuperXTerminal-dropped"), &name, &unique);
    std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Open a file in its own app (Search Output's Open log)
#[tauri::command]
pub(crate) fn open_path(path: PathBuf) -> Result<(), String> {
    opening::open_in_app(&path)
}

/// Open a web link (http or https) in the default browser: a link Ctrl+clicked in a terminal, Settings' links, a new
/// version's page. Through the opener plugin, except in an AppImage on Linux, where the plugin's xdg-open would
/// inherit the AppImage's libraries and fail (appimage.rs); there it's the system's xdg-open, given the system's
/// environment. Anything but http and https is refused, as TerminalControl's Ctrl+click is.
#[tauri::command]
pub(crate) fn open_link(url: String) -> Result<(), String> {
    if !(url.starts_with("http://") || url.starts_with("https://")) {
        return Err(format!("Not a web link: {url}"));
    }
    if cfg!(target_os = "linux") && std::env::var_os("APPIMAGE").is_some() {
        let mut command = std::process::Command::new("xdg-open");
        crate::appimage::tidy(&mut command);
        return command.arg(&url).spawn().map(|_| ()).map_err(|e| format!("Couldn't open {url}: {e}"));
    }
    tauri_plugin_opener::open_url(&url, None::<&str>).map_err(|e| format!("Couldn't open {url}: {e}"))
}

/// Whether this account can make files in a folder chosen as the settings folder, so the UI can warn first (see
/// [`saving::writable`])
#[tauri::command]
pub(crate) fn folder_writable(path: String) -> bool {
    saving::writable(std::path::Path::new(&path))
}

/// Whether there's a folder at `path` that can be reached now: asked of a settings folder typed in Settings > General
/// before it's used, as one that isn't there (a slip of the keys, a drive that's away) would show no Saved sessions
#[tauri::command]
pub(crate) fn folder_exists(path: String) -> bool {
    saved_cache::reachable(std::path::Path::new(&path))
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_dropped_file_is_saved_under_its_own_name_in_a_folder_of_its_own() {
        use std::path::Path;
        let base = Path::new("/tmp/SuperXTerminal-dropped");
        assert_eq!(super::dropped_file_path(base, "Sessions.xml", "1-2"), base.join("1-2").join("Sessions.xml"));
        // a name with folders, or .., stays inside
        assert_eq!(super::dropped_file_path(base, "../../etc/passwd", "1-2"), base.join("1-2").join("passwd"));
        assert_eq!(super::dropped_file_path(base, "..", "1-2"), base.join("1-2").join("dropped"));
    }
}
