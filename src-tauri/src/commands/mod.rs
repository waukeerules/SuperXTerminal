//! The commands the UI (`src/`) calls through Tauri's `invoke`, a module for each area of the app. Each command
//! is a thin step between the page and the code that does the work (`profiles.rs`, `sessions.rs`, `db.rs`...):
//! it takes what it needs from [`crate::AppState`], calls that code, and gives the page the result. `lib.rs`
//! registers them all in `run`; the names the page calls them by are the functions' own.

/// Custom icons, highlighting rule sets and themes, and the UI themes
pub(crate) mod appearance;
/// Database sessions: the connection, reading the table, changing it, and Pending changes
pub(crate) mod database;
/// Opening a session in another app: Remote Desktop, VNC, Terminal, FileZilla
pub(crate) mod external;
/// Text files, dropped files, opening a file or a link, and checking a folder
pub(crate) mod files;
/// File > Import Sessions and the CSV export
pub(crate) mod importing;
/// The saved layouts of tabs and splits
pub(crate) mod layouts;
/// A tab's session log and the app's own log
pub(crate) mod logs;
/// A session's local overrides, their presets, and this computer's serial devices
pub(crate) mod overrides;
/// Settings > Profiles
pub(crate) mod profiles;
/// Login scripts and the Script Editor's files
pub(crate) mod scripts;
/// The button bars and Send Commands' history
pub(crate) mod sending;
/// Changing Saved and Private sessions and folders
pub(crate) mod session_editing;
/// The session files themselves: editing them externally, XML export, backups
pub(crate) mod session_files;
/// Reading the sessions for the tree, and what the session commands share
pub(crate) mod sessions;
/// File > Edit Sessions as Table
pub(crate) mod sessions_table;
/// The Settings window: the settings, the settings folder, the shortcuts
pub(crate) mod settings;
/// Favorites and Recent
pub(crate) mod shortcuts;
/// Move to, Copy to and Edit Several Sessions, across the stores
pub(crate) mod transfer;
/// Environment Variables
pub(crate) mod variables;
