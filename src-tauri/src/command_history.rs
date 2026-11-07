//! Send Commands' history, kept between runs for as many days as Settings > General > Command bar says, as
//! SuperPuTTY's Command bar "Save history for N days".
//!
//! It's `CommandHistory.json` in the user's own folder (`sessions::user_folder`): only this user's, on this computer,
//! never the shared settings folder. Each command has the time it was last sent, newest first, and is dropped once
//! it's older than the days chosen. With 0 days nothing is saved, and the file goes: the history lasts until the app
//! closes, as it always did. Masked commands never reach here (`SendCommands.svelte` keeps them out), and a command is
//! kept as typed, with its {prompt:} and {secret:} markers, so the values given for them never go to disk.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The history's file name in the user's folder
const FILE: &str = "CommandHistory.json";

/// At most this many commands are kept, however many days: Up and Down, and the ▾ list, stay usable
const MOST: usize = 500;

/// The most days Settings > General takes (ten years), so a mistyped number can't overflow the arithmetic
pub const MOST_DAYS: u32 = 3650;

/// One command sent, and when it was last sent
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    /// The command as typed in the Command box
    pub command: String,
    /// When it was last sent, in seconds since 1970
    pub sent: u64,
}

/// Now, in seconds since 1970
pub fn now() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// The commands sent in the last `days` days, newest first; none with 0 days
pub fn kept(entries: Vec<Entry>, days: u32, now: u64) -> Vec<Entry> {
    if days == 0 {
        return Vec::new();
    }
    let oldest = now.saturating_sub(u64::from(days) * 24 * 60 * 60);
    let mut kept: Vec<Entry> = entries.into_iter().filter(|e| e.sent >= oldest).collect();
    kept.truncate(MOST);
    kept
}

/// The history read from the user's folder, as saved (newest first); empty when there's none or it can't be read
fn read(user_folder: &Path) -> Vec<Entry> {
    std::fs::read_to_string(user_folder.join(FILE))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// Write the history to the user's folder, or take the file away when there's nothing to keep
fn write(user_folder: &Path, entries: &[Entry]) -> Result<(), String> {
    let file = user_folder.join(FILE);
    if entries.is_empty() {
        return match std::fs::remove_file(&file) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(e.to_string()),
            _ => Ok(()),
        };
    }
    std::fs::create_dir_all(user_folder).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(entries).map_err(|e| e.to_string())?;
    std::fs::write(file, text).map_err(|e| e.to_string())
}

/// The commands still kept, newest first, for the Command box when a window opens. Older ones are dropped from the
/// file as they're found, so it doesn't keep what the days no longer allow
pub fn load(user_folder: &Path, days: u32, now: u64) -> Vec<String> {
    let saved = read(user_folder);
    let kept = kept(saved.clone(), days, now);
    if kept != saved {
        let _ = write(user_folder, &kept);
    }
    kept.into_iter().map(|e| e.command).collect()
}

/// Note a command sent: first in the history, as sent now, without its earlier sending
pub fn add(user_folder: &Path, days: u32, command: &str, now: u64) -> Result<(), String> {
    let mut entries = read(user_folder);
    entries.retain(|e| e.command != command);
    entries.insert(0, Entry { command: command.to_string(), sent: now });
    write(user_folder, &kept(entries, days, now))
}

/// Drop what the days no longer allow, after Settings > General changes them (0 takes the file away)
pub fn prune(user_folder: &Path, days: u32, now: u64) -> Result<(), String> {
    let saved = read(user_folder);
    let kept = kept(saved.clone(), days, now);
    if kept == saved { Ok(()) } else { write(user_folder, &kept) }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A day, in seconds
    const DAY: u64 = 24 * 60 * 60;

    /// A user's folder of its own for one test
    fn folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("sxt-command-history-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        folder
    }

    #[test]
    fn commands_sent_are_there_next_time_newest_first_without_repeats() {
        let folder = folder("kept");
        let start = 100 * DAY;
        add(&folder, 7, "show clock", start).unwrap();
        add(&folder, 7, "show version", start + 10).unwrap();
        add(&folder, 7, "show clock", start + 20).unwrap();
        assert_eq!(load(&folder, 7, start + 30), ["show clock", "show version"]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn a_command_older_than_the_days_chosen_is_gone_and_one_sent_again_stays() {
        let folder = folder("days");
        let start = 100 * DAY;
        add(&folder, 7, "show clock", start).unwrap();
        add(&folder, 7, "show version", start + DAY).unwrap();
        // eight days on, show clock is past the seven days; show version, a day younger, is still in them
        assert_eq!(load(&folder, 7, start + 7 * DAY + 1), ["show version"]);
        // and gone from the file too, not only from what's shown
        assert_eq!(read(&folder).len(), 1);
        // sent again, it's as new as the day it was sent
        add(&folder, 7, "show version", start + 7 * DAY + 2).unwrap();
        assert_eq!(load(&folder, 7, start + 14 * DAY), ["show version"]);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn with_0_days_nothing_is_saved_and_a_history_already_saved_goes() {
        let folder = folder("zero");
        let start = 100 * DAY;
        add(&folder, 0, "show clock", start).unwrap();
        assert!(!folder.join(FILE).exists());
        add(&folder, 7, "show clock", start).unwrap();
        assert!(folder.join(FILE).exists());
        // Settings > General set to 0 days: the file goes at once
        prune(&folder, 0, start + 1).unwrap();
        assert!(!folder.join(FILE).exists());
        assert!(load(&folder, 7, start + 2).is_empty());
        let _ = std::fs::remove_dir_all(&folder);
    }
}
