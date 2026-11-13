//! SuperXTerminal's own log, as SuperXPuTTY's log4net log: what the app did (started, read the sessions, opened and
//! closed sessions, saved settings, what went wrong), kept in memory for the Log Viewer pane and written to
//! `~/Library/Logs/SuperXTerminal/SuperXTerminal.log` (the previous one kept as `.1.log` once it passes 5 MB).
//! Terminal output isn't here: that's in each tab's session log.

use std::collections::VecDeque;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

use serde::Serialize;

/// One entry
#[derive(Clone, Serialize)]
pub struct Entry {
    /// Counts up from 1, so the pane asks only for what's new
    pub seq: u64,
    /// "2026-11-02 14:05:01.123"
    pub time: String,
    /// "INFO", "WARN" or "ERROR"
    pub level: &'static str,
    pub text: String,
}

/// How many entries the pane can show
const KEPT: usize = 2000;
/// The file's size before it starts again
const ROLL_AT: u64 = 5 * 1024 * 1024;

/// The last sequence number given, and the entries kept for the log viewer (the newest [`KEPT`])
static LOG: Mutex<(u64, VecDeque<Entry>)> = Mutex::new((0, VecDeque::new()));

/// The log file: in ~/Library/Logs on the Mac, %LOCALAPPDATA%\SuperXTerminal\logs on Windows, and on Linux
/// ~/.local/share/SuperXTerminal/logs (its XDG data folder, which data_local_dir gives there)
pub fn file() -> PathBuf {
    // the tests log as the app does (a drive left out, a request refused): into a file of their own in the temporary
    // folder, not among the user's own app's lines
    if cfg!(test) {
        return std::env::temp_dir().join("SuperXTerminal-tests.log");
    }
    if cfg!(target_os = "macos") {
        dirs::home_dir().unwrap_or_default().join("Library/Logs/SuperXTerminal/SuperXTerminal.log")
    } else {
        dirs::data_local_dir().unwrap_or_else(std::env::temp_dir).join("SuperXTerminal").join("logs").join("SuperXTerminal.log")
    }
}

/// Keep an entry and add it to the log file, starting the file again once it reaches [`ROLL_AT`]
fn write(level: &'static str, text: &str) {
    let time = chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f").to_string();
    {
        let mut log = LOG.lock().unwrap();
        log.0 += 1;
        let seq = log.0;
        log.1.push_back(Entry { seq, time: time.clone(), level, text: text.to_string() });
        while log.1.len() > KEPT {
            log.1.pop_front();
        }
    }
    let path = file();
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    if std::fs::metadata(&path).is_ok_and(|m| m.len() > ROLL_AT) {
        let _ = std::fs::rename(&path, path.with_extension("1.log"));
    }
    if let Ok(mut out) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(out, "{time} {level:<5} {text}");
    }
}

/// Log something that happened, as it should
pub fn info(text: impl AsRef<str>) {
    write("INFO", text.as_ref());
}

/// Log something that went wrong but the app carried on from
pub fn warn(text: impl AsRef<str>) {
    write("WARN", text.as_ref());
}

/// Log something that failed
pub fn error(text: impl AsRef<str>) {
    write("ERROR", text.as_ref());
}

/// The entries after `seq` (0 for all kept)
pub fn since(seq: u64) -> Vec<Entry> {
    LOG.lock().unwrap().1.iter().filter(|e| e.seq > seq).cloned().collect()
}

/// Empty the pane's entries (the file keeps them)
pub fn clear() {
    LOG.lock().unwrap().1.clear();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_count_up_and_are_kept() {
        let before = since(0).last().map(|e| e.seq).unwrap_or(0);
        info("one");
        warn("two");
        let new = since(before);
        assert!(new.len() >= 2);
        assert!(new.iter().any(|e| e.text == "two" && e.level == "WARN"));
        assert!(new.windows(2).all(|w| w[0].seq < w[1].seq));
    }
}
