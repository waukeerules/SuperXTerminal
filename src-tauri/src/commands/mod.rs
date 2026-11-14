//! The commands the UI (`src/`) calls through Tauri's `invoke`, a module for each area of the app. Each command
//! is a thin step between the page and the code that does the work (`profiles.rs`, `sessions.rs`, `db.rs`...):
//! it takes what it needs from [`crate::AppState`], calls that code, and gives the page the result. `lib.rs`
//! registers them all in `run`; the names the page calls them by are the functions' own.

pub(crate) mod profiles;
