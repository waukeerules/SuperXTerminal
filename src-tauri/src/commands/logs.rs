//! Logs: a tab's session log (opened, written to and closed as the tab's output comes, and read back for Search
//! Output), and the app's own log (Help > Application Log). The session logs' files are `crate::logs`, the app's
//! log `crate::applog`.

use std::path::PathBuf;

use tauri::State;

use crate::{applog, logs, opening, AppState};

/// A log just started: its id for writing, its file, and whether it goes when the tab closes
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LogStarted {
    id: u32,
    path: PathBuf,
    temporary: bool,
}

/// Start a tab's session log: kept in the chosen folder when the tab's profile says so (`keep`, its Session log),
/// else when Settings > General says so (`keep` None); otherwise temporary
#[tauri::command]
pub(crate) fn log_open(state: State<'_, AppState>, logs: State<'_, logs::Logs>, name: String, keep: Option<bool>) -> Result<LogStarted, String> {
    let (keep, folder) = {
        let settings = state.settings.lock().unwrap();
        (keep.unwrap_or(settings.session_log_enabled), settings.session_log_folder.clone())
    };
    let folder = if keep { logs::kept_folder(&folder) } else { logs::temp_folder() };
    let (id, path) = logs.open(&folder, &name)?;
    Ok(LogStarted { id, path, temporary: !keep })
}

/// Add a tab's finished lines to its log
#[tauri::command]
pub(crate) fn log_write(logs: State<'_, logs::Logs>, id: u32, text: String) -> Result<(), String> {
    logs.write(id, &text)
}

/// Close a tab's log (a temporary one is deleted shortly after)
#[tauri::command]
pub(crate) fn log_close(logs: State<'_, logs::Logs>, id: u32, temporary: bool) {
    logs.close(id, temporary);
}

/// A log's text from a byte offset, and the offset after it (Search Output, Highlighted Output)
#[tauri::command]
pub(crate) fn log_read(path: PathBuf, offset: u64) -> Result<(String, u64), String> {
    logs::read_from(&path, offset)
}

/// Highlighted Output's next chunk of a file: base64 bytes (512 KB at most), the offset after them, and whether the
/// file started again
#[tauri::command]
pub(crate) fn log_chunk(path: PathBuf, offset: u64) -> Result<(String, u64, bool), String> {
    logs::read_chunk(&path, offset, 512 * 1024)
}

/// The Log Viewer: entries after `after` (0 for all kept)
#[tauri::command]
pub(crate) fn applog_since(after: u64) -> Vec<applog::Entry> {
    applog::since(after)
}

/// A line from the UI (the status bar's messages, as SuperXPuTTY's ReportStatus logs them)
#[tauri::command]
pub(crate) fn applog_write(level: String, text: String) {
    match level.as_str() {
        "WARN" => applog::warn(text),
        "ERROR" => applog::error(text),
        _ => applog::info(text),
    }
}

/// The Log Viewer's Clear (the file keeps them)
#[tauri::command]
pub(crate) fn applog_clear() {
    applog::clear();
}

/// The Log Viewer's Show Log File: the file in Finder
#[tauri::command]
pub(crate) fn applog_reveal() -> Result<(), String> {
    opening::reveal(&applog::file())
}
