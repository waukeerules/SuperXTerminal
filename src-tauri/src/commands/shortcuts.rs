//! Favorites and Recent, above the Sessions tree: reading them, starring a session, noting one opened, forgetting
//! recents, and whether each group is open. Kept in the user's folder by `crate::shortcuts`; the tray's menu lists
//! them too, so it's refreshed when they change. (The keyboard shortcuts are `settings.rs`'s, from `crate::keys`.)

use tauri::{AppHandle, State};

use crate::{shortcuts, tray, AppState};

/// The Favorites and Recent lists
#[tauri::command]
pub(crate) fn shortcuts_get(state: State<'_, AppState>) -> shortcuts::Shortcuts {
    shortcuts::load(&state.user_folder)
}

/// Add a session to Favorites, or take it out
#[tauri::command]
pub(crate) fn shortcuts_favorite(app: AppHandle, state: State<'_, AppState>, id: String, on: bool) -> Result<shortcuts::Shortcuts, String> {
    let changed = shortcuts::edit(&state.user_folder, |s| {
        s.favorites.retain(|f| *f != id);
        if on {
            s.favorites.push(id);
        }
    });
    tray::refresh(&app);
    changed
}

/// Record a session opened from the tree, first in Recent
#[tauri::command]
pub(crate) fn shortcuts_opened(app: AppHandle, state: State<'_, AppState>, id: String) -> Result<shortcuts::Shortcuts, String> {
    let changed = shortcuts::edit(&state.user_folder, |s| shortcuts::push_recent(&mut s.recent, &id));
    tray::refresh(&app);
    changed
}

/// Take one session out of Recent, or clear it (`id` None)
#[tauri::command]
pub(crate) fn shortcuts_forget_recent(app: AppHandle, state: State<'_, AppState>, id: Option<String>) -> Result<shortcuts::Shortcuts, String> {
    let changed = shortcuts::edit(&state.user_folder, |s| match &id {
        Some(id) => s.recent.retain(|r| r != id),
        None => s.recent.clear(),
    });
    tray::refresh(&app);
    changed
}

/// Remember Favorites or Recent being opened or closed by the user
#[tauri::command]
pub(crate) fn shortcuts_expanded(state: State<'_, AppState>, group: String, open: bool) -> Result<shortcuts::Shortcuts, String> {
    shortcuts::edit(&state.user_folder, |s| {
        if group == "favorites" {
            s.favorites_expanded = open;
        } else {
            s.recent_expanded = open;
        }
    })
}
