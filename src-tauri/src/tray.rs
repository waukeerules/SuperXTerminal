//! The menu bar icon (Settings > General > Show in the menu bar), as SuperXPuTTY's tray icon (Options > Minimize to
//! tray): its menu opens sessions without bringing the window forward first (Favorites, Recent, and Saved, Private
//! and Database sessions by folder), and while it's shown SuperXTerminal keeps running in the menu bar when its last
//! window closes, as SuperXPuTTY hides in the tray.
//!
//! The menu is made again whenever the sessions are read or Favorites and Recent change. Its items' ids start with
//! "tray:"; `lib.rs` handles them.

use std::collections::BTreeMap;

use tauri::image::Image;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Runtime};

use crate::sessions::{Sessions, Store, StoredSession};

const TRAY: &str = "superxterminal";

/// The icon: a template image (black on clear), which macOS draws to suit a light or dark menu bar
const ICON: &[u8] = include_bytes!("../icons/tray-template@2x.png");

/// The most sessions a submenu lists before "… and N more" (the Quick Selector finds the rest)
const MOST: usize = 200;

/// Show, update or remove the icon, as the setting says
pub fn refresh<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<crate::AppState>();
    let show = state.settings.lock().unwrap().show_in_menu_bar;
    if !show {
        let _ = app.remove_tray_by_id(TRAY);
        return;
    }
    let shortcuts = crate::shortcuts::load(&state.user_folder);
    let menu = {
        let sessions = state.sessions.lock().unwrap();
        build(app, &sessions, &shortcuts.favorites, &shortcuts.recent)
    };
    let Ok(menu) = menu else { return };
    if let Some(tray) = app.tray_by_id(TRAY) {
        let _ = tray.set_menu(Some(menu));
        return;
    }
    let Ok(icon) = Image::from_bytes(ICON) else { return };
    let _ = TrayIconBuilder::with_id(TRAY)
        .icon(icon)
        .icon_as_template(true)
        .tooltip("SuperXTerminal")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .build(app);
}

/// The menu
fn build<R: Runtime>(app: &AppHandle<R>, sessions: &Sessions, favorites: &[String], recent: &[String]) -> tauri::Result<Menu<R>> {
    let item = |id: &str, label: &str| MenuItem::with_id(app, id, label, true, None::<&str>);
    let find = |id: &str| -> Option<(Store, &StoredSession)> {
        [(Store::Saved, &sessions.saved), (Store::Private, &sessions.private), (Store::Database, &sessions.database)]
            .into_iter()
            .find_map(|(store, list)| list.iter().find(|s| s.id() == id).map(|s| (store, s)))
    };
    let group = |title: &str, ids: &[String]| -> tauri::Result<Submenu<R>> {
        let found: Vec<(Store, &StoredSession)> = ids.iter().filter_map(|id| find(id)).collect();
        let items: Vec<MenuItem<R>> = found.iter().map(|(store, s)| session_item(app, *store, s)).collect::<tauri::Result<_>>()?;
        let refs: Vec<&dyn IsMenuItem<R>> = items.iter().map(|i| i as &dyn IsMenuItem<R>).collect();
        Submenu::with_items(app, title, !items.is_empty(), &refs)
    };
    let favorites = group("Favorites", favorites)?;
    let recent = group("Recent", recent)?;
    let saved = root(app, "Saved sessions", Store::Saved, &sessions.saved)?;
    let private = root(app, "Private sessions", Store::Private, &sessions.private)?;
    let database = root(app, "Database sessions", Store::Database, &sessions.database)?;
    Menu::with_items(
        app,
        &[
            &item("tray:openSession", "Open Session…")?,
            &PredefinedMenuItem::separator(app)?,
            &favorites,
            &recent,
            &PredefinedMenuItem::separator(app)?,
            &saved,
            &private,
            &database,
            &PredefinedMenuItem::separator(app)?,
            &item("tray:newLocalShell", "New Local Shell")?,
            &item("tray:newWindow", "New Window")?,
            &item("tray:show", "Show SuperXTerminal")?,
            &PredefinedMenuItem::separator(app)?,
            &item("tray:quit", "Quit SuperXTerminal")?,
        ],
    )
}

/// A session's item: its name; choosing it opens it ("tray:open:<store>:<id>")
fn session_item<R: Runtime>(app: &AppHandle<R>, store: Store, session: &StoredSession) -> tauri::Result<MenuItem<R>> {
    let name = match session.get("SessionName") {
        "" => session.id().rsplit('/').next().unwrap_or(""),
        name => name,
    };
    let store = match store {
        Store::Saved => "saved",
        Store::Private => "private",
        Store::Database => "database",
    };
    MenuItem::with_id(app, format!("tray:open:{store}:{}", session.id()), name, true, None::<&str>)
}

/// A folder in the menu: its folders (by name, ignoring case), then its sessions
#[derive(Default)]
struct Folder<'a> {
    folders: BTreeMap<String, (String, Folder<'a>)>,
    sessions: Vec<&'a StoredSession>,
}

/// A root's submenu, folders as submenus, as the Sessions tree shows them
fn root<R: Runtime>(app: &AppHandle<R>, title: &str, store: Store, list: &[StoredSession]) -> tauri::Result<Submenu<R>> {
    let mut top = Folder::default();
    for session in list {
        let mut node = &mut top;
        for part in session.folder().split('/').filter(|p| !p.is_empty()) {
            node = &mut node.folders.entry(part.to_lowercase()).or_insert_with(|| (part.to_string(), Folder::default())).1;
        }
        node.sessions.push(session);
    }
    folder_menu(app, title, store, &mut top, !list.is_empty())
}

fn folder_menu<R: Runtime>(app: &AppHandle<R>, title: &str, store: Store, folder: &mut Folder, enabled: bool) -> tauri::Result<Submenu<R>> {
    let mut subs = Vec::new();
    for (name, child) in folder.folders.values_mut() {
        subs.push(folder_menu(app, name, store, child, true)?);
    }
    let name_of = |s: &StoredSession| s.id().rsplit('/').next().unwrap_or("").to_lowercase();
    folder.sessions.sort_by_key(|s| name_of(s));
    let mut items = Vec::new();
    for session in folder.sessions.iter().take(MOST) {
        items.push(session_item(app, store, session)?);
    }
    let more = folder.sessions.len().saturating_sub(MOST);
    let more_item = (more > 0).then(|| MenuItem::with_id(app, "tray:openSession", format!("… and {more} more (Open Session…)"), true, None::<&str>)).transpose()?;
    let mut refs: Vec<&dyn IsMenuItem<R>> = subs.iter().map(|s| s as &dyn IsMenuItem<R>).collect();
    refs.extend(items.iter().map(|i| i as &dyn IsMenuItem<R>));
    if let Some(more) = &more_item {
        refs.push(more);
    }
    Submenu::with_items(app, title, enabled, &refs)
}
