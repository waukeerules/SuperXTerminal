//! Session logs, as SuperXPuTTY's SessionLog: each tab writes what it shows to a file, named for the session and
//! when it started (`Lab Switch_20261102_014501_123.log`). Kept in a folder the user chooses when "Keep a log of every
//! session" is on; otherwise temporary, deleted a few seconds after the tab closes, and swept when a day old. Search
//! Output and Highlighted Output read them. UTF-8 without a BOM, CRLF line endings, as on Windows.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

/// The open logs, by the id given to their tab
#[derive(Default)]
pub struct Logs {
    next: AtomicU32,
    files: Mutex<HashMap<u32, (File, PathBuf)>>,
}

/// Where temporary logs go
pub fn temp_folder() -> PathBuf {
    std::env::temp_dir().join("SuperXTerminal").join("logs")
}

/// Where kept logs go: the chosen folder, else Documents/SuperXTerminal/logs
pub fn kept_folder(chosen: &str) -> PathBuf {
    let chosen = chosen.trim();
    if chosen.is_empty() {
        dirs::document_dir().or_else(dirs::home_dir).unwrap_or_else(std::env::temp_dir).join("SuperXTerminal").join("logs")
    } else {
        PathBuf::from(chosen)
    }
}

/// A log's file name: the session's name with anything odd as "_", then the start time
pub fn file_name(name: &str, now: chrono::DateTime<chrono::Local>) -> String {
    let name = if name.trim().is_empty() { "session" } else { name.trim() };
    let clean: String = name.chars().map(|c| if c.is_alphanumeric() || "-_. ".contains(c) { c } else { '_' }).collect();
    format!("{clean}_{}.log", now.format("%Y%m%d_%H%M%S_%3f"))
}

impl Logs {
    /// Start a log for a tab; returns its id and path
    pub fn open(&self, folder: &Path, name: &str) -> Result<(u32, PathBuf), String> {
        std::fs::create_dir_all(folder).map_err(|e| format!("Couldn't make the log folder {}: {e}", folder.display()))?;
        let path = folder.join(file_name(name, chrono::Local::now()));
        let file = File::create(&path).map_err(|e| format!("Couldn't start the log {}: {e}", path.display()))?;
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.files.lock().unwrap().insert(id, (file, path.clone()));
        Ok((id, path))
    }

    /// Add text to a log (the window adds the time stamps and CRLF line endings)
    pub fn write(&self, id: u32, text: &str) -> Result<(), String> {
        let mut files = self.files.lock().unwrap();
        let (file, _) = files.get_mut(&id).ok_or("The log is closed")?;
        file.write_all(text.as_bytes()).and_then(|_| file.flush()).map_err(|e| e.to_string())
    }

    /// Close a log; a temporary one is deleted a few seconds later, so a reader can finish with it
    pub fn close(&self, id: u32, temporary: bool) {
        let closed = self.files.lock().unwrap().remove(&id);
        if let (Some((_, path)), true) = (closed, temporary) {
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_secs(5));
                let _ = std::fs::remove_file(path);
            });
        }
    }
}

/// Delete temporary logs more than a day old (left by a crash), as SuperXPuTTY does at start
pub fn sweep_temp() {
    let Ok(entries) = std::fs::read_dir(temp_folder()) else { return };
    let day_ago = SystemTime::now() - Duration::from_secs(24 * 60 * 60);
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        let old = entry.metadata().and_then(|m| m.modified()).map(|t| t < day_ago).unwrap_or(false);
        if old && path.extension().is_some_and(|e| e == "log") {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// A log's text from a byte offset (Search Output reads it all; Highlighted Output follows it)
pub fn read_from(path: &Path, offset: u64) -> Result<(String, u64), String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    let start = offset.min(length);
    file.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok((String::from_utf8_lossy(&bytes).into_owned(), start + bytes.len() as u64))
}

/// Up to `max` bytes of a file from `offset`, base64 (Highlighted Output's chunks), the offset after them, and whether
/// the file is now shorter than `offset` (it started again)
pub fn read_chunk(path: &Path, offset: u64, max: u64) -> Result<(String, u64, bool), String> {
    use base64::Engine;
    use std::io::{Read, Seek, SeekFrom};
    let mut file = File::open(path).map_err(|e| e.to_string())?;
    let length = file.metadata().map_err(|e| e.to_string())?.len();
    if length < offset {
        return Ok((String::new(), 0, true));
    }
    file.seek(SeekFrom::Start(offset)).map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    file.take(max).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    Ok((base64::engine::general_purpose::STANDARD.encode(&bytes), offset + bytes.len() as u64, false))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_logs_as_superxputty() {
        let at = chrono::Local::now();
        let name = file_name("Lab/Core: sw1", at);
        assert!(name.starts_with("Lab_Core_ sw1_"), "{name}");
        assert!(name.ends_with(".log"));
    }

    #[test]
    fn writes_and_reads_back() {
        let logs = Logs::default();
        let folder = std::env::temp_dir().join(format!("sxt-logs-{}", std::process::id()));
        let (id, path) = logs.open(&folder, "test").unwrap();
        logs.write(id, "one\r\ntwo\r\n").unwrap();
        let (text, end) = read_from(&path, 0).unwrap();
        assert_eq!(text, "one\r\ntwo\r\n");
        assert_eq!(read_from(&path, end).unwrap().0, "");
        logs.close(id, false);
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
