//! SuperXTerminal's Rust core: the commands the UI (`src/`) calls through Tauri's `invoke`.

mod algorithms;
mod applog;
mod appimage;
mod askpass;
mod buttonbars;
mod cli;
mod charset;
mod onepassword;
mod opening;
mod own_process;
mod ui_themes;
mod clipboard;
mod clipboard_contents;
mod clipboard_files;
#[cfg(any(target_os = "macos", test))]
mod clipboard_forms;
mod command_history;
mod command_tap;
mod commands;
mod console;
mod db;
mod demo;
mod demo_device;
mod editing;
mod external;
mod file_watch;
mod folders;
mod highlighting;
mod importing;
mod keys;
mod launch;
mod layouts;
mod logs;
mod ppk;
mod profiles;
mod pty;
mod putty_reg;
mod rdp;
mod rdp_clipboard;
mod rdp_drives;
mod rdp_options;
mod rdp_sound;
mod rdp_win_keys;
mod remount;
mod rhai_scripts;
mod scripts;
mod secrets;
mod server_load;
mod outbox;
mod overrides;
// taskbar pins exist on Windows alone
#[cfg(windows)]
mod pins;
mod saved_cache;
mod sessions;
mod session_passwords;
mod shortcuts;
mod streams;
mod tray;
mod variables;
mod settings;
mod saving;
mod sftp;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use profiles::Profiles;
use commands::database::DbState;
use commands::sessions::FilesRead;
use sessions::Sessions;
use settings::AppSettings;

/// What every command shares: the user's folder, their settings and the sessions as last read
pub struct AppState {
    user_folder: PathBuf,
    settings: Mutex<AppSettings>,
    sessions: Mutex<Sessions>,
    profiles: Mutex<Profiles>,
    /// Database sessions' state: the change token of the last read, and how the last read went
    database: Mutex<DbState>,
    /// The Database sessions as last read from the table (or its cache), before queued changes are applied on top
    db_server: Mutex<Vec<sessions::StoredSession>>,
    /// The session files' contents when last read, so a change made elsewhere can be noticed
    files_seen: Mutex<file_watch::Seen>,
    /// What the last read of the session files said; None until they've been read ([`sessions_shown`])
    files_read: Mutex<Option<FilesRead>>,
}

/// The app's settings, for the UI
#[tauri::command]
fn settings_get(state: State<'_, AppState>) -> AppSettings {
    state.settings.lock().unwrap().clone()
}

/// The custom icons in `<SettingsFolder>/icons`: (name, PNG as a data URL) for each `.png`, as SuperXPuTTY's CustomIcons
#[tauri::command]
fn icons_custom(state: State<'_, AppState>) -> Vec<(String, String)> {
    use base64::Engine;
    let Some(folder) = state.settings.lock().unwrap().settings_folder.clone() else { return Vec::new() };
    let Ok(entries) = std::fs::read_dir(folder.join("icons")) else { return Vec::new() };
    let mut icons = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("png")) != Some(true) {
            continue;
        }
        let (Some(name), Ok(bytes)) = (path.file_stem().and_then(|n| n.to_str()), std::fs::read(&path)) else { continue };
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        icons.push((name.to_string(), format!("data:image/png;base64,{data}")));
    }
    icons
}

/// The highlighting rule sets and themes in the team's and the user's `highlighting` folders (the UI adds the defaults)
#[tauri::command]
fn highlighting_load(state: State<'_, AppState>) -> highlighting::Highlighting {
    let shared = state.settings.lock().unwrap().settings_folder.clone().map(|f| f.join("highlighting"));
    let user = state.user_folder.join("highlighting");
    let mut folders: Vec<&std::path::Path> = Vec::new();
    if let Some(shared) = &shared {
        folders.push(shared);
    }
    folders.push(&user);
    highlighting::load(&folders)
}

/// The settings folder, or the user's folder when none is chosen, for shared files (ButtonBars.xml, scripts)
fn shared_folder(state: &AppState) -> PathBuf {
    state.settings.lock().unwrap().settings_folder.clone().unwrap_or_else(|| state.user_folder.clone())
}

/// The user's own highlighting folder (Highlighted Output's folder button), made if it isn't there
#[tauri::command]
fn highlighting_user_folder(state: State<'_, AppState>) -> Result<PathBuf, String> {
    let folder = state.user_folder.join("highlighting");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    Ok(folder)
}

/// What this computer's ssh has for each kind of algorithm, and its default order, for the algorithm lists in
/// Settings > Profiles and Edit Session (algorithms.rs)
#[tauri::command]
fn ssh_algorithms_available() -> Vec<algorithms::Available> {
    algorithms::available()
}

/// Open a session in another app: "open" for RDP (Windows App) and VNC (Screen Sharing), "terminal" for Connect
/// external (its ssh command in a Terminal window), "filezilla" for FileZilla; returns what to say in the status bar
// async: a 1Password reference in its values waits for the user's approval, which mustn't hold up the window
#[tauri::command(async)]
fn session_external(state: State<'_, AppState>, store: sessions::Store, id: String, how: String) -> Result<String, String> {
    let profiles = state.profiles.lock().unwrap().clone();
    let values = variables::values(&state.user_folder);
    let (serial_devices, overrides_on) = {
        let settings = state.settings.lock().unwrap();
        (settings.serial_devices.clone(), settings.use_local_overrides)
    };
    let sessions = state.sessions.lock().unwrap();
    let session = &overrides::applied(&state.user_folder, sessions.find(store, &id).ok_or_else(|| format!("The session {id} is gone."))?, overrides_on);
    // the folders' username and port, as for a tab
    let mut username = session.get("Username").to_string();
    let mut port = session.port();
    for folder in sessions.along(store, session.folder()) {
        if username.is_empty() {
            username = folder.username.clone();
        }
        if port == 0 {
            port = folder.port;
        }
    }
    let host = variables::substitute(session.get("Host").trim(), &values);
    let username = variables::substitute(&username, &values);
    match (how.as_str(), session.get("Proto").to_ascii_uppercase().as_str()) {
        ("open", "RDP") => external::open_rdp(session, &host, &username, &state.settings.lock().unwrap().rdp_defaults.clone()),
        ("open", "VNC") => external::open_vnc(&host, port, &username),
        ("filezilla", _) => {
            // an SSH session's login as it connects in a tab (session, folders, profile); others as stored
            let launch = launch::launch(&sessions, &profiles, &values, &serial_devices, store, session).ok();
            let login = launch.as_ref().and_then(|l| l.login.clone());
            let (host, port, username, key_file) = match &login {
                Some(l) => (l.host.clone(), l.port, l.username.clone(), l.key_file.clone()),
                None => (host, port, username, String::new()),
            };
            // no username anywhere: the one ssh would use, as a tab connects with it
            let username = if username.is_empty() { external::ssh_username(&host, port).unwrap_or_default() } else { username };
            let password = if state.settings.lock().unwrap().filezilla_passwords { launch.and_then(|l| l.password) } else { None };
            let local = variables::substitute(session.get("LocalPath"), &values);
            let remote = variables::substitute(session.get("RemotePath"), &values);
            let opened = external::open_filezilla(&host, port, &username, password.as_deref(), &local, &remote)?;
            // FileZilla takes no key on its command line: it uses ssh-agent and its own list
            Ok(if key_file.is_empty() {
                opened
            } else {
                format!("{opened}. Its key {key_file} isn't passed to FileZilla: add it in FileZilla's Settings > SFTP, or run ssh-add {key_file}")
            })
        }
        ("terminal", _) => {
            let launch = launch::launch(&sessions, &profiles, &values, &serial_devices, store, session)?;
            if launch.stream.is_some() || launch.local {
                return Err("Only SSH sessions open in Terminal.".into());
            }
            let words: Vec<String> = launch.command.get_argv().iter().map(|w| w.to_string_lossy().to_string()).collect();
            let mut line = external::shell_line(&words);
            // the password, if any, goes to ssh through this app (askpass.rs), never on the command line
            if let Some(password) = launch.password.as_deref() {
                let program = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().to_string();
                let token = askpass::register(password)?;
                line = format!(
                    "SSH_ASKPASS={} SSH_ASKPASS_REQUIRE=force {}={token} {line}",
                    external::shell_line(&[program]),
                    askpass::TOKEN_VAR
                );
            }
            external::open_in_terminal(&line)?;
            Ok(format!("Opened {} in Terminal", session.get("SessionName")))
        }
        (_, proto) => Err(format!("{proto} sessions don't open in another app.")),
    }
}

/// How old a profile's key file is, in seconds; None when there's none (the profile editor's key fetch)
#[tauri::command]
fn profile_key_age(key_file: String) -> Option<u64> {
    launch::key_age(&key_file)
}

/// Save the Settings window's General tab; the settings folder is changed with `settings_set_folder`
#[tauri::command]
fn settings_save(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> Result<AppSettings, String> {
    let mut current = state.settings.lock().unwrap();
    let folder = current.settings_folder.clone();
    let before = current.clone();
    *current = AppSettings { settings_folder: folder, ..settings };
    current.icon_size = match current.icon_size {
        16 | 20 | 24 | 32 => current.icon_size,
        _ => 24,
    };
    current.tree_font_size = current.tree_font_size.clamp(10, 24);
    current.shortcuts = keys::cleaned(&current.shortcuts);
    current.command_history_days = current.command_history_days.min(command_history::MOST_DAYS);
    current.save(&state.user_folder)?;
    onepassword::set_enabled(current.one_password);
    // Settings > General > Command bar's days shortened (or 0): what they no longer allow goes now, not at next start
    if current.command_history_days < before.command_history_days {
        let _ = command_history::prune(&state.user_folder, current.command_history_days, command_history::now());
    }
    let changes = window_changes(&before, &current);
    // the menu bar's keys follow Settings > Shortcuts
    if changes.menu && let Ok(bar) = menu(&app, &current) {
        let _ = app.set_menu(bar);
    }
    if changes.theme {
        apply_window_theme(&app, &current.theme);
    }
    let saved = current.clone();
    drop(current);
    applog::info("Settings saved");
    // and the menu bar icon, Settings > General
    tray::refresh(&app);
    // every window reads them again, so another window's next save doesn't put back what it had
    let _ = app.emit(SETTINGS_CHANGED, ());
    Ok(saved)
}

/// What a settings save has to redo in the windows themselves (see [`window_changes`])
#[derive(Debug, PartialEq)]
struct WindowChanges {
    /// The menu bar again: its keys (Settings > Shortcuts), Local overrides, or what View ticks changed
    menu: bool,
    /// The menu bar, menus and title bars in the theme again: Settings > General > Theme changed
    theme: bool,
}

/// What a save from `before` to `after` changes in the windows. Each only when its own setting changed: on Windows,
/// setting the menu bar again lays the whole window out again, which showed as the window shifting at every save (a
/// checkbox in a terminal tab's menu saves too)
fn window_changes(before: &AppSettings, after: &AppSettings) -> WindowChanges {
    WindowChanges {
        menu: before.shortcuts != after.shortcuts
            || before.use_local_overrides != after.use_local_overrides
            || view_checks(before) != view_checks(after),
        theme: before.theme != after.theme,
    }
}

/// The View menu's items that are shown or hidden, each with whether it's shown now, for its tick (as a Mac app's
/// View menu, and SuperXPuTTY's, tick the panes and bars on screen). Each is a setting, saved when the item is chosen,
/// so the menu is made again then ([`window_changes`]) and the tick follows. Full Screen isn't among them: the Mac's
/// own View menu doesn't tick it either. Nor is Layouts…, which opens Settings > Layouts.
fn view_checks(settings: &AppSettings) -> [(&'static str, bool); 9] {
    [
        ("toggleSessionsPane", settings.show_sessions_pane),
        ("toggleSendCommands", settings.show_send_commands),
        ("toggleButtonBar", settings.show_button_bar),
        ("toggleStatusBar", settings.show_status_bar),
        ("toggleSessionsAutoHide", settings.sessions_auto_hide),
        ("toggleSessionDetail", settings.show_session_detail),
        ("toggleLogViewer", settings.show_log_viewer),
        ("toggleAlwaysOnTop", settings.always_on_top),
        ("toggleGridView", settings.grid_view),
    ]
}

/// The window's own themes (Settings > General > Look > Theme), from UiThemes.json in the user's folder
#[tauri::command]
fn ui_themes_load(state: State<'_, AppState>) -> Vec<ui_themes::UiTheme> {
    ui_themes::load(&state.user_folder)
}

/// Save the window's own themes, whole; what's saved (names trimmed), or why they can't be
#[tauri::command]
fn ui_themes_save(state: State<'_, AppState>, themes: Vec<ui_themes::UiTheme>) -> Result<Vec<ui_themes::UiTheme>, String> {
    ui_themes::save(&state.user_folder, &themes)
}

/// Settings > General's Check 1Password: whether its CLI works here, through the 1Password app (onepassword.rs).
/// async: it waits for the user to approve it in 1Password
#[tauri::command(async)]
fn onepassword_check() -> Result<String, String> {
    onepassword::check()
}

/// Use another settings folder (chosen in the UI), and remember it
#[tauri::command]
fn settings_set_folder(app: AppHandle, state: State<'_, AppState>, path: String) -> Result<(), String> {
    let mut settings = state.settings.lock().unwrap();
    settings.settings_folder = Some(PathBuf::from(path));
    settings.save(&state.user_folder)?;
    let _ = app.emit(SETTINGS_CHANGED, ());
    Ok(())
}

/// The event every window gets when the settings are saved
const SETTINGS_CHANGED: &str = "settings-changed";

/// SuperXTerminal > Third-Party Notices: the components it ships and their licences (scripts/notices.mjs), in the
/// system's text editor ([`Opening::AsText`]). The app has the file among its resources; a development build reads
/// the project's.
fn open_notices<R: Runtime>(app: &AppHandle<R>) {
    let bundled = app.path().resource_dir().map(|d| d.join("THIRD-PARTY-NOTICES.txt")).ok().filter(|p| p.exists());
    let path = bundled.unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../THIRD-PARTY-NOTICES.txt"));
    // on Windows, the app for .txt files
    #[cfg(windows)]
    let opened = opening::open_in_app(&path).map_err(std::io::Error::other);
    #[cfg(not(windows))]
    let opened = opening::open_command(&path, opening::Opening::AsText).status().map(|_| ());
    if let Err(e) = opened {
        applog::warn(format!("Couldn't open {}: {e}", path.display()));
    }
}

/// The menu bar: the app menu (with Settings..., Cmd+,), File (New Local Shell, Cmd+T), Edit (which WebKit needs for
/// Cmd+C/V), Tools (Environment Variables...) and Window.
/// SuperXPuTTY's File, View and Tools menus join it as their features arrive.
/// Its keys are the settings' (Settings > Shortcuts). Tools > Use Local Overrides is ticked while they're used, and
/// View's panes and bars while they're shown ([`view_checks`]): the menu is made again whenever one of those changes
/// ([`window_changes`]), so the ticks follow however it changed: the item itself, the status bar's "Local overrides
/// off", or a pane's own ×.
fn menu<R: Runtime>(app: &AppHandle<R>, settings: &AppSettings) -> tauri::Result<Menu<R>> {
    let shortcuts = &settings.shortcuts;
    let overrides_on = settings.use_local_overrides;
    // an action's item, with its key now (keys.rs: the user's, else the Mac default)
    let action = |id: &str| MenuItem::with_id(app, id, keys::label(id), true, keys::key(shortcuts, id).as_deref());
    let plain = |id: &str, label: &str| MenuItem::with_id(app, id, label, true, None::<&str>);
    // a View item that shows or hides something: ticked while it's shown, with its key as an action's
    let views = view_checks(settings);
    let shown = |id: &str| {
        let on = views.iter().any(|(view, on)| *view == id && *on);
        CheckMenuItem::with_id(app, id, keys::label(id), true, on, keys::key(shortcuts, id).as_deref())
    };
    let app_menu = Submenu::with_items(
        app,
        "SuperXTerminal",
        true,
        &[
            &PredefinedMenuItem::about(app, None, None)?,
            &action("checkUpdates")?,
            &plain("thirdPartyNotices", "Third-Party Notices")?,
            &PredefinedMenuItem::separator(app)?,
            &action("settings")?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::quit(app, None)?,
        ],
    )?;
    let file = Submenu::with_items(
        app,
        "File",
        true,
        &[
            &action("newWindow")?,
            &action("newLocalShell")?,
            // SuperXPuTTY's File > Open session… (its Quick Selector)
            &action("openSession")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXPuTTY's File > Import sessions, Export sessions and Export sessions to CSV, in its order
            &Submenu::with_items(
                app,
                "Import Sessions",
                true,
                &[
                    &plain("import:superputty", "From SuperPuTTY XML File…")?,
                    &plain("import:putty", if cfg!(windows) { "From PuTTY/KiTTY (Registry)…" } else { "From PuTTY (~/.putty)…" })?,
                    &plain("import:puttyFolder", "From PuTTY Portable Settings…")?,
                    &plain("import:puttycm", "From PuTTY CM Export…")?,
                    // Remote Desktop Connection's recent computers are Windows' only: shown, greyed, on the Mac
                    &MenuItem::with_id(app, "import:rdpRegistry", "From RDP (Registry)…", cfg!(windows), None::<&str>)?,
                    &plain("import:rdp", "From Remote Desktop (.rdp) Files…")?,
                    &plain("import:mobaxterm", "From MobaXterm File…")?,
                    // SuperXTerminal's own: SecureCRT's Sessions folder
                    &plain("import:securecrt", "From SecureCRT…")?,
                    &plain("import:csv", "From CSV File…")?,
                    &plain("import:ssh", "From ssh config…")?,
                ],
            )?,
            &plain("exportXml", "Export Sessions…")?,
            &plain("exportCsv", "Export Sessions to CSV…")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXTerminal's own: every session in one table, its columns filtered, to change many at once
            &plain("editSessionsTable", "Edit Sessions as Table…")?,
            // SuperXPuTTY's File > Edit sessions externally and Reload sessions file
            &plain("editSessionsExternally", "Edit Sessions Externally…")?,
            &plain("reloadSessions", "Reload Sessions File")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXPuTTY's File > Save layout and Save layout as...
            &action("saveLayout")?,
            &action("saveLayoutAs")?,
            &PredefinedMenuItem::separator(app)?,
            // Cmd+W closes the tab, as in Terminal and iTerm2; the window closes with its red button
            &action("closeTab")?,
        ],
    )?;
    let edit = Submenu::with_items(
        app,
        "Edit",
        true,
        &[
            &PredefinedMenuItem::undo(app, None)?,
            &PredefinedMenuItem::redo(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::cut(app, None)?,
            &PredefinedMenuItem::copy(app, None)?,
            &PredefinedMenuItem::paste(app, None)?,
            &PredefinedMenuItem::select_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            // the terminal's own Find (its Ctrl+Shift+F), Clear Scrollback, Reset and Copy All, with the Mac's keys
            &action("find")?,
            &action("copyAll")?,
            &action("clearScrollback")?,
            &action("resetTerminal")?,
        ],
    )?;
    let window = Submenu::with_items(
        app,
        "Window",
        true,
        &[
            &PredefinedMenuItem::minimize(app, None)?,
            &PredefinedMenuItem::maximize(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &action("nextTab")?,
            &action("prevTab")?,
            // SuperXPuTTY's Switch session…: the Quick Selector over the open tabs
            &action("switchSession")?,
            &PredefinedMenuItem::separator(app)?,
            &action("duplicateSession")?,
            &action("renameTab")?,
            &action("restartSession")?,
            &PredefinedMenuItem::separator(app)?,
            &action("splitRight")?,
            &action("splitDown")?,
            &action("joinAll")?,
            &PredefinedMenuItem::separator(app)?,
            &action("gotoCommandBar")?,
            &action("gotoConnectionBar")?,
            &action("focusActiveSession")?,
        ],
    )?;
    let tools = Submenu::with_items(
        app,
        "Tools",
        true,
        &[
            &action("scriptEditor")?,
            &PredefinedMenuItem::separator(app)?,
            &plain("databaseSessions", "Database Sessions…")?,
            &plain("environmentVariables", "Environment Variables…")?,
            &plain("environmentColours", "Environment Colours…")?,
            &PredefinedMenuItem::separator(app)?,
            // Settings' tabs in its order, as the app menu's Settings… opens them: "settings:" and the tab's name (Profiles
            // Settings… opens on Default Settings)
            &plain("settings:General", "General Settings…")?,
            &plain("profiles", "Profiles Settings…")?,
            &plain("settings:Terminal", "Terminal Settings…")?,
            &plain("settings:Remote Desktop", "Remote Desktop Settings…")?,
            &plain("settings:Shortcuts", "Shortcut Settings…")?,
            &plain("settings:Layouts", "Layout Settings…")?,
            &PredefinedMenuItem::separator(app)?,
            &CheckMenuItem::with_id(app, "toggleLocalOverrides", "Use Local Overrides", true, overrides_on, None::<&str>)?,
        ],
    )?;
    // shown or hidden, as SuperXPuTTY's View > Send Commands and View > Button Bar
    let view = Submenu::with_items(
        app,
        "View",
        true,
        &[
            &shown("toggleSessionsPane")?,
            &shown("toggleSendCommands")?,
            &shown("toggleButtonBar")?,
            &shown("toggleStatusBar")?,
            &PredefinedMenuItem::separator(app)?,
            &shown("toggleSessionsAutoHide")?,
            // the saved layouts, in Settings > Layouts
            &action("toggleLayouts")?,
            &shown("toggleSessionDetail")?,
            &shown("toggleLogViewer")?,
            &PredefinedMenuItem::separator(app)?,
            // SuperXTerminal's own: the tabs tiled, to watch many sessions at once
            &shown("toggleGridView")?,
            &shown("toggleAlwaysOnTop")?,
            &action("fullScreen")?,
        ],
    )?;
    Menu::with_items(app, &[&app_menu, &file, &edit, &view, &tools, &window])
}

/// The Shortcuts editor's rows: every action, its default and its key now
#[tauri::command]
fn shortcuts_keys(state: State<'_, AppState>) -> Vec<keys::Row> {
    keys::rows(&state.settings.lock().unwrap().shortcuts)
}

/// The keys this system keeps (the Mac's Cmd+Q, Cmd+C...; Windows' Alt+F4 and the terminal's Ctrl+Shift+C/V), which
/// the Shortcuts editor won't give to an action
#[tauri::command]
fn shortcuts_reserved() -> Vec<&'static str> {
    keys::reserved().to_vec()
}

/// What a new window starts with: a session to open ("store:id", Connect in new window), or a running tab to take over
/// (Move to New Window: its session's pty id, title and how its tab looks, as the window that had it describes it)
#[derive(Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
struct WindowStart {
    open: Option<String>,
    adopt: Option<serde_json::Value>,
    /// Load in New Window: a layout to open
    layout: Option<String>,
}

/// The layouts' names, "<Auto Restore>" first
#[tauri::command]
fn layouts_list(state: State<'_, AppState>) -> Vec<String> {
    layouts::list(&shared_folder(&state))
}

/// A layout's text; None when it hasn't been saved
#[tauri::command]
fn layout_read(state: State<'_, AppState>, name: String) -> Option<String> {
    layouts::read(&shared_folder(&state), &state.user_folder, &name)
}

/// Save a layout
#[tauri::command]
fn layout_save(state: State<'_, AppState>, name: String, text: String) -> Result<(), String> {
    layouts::save(&shared_folder(&state), &state.user_folder, &name, &text)
}

/// Delete a saved layout
#[tauri::command]
fn layout_delete(state: State<'_, AppState>, name: String) -> Result<(), String> {
    layouts::delete(&shared_folder(&state), &name)
}

/// Rename a saved layout
#[tauri::command]
fn layout_rename(state: State<'_, AppState>, from: String, to: String) -> Result<(), String> {
    layouts::rename(&shared_folder(&state), &from, &to)
}

/// What's been asked to open from outside (the command line, a second start, a link), waiting for the window in
/// front to take it (`requests_take`)
static PENDING: Mutex<Vec<cli::Request>> = Mutex::new(Vec::new());

/// The menu bar icon's items ("tray:..."): a session opens in the window in front, or in a new one when none is open
fn tray_command(app: &AppHandle, id: &str) {
    let has_window = !app.webview_windows().is_empty();
    if let Some(rest) = id.strip_prefix("tray:open:") {
        // "<store>:<id>"; Database sessions' ids start "@db/" already
        let session = rest.split_once(':').map(|(_, id)| id).unwrap_or(rest);
        deliver(app, Some(cli::Request::Session { id: session.to_string() }));
        return;
    }
    match id {
        "tray:quit" => app.exit(0),
        // a new window starts with a Local Shell
        "tray:newLocalShell" | "tray:openSession" | "tray:newWindow" if !has_window => deliver(app, None),
        "tray:newWindow" => {
            let _ = new_window(app, None, None, None);
        }
        "tray:newLocalShell" | "tray:openSession" => {
            deliver(app, None);
            let front = FRONT_WINDOW.lock().unwrap().clone().unwrap_or_else(|| "main".into());
            let _ = app.emit_to(front.as_str(), "menu", id.trim_start_matches("tray:"));
        }
        _ => deliver(app, None),
    }
}

/// Something to open from outside: kept for the window in front, which is told and brought forward; with no window
/// open (SuperXTerminal kept in the menu bar), a new one, which takes it as it starts
fn deliver(app: &AppHandle, request: Option<cli::Request>) {
    if app.webview_windows().is_empty() {
        if let Some(request) = request {
            PENDING.lock().unwrap().push(request);
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let _ = main_window(&handle);
        });
        return;
    }
    let front = FRONT_WINDOW.lock().unwrap().clone().filter(|label| app.get_webview_window(label).is_some()).unwrap_or_else(|| "main".into());
    if let Some(request) = request {
        PENDING.lock().unwrap().push(request);
        let _ = app.emit_to(front.as_str(), "open-request", ());
    }
    if let Some(window) = app.get_webview_window(&front) {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// What's waiting to open, taken once
#[tauri::command]
fn requests_take() -> Vec<cli::Request> {
    std::mem::take(&mut *PENDING.lock().unwrap())
}

/// Settings > General: SuperXTerminal opens ssh:// and telnet:// links (Terminal does until then), as LaunchServices
/// records it for the user
#[tauri::command]
fn links_make_default(app: AppHandle) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::NSString;
        #[link(name = "CoreServices", kind = "framework")]
        unsafe extern "C" {
            /// LaunchServices' call to make an app the default for a URL scheme; 0 when it worked
            fn LSSetDefaultHandlerForURLScheme(scheme: *const std::ffi::c_void, bundle: *const std::ffi::c_void) -> i32;
        }
        let bundle = NSString::from_str(&app.config().identifier);
        for scheme in ["ssh", "telnet"] {
            let scheme = NSString::from_str(scheme);
            // NSString is toll-free bridged to the CFString LaunchServices takes
            let status = unsafe {
                LSSetDefaultHandlerForURLScheme(&*scheme as *const NSString as *const _, &*bundle as *const NSString as *const _)
            };
            if status != 0 {
                return Err(format!("macOS refused (error {status}). The app may need to be in Applications first."));
            }
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = app;
        Err("Only on the Mac.".into())
    }
}

/// What each new window starts with, by its label; taken once by `window_start`
static WINDOW_OPENS: Mutex<Option<std::collections::HashMap<String, WindowStart>>> = Mutex::new(None);
/// The window last brought to the front, which the menu bar's commands go to
static FRONT_WINDOW: Mutex<Option<String>> = Mutex::new(None);
/// The number of the next window ("window-2" ...)
static NEXT_WINDOW: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(2);

/// File > New Window, and Connect in new window (`open` the session, "store:id"), as SuperXPuTTY's new instance: another
/// window with its own Sessions tree and tabs
///
/// Async, so it runs off the main thread. On Windows Tauri runs a plain command on the main thread, inside WebView2's
/// handling of the page's call, and `run_on_main_thread` from there makes the window at once, in that handler: WebView2
/// never finished the new web view, which stayed blank (about:blank, the window's grey), and the page that asked never
/// heard back. Move to New Window, New Window and Connect in new window all did this on Windows; the Mac didn't mind.
#[tauri::command]
async fn window_new(app: AppHandle, open: Option<String>, adopt: Option<serde_json::Value>, layout: Option<String>) -> Result<(), String> {
    new_window(&app, open, adopt, layout)
}

/// Make another window, starting with `open`, `adopt` or `layout` (see [`WindowStart`]), on the main thread. Never
/// from a plain (sync) command: see [`window_new`].
fn new_window(app: &AppHandle, open: Option<String>, adopt: Option<serde_json::Value>, layout: Option<String>) -> Result<(), String> {
    let label = format!("window-{}", NEXT_WINDOW.fetch_add(1, std::sync::atomic::Ordering::Relaxed));
    if open.is_some() || adopt.is_some() || layout.is_some() {
        WINDOW_OPENS.lock().unwrap().get_or_insert_with(Default::default).insert(label.clone(), WindowStart { open, adopt, layout });
    }
    let handle = app.clone();
    // windows are made on the main thread (the web view's settings need it)
    app.run_on_main_thread(move || {
        // down and to the right of the window in front, as a Mac app cascades new windows
        let front_label = FRONT_WINDOW.lock().unwrap().clone().unwrap_or_else(|| "main".into());
        let front = handle.get_webview_window(&front_label).or_else(|| handle.get_webview_window("main"));
        let at = front.and_then(|w| Some(w.outer_position().ok()?.to_logical::<f64>(w.scale_factor().ok()?))).map(|p| (p.x + 28.0, p.y + 28.0));
        let _ = window(&handle, &label, at);
        // in front at once, so the menu's next command (Cmd+T) goes to it
        *FRONT_WINDOW.lock().unwrap() = Some(label.clone());
        if let Some(new) = handle.get_webview_window(&label) {
            let _ = new.set_focus();
        }
    })
    .map_err(|e| e.to_string())
}

/// What a window starts with: a session to open or a tab to take over; None for a Local Shell
#[tauri::command]
fn window_start(window: tauri::WebviewWindow) -> Option<WindowStart> {
    WINDOW_OPENS.lock().unwrap().as_mut().and_then(|opens| opens.remove(window.label()))
}

/// The main window, made here rather than in tauri.conf.json so its web view can have Writing Tools off: macOS put
/// its "Write with Siri" badge by the terminal's cursor, and offered to rewrite what's typed into a session
fn main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    window(app, "main", None)
}

/// A window's own light or dark for the app's theme (Settings > General > Theme); None follows the system. On
/// Windows it's what Windows draws the window's menu bar and title bar in, which otherwise follow the system whatever
/// the app's theme; on the Mac the menu bar is the system's, so windows are left to it.
#[cfg(windows)]
fn window_theme(theme: &str) -> Option<tauri::Theme> {
    match theme {
        "light" => Some(tauri::Theme::Light),
        "dark" => Some(tauri::Theme::Dark),
        _ => None,
    }
}

/// Windows: the menus that drop down from the menu bar (and right-click menus) in the app's theme. They aren't the
/// window's: Windows draws every popup menu in the app's one preferred mode, which Tauri leaves following the system,
/// so a Light app had dark menus on a dark system. uxtheme's undocumented SetPreferredAppMode (ordinal 135, Windows 10
/// 1903 and later) forces it, and FlushMenuThemes (136) has menus already made redrawn in it.
#[cfg(windows)]
fn apply_menu_theme(theme: &str) {
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    // PreferredAppMode: AllowDark (follow the system), ForceDark, ForceLight
    let mode: i32 = match theme {
        "light" => 3,
        "dark" => 2,
        _ => 1,
    };
    let name: Vec<u16> = "uxtheme.dll\0".encode_utf16().collect();
    unsafe {
        let uxtheme = LoadLibraryW(name.as_ptr());
        if uxtheme.is_null() {
            return;
        }
        if let Some(set_preferred_app_mode) = GetProcAddress(uxtheme, 135 as *const u8) {
            let set_preferred_app_mode: unsafe extern "system" fn(i32) -> i32 = std::mem::transmute(set_preferred_app_mode);
            set_preferred_app_mode(mode);
        }
        if let Some(flush_menu_themes) = GetProcAddress(uxtheme, 136 as *const u8) {
            let flush_menu_themes: unsafe extern "system" fn() = std::mem::transmute(flush_menu_themes);
            flush_menu_themes();
        }
    }
}

/// Windows: every window's menu bar, menus and title bar in the app's theme (on the Mac nothing changes)
fn apply_window_theme(app: &AppHandle, theme: &str) {
    #[cfg(windows)]
    {
        apply_menu_theme(theme);
        for window in app.webview_windows().values() {
            let _ = window.set_theme(window_theme(theme));
        }
    }
    #[cfg(not(windows))]
    let _ = (app, theme);
}

/// A window's colour before its page has drawn: the page's own window colour (app.css's --bg-window), light only when
/// Settings > General > Theme is Light. Following the system starts dark too, so a dark system never sees a white
/// flash; a light one sees dark for a moment instead, as the page follows the system once it's loaded.
fn loading_colour(theme: &str) -> tauri::window::Color {
    if theme == "light" { tauri::window::Color(0xf5, 0xf5, 0xf5, 0xff) } else { tauri::window::Color(0x2d, 0x2d, 0x30, 0xff) }
}

/// Whether a window opens where it was last left: the main window only, as the one SuperXTerminal starts with; New
/// Window's windows each start down and to the right of the one in front, as a Mac app cascades them
fn remembers_place(label: &str) -> bool {
    label == "main" && !demo::on()
}

/// What's remembered of the main window (tauri-plugin-window-state, in the app's config folder as .window-state.json):
/// its position and size, and whether it was maximized or full screen. Not whether it was shown, since it's always
/// shown at the start, nor its frame, which is always the system's.
fn remembered_state() -> tauri_plugin_window_state::StateFlags {
    use tauri_plugin_window_state::StateFlags;
    StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED | StateFlags::FULLSCREEN
}

/// A SuperXTerminal window: the first ("main"), or another made by New Window ("window-2", ...) at `at` (logical)
fn window(app: &tauri::AppHandle, label: &str, at: Option<(f64, f64)>) -> tauri::Result<()> {
    let builder = tauri::WebviewWindowBuilder::new(app, label, tauri::WebviewUrl::default())
        .title("SuperXTerminal")
        .inner_size(1400.0, 900.0)
        .min_inner_size(800.0, 500.0)
        .disable_drag_drop_handler();
    let builder = match at {
        Some((x, y)) => builder.position(x, y),
        None => builder,
    };
    #[cfg(target_os = "macos")]
    let builder = builder.with_webview_configuration(writing_tools_off());
    let theme = app.try_state::<AppState>().map(|s| s.settings.lock().unwrap().theme.clone()).unwrap_or_default();
    // the window's own colour while the page loads, which was white whatever the theme, a flash in a dark room
    let builder = builder.background_color(loading_colour(&theme));
    // Windows: its menu bar, menus and title bar in the app's theme from the start
    #[cfg(windows)]
    let builder = {
        apply_menu_theme(&theme);
        builder.theme(window_theme(&theme))
    };
    builder.build()?;
    Ok(())
}

/// A WebKit configuration with Writing Tools off (writingToolsBehavior = none, macOS 15 and later; older macOS has
/// none to turn off). Set by key, as objc2-web-kit has no binding for it yet.
#[cfg(target_os = "macos")]
fn writing_tools_off() -> objc2::rc::Retained<objc2_web_kit::WKWebViewConfiguration> {
    use objc2::{sel, MainThreadMarker};
    use objc2_foundation::{ns_string, NSNumber, NSObjectNSKeyValueCoding, NSObjectProtocol};
    let mtm = MainThreadMarker::new().expect("the window is made on the main thread");
    let config = unsafe { objc2_web_kit::WKWebViewConfiguration::new(mtm) };
    if config.respondsToSelector(sel!(setWritingToolsBehavior:)) {
        // NSWritingToolsBehaviorNone
        unsafe { config.setValue_forKey(Some(&NSNumber::new_isize(-1)), ns_string!("writingToolsBehavior")) };
    }
    config
}

/// Keep macOS 27's "Write with Siri" badge (AppKit's "Campo" lightweight UI) away from the terminal. AppKit shows it by
/// the cursor of any text field it finds eligible, and the terminal's hidden input (xterm.js types through it, under
/// its own drawn cursor) is one; what's typed there goes to a session, so it isn't writing to help with. There's no
/// public switch and WebKit passes no field setting for it, so in this app NSTextInputContext finds no field eligible
/// and shows no affordance (the eligibility check alone wasn't enough). Nothing changes where these private methods
/// don't exist (older macOS, or a later one that renames them).
#[cfg(target_os = "macos")]
fn no_siri_badge() {
    use objc2::ffi::{class_getInstanceMethod, method_setImplementation};
    use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
    use objc2::sel;
    extern "C-unwind" fn not_eligible(_this: *mut AnyObject, _cmd: Sel) -> Bool {
        Bool::NO
    }
    extern "C-unwind" fn show_nothing(_this: *mut AnyObject, _cmd: Sel, _mode: isize) {}
    let Some(class) = AnyClass::get(c"NSTextInputContext") else { return };
    unsafe {
        let eligible = class_getInstanceMethod(class, sel!(_isCampoLightweightUIEligible));
        if !eligible.is_null() {
            let imp: Imp = std::mem::transmute(not_eligible as extern "C-unwind" fn(*mut AnyObject, Sel) -> Bool);
            method_setImplementation(eligible as *mut _, imp);
        }
        let show = class_getInstanceMethod(class, sel!(_showLightweightAffordanceWithMode:));
        if !show.is_null() {
            let imp: Imp = std::mem::transmute(show_nothing as extern "C-unwind" fn(*mut AnyObject, Sel, isize));
            method_setImplementation(show as *mut _, imp);
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// Run as ssh's askpass for Connect external (`askpass.rs`) instead of as the app; true when it did
pub fn askpass_helper() -> bool {
    askpass::helper()
}

/// Run as one of demo mode's stand-in devices (`demo_device.rs`) instead of as the app; true when it did
pub fn demo_device() -> bool {
    demo_device::run()
}

/// Start the app: load the settings, open the main window, and handle the command line SuperXPuTTY takes
pub fn run() {
    // demo mode: a fresh folder of made-up sessions in place of the user's own (demo.rs)
    if demo::on() {
        if let Err(e) = demo::prepare() {
            applog::warn(format!("The demo's folder couldn't be made: {e}"));
        }
    }
    let user_folder = sessions::user_folder();
    let settings = AppSettings::load(&user_folder);
    onepassword::set_enabled(settings.one_password);
    let profiles = Profiles::load(&user_folder);
    applog::info(format!("SuperXTerminal {} started (user folder {})", env!("CARGO_PKG_VERSION"), user_folder.display()));
    // started with something to open (SuperXPuTTY's command line)
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(request) = cli::from_args(&args) {
        PENDING.lock().unwrap().push(request);
    }
    // No raw device events (Windows; the setting does nothing elsewhere): the window toolkit (tao) otherwise registers
    // the app for raw keyboard and mouse input, to report device events nothing here listens to, and Windows doesn't
    // call a low-level keyboard hook of a program registered so. A Remote Desktop tab's hook for the Windows key
    // (rdp_win_keys.rs) then went in without an error and never heard a key: Win+R stayed on this computer.
    let builder = tauri::Builder::default().device_event_filter(tauri::DeviceEventFilter::Always);
    // one SuperXTerminal: started again (from a script, a terminal), the first one opens what it was given. Not for a
    // demo, which would otherwise only bring a SuperXTerminal already running to the front
    let builder = if demo::on() {
        builder
    } else {
        builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| deliver(app, cli::from_args(argv.get(1..).unwrap_or(&[])))))
    };
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // the main window opens where it was last, its size, maximized or full screen as it was left
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(remembered_state())
                .with_filter(remembers_place)
                .build(),
        )
        .menu({
            let settings = settings.clone();
            move |app| menu(app, &settings)
        })
        // menu items the UI handles arrive there as a "menu" event with the item's id
        .on_menu_event(|app, event| {
            if event.id().0.starts_with("tray:") {
                tray_command(app, event.id().0.as_str());
                return;
            }
            if event.id().0 == "thirdPartyNotices" {
                open_notices(app);
                return;
            }
            // to the window in front only: with several windows, Cmd+T opens one shell, in that window
            let front = FRONT_WINDOW.lock().unwrap().clone().filter(|label| app.get_webview_window(label).is_some());
            let _ = app.emit_to(front.as_deref().unwrap_or("main"), "menu", event.id().0.as_str());
        })
        // a window's tabs go with it: their shells and connections end
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(true) => *FRONT_WINDOW.lock().unwrap() = Some(window.label().to_string()),
            tauri::WindowEvent::Destroyed => {
                applog::info(format!("Window {} closed; its sessions end", window.label()));
                // kept now, not only when the app quits: it may stay in the menu bar or tray, and be ended there
                if remembers_place(window.label()) {
                    use tauri_plugin_window_state::AppHandleExt;
                    let _ = window.app_handle().save_window_state(remembered_state());
                }
                pty::close_window(&window.state::<pty::Ptys>(), window.label());
                rdp::close_window(&window.state::<rdp::Rdps>(), window.label());
                sftp::close_window(&window.state::<sftp::Sftps>(), window.label())
            }
            _ => {}
        })
        .setup(|app| {
            console::allow_ctrl_c();
            logs::sweep_temp();
            // pins left by an older MSI show a blank file once their icon has gone (pins.rs). Not for a demo, which
            // may be a development build: its pins are the installed app's
            #[cfg(windows)]
            if !demo::on() {
                pins::repair();
            }
            #[cfg(target_os = "macos")]
            no_siri_badge();
            main_window(app.handle())?;
            tray::refresh(app.handle());
            command_tap::install(app.handle());
            Ok(())
        })
        .manage(pty::Ptys::default())
        .manage(rdp::Rdps::default())
        .manage(sftp::Sftps::default())
        .manage(logs::Logs::default())
        .manage(rhai_scripts::RhaiRuns::default())
        .manage(server_load::ServerLoads::default())
        .manage(AppState {
            user_folder,
            settings: Mutex::new(settings),
            sessions: Mutex::new(Sessions::default()),
            profiles: Mutex::new(profiles),
            database: Mutex::new(DbState::default()),
            db_server: Mutex::new(Vec::new()),
            files_seen: Mutex::new(file_watch::Seen::nothing()),
            files_read: Mutex::new(None),
        })
        .invoke_handler(tauri::generate_handler![
            commands::sessions::sessions_load,
            commands::sessions::sessions_shown,
            commands::files::open_link,
            settings_get,
            settings_set_folder,
            commands::files::folder_writable,
            commands::files::folder_exists,
            settings_save,
            icons_custom,
            highlighting_load,
            commands::session_editing::session_get,
            commands::session_editing::session_password_has,
            commands::session_editing::session_password_set,
            commands::session_editing::session_password_from_pw,
            commands::session_editing::session_save,
            commands::session_editing::session_delete,
            commands::session_editing::session_move,
            commands::session_editing::folder_move,
            commands::session_editing::items_move,
            commands::session_editing::items_delete,
            commands::session_editing::folder_delete,
            commands::session_editing::folder_settings_save,
            commands::variables::variables_list,
            commands::database::db_settings_get,
            commands::database::db_settings_save,
            commands::database::db_test,
            commands::database::db_refresh,
            commands::database::db_cached,
            commands::database::db_check,
            commands::database::db_export,
            commands::database::db_import,
            commands::database::db_session_save,
            commands::database::db_session_delete,
            commands::database::db_session_move,
            commands::database::db_folder_move,
            commands::database::db_folder_delete,
            commands::database::db_folder_settings_save,
            commands::transfer::sessions_transfer,
            commands::database::db_create_schema,
            commands::database::db_pending_list,
            commands::shortcuts::shortcuts_get,
            commands::overrides::serial_devices_list,
            commands::sessions::sessions_files_changed,
            session_external,
            commands::overrides::serial_device_set,
            pty::pty_special,
            commands::overrides::overrides_get,
            commands::transfer::sessions_bulk_edit,
            commands::sessions_table::sessions_table_load,
            commands::sessions_table::sessions_table_save,
            commands::sessions_table::sessions_table_export_csv,
            commands::sessions_table::sessions_table_read_csv,
            commands::session_editing::session_passwords_set,
            commands::importing::sessions_export_csv,
            commands::importing::sessions_csv_template,
            onepassword_check,
            ui_themes_load,
            ui_themes_save,
            window_new,
            layouts_list,
            commands::logs::applog_since,
            commands::logs::applog_write,
            commands::logs::applog_clear,
            commands::logs::applog_reveal,
            commands::session_files::sessions_reveal,
            commands::session_files::sessions_backups,
            commands::session_files::sessions_restore,
            commands::sending::command_history_load,
            commands::sending::command_history_add,
            requests_take,
            links_make_default,
            shortcuts_keys,
            shortcuts_reserved,
            layout_read,
            layout_save,
            layout_delete,
            layout_rename,
            window_start,
            commands::importing::sessions_import_plan,
            commands::importing::sessions_import_apply,
            commands::importing::ssh_config_path,
            commands::importing::sessions_import_source,
            ssh_algorithms_available,
            commands::session_files::sessions_export_xml,
            commands::session_files::sessions_edit_externally,
            commands::overrides::overrides_set,
            commands::overrides::override_presets_get,
            profile_key_age,
            commands::variables::variables_usage,
            commands::variables::profiles_missing,
            commands::variables::variables_built_in,
            commands::overrides::override_presets_set,
            pty::pty_serial_get,
            pty::pty_serial_set,
            commands::logs::log_open,
            commands::logs::log_write,
            commands::logs::log_close,
            commands::logs::log_read,
            commands::logs::log_chunk,
            highlighting_user_folder,
            commands::files::open_path,
            commands::sending::buttonbars_load,
            commands::sending::buttonbars_save,
            commands::sending::buttonbars_export,
            commands::sending::buttonbars_import,
            commands::scripts::scripts_list,
            commands::scripts::script_chosen,
            commands::scripts::script_copy_in,
            commands::scripts::session_script,
            commands::scripts::quick_connect_script,
            commands::scripts::script_load,
            commands::scripts::script_save,
            commands::scripts::script_delete,
            commands::files::text_file_read,
            commands::files::text_file_write,
            commands::scripts::script_output_save,
            rhai_scripts::rhai_run,
            rhai_scripts::rhai_reply,
            rhai_scripts::rhai_stop,
            rhai_scripts::rhai_check,
            server_load::server_load_watch,
            server_load::server_load_stop,
            commands::files::dropped_file_save,
            commands::shortcuts::shortcuts_favorite,
            commands::shortcuts::shortcuts_opened,
            commands::shortcuts::shortcuts_forget_recent,
            commands::shortcuts::shortcuts_expanded,
            commands::database::db_status,
            commands::database::db_pending_retry,
            commands::database::db_pending_discard,
            commands::variables::variables_save,
            commands::variables::variables_fill,
            commands::profiles::profiles_list,
            commands::profiles::profiles_export,
            commands::profiles::profiles_read_file,
            commands::profiles::profile_templates,
            commands::profiles::profiles_read_putty,
            commands::profiles::profile_template_share,
            commands::profiles::profiles_save,
            commands::profiles::profiles_delete,
            clipboard::clipboard_text,
            pty::pty_open,
            pty::pty_write,
            pty::pty_resize,
            pty::pty_close,
            sftp::sftp_open,
            sftp::sftp_close,
            sftp::sftp_answer,
            sftp::sftp_list,
            sftp::sftp_mkdir,
            sftp::sftp_rename,
            sftp::sftp_delete,
            sftp::sftp_cancel,
            sftp::sftp_transfer,
            sftp::local_list,
            sftp::local_mkdir,
            sftp::local_rename,
            sftp::local_reveal,
            rdp::rdp_open,
            rdp::rdp_key,
            rdp::rdp_keys,
            rdp::rdp_grab_keys,
            rdp::rdp_text,
            rdp::rdp_sync,
            rdp::rdp_mouse,
            rdp::rdp_resize,
            rdp::rdp_close,
            rdp::rdp_clipboard_announce,
            pty::pty_attach
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app, event| {
            match &event {
                // a link macOS hands over (superxterminal://, ssh://, telnet://)
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Opened { urls } => {
                    for url in urls {
                        deliver(app, cli::from_url(url.as_str()));
                    }
                }
                // the last window closed: with the menu bar icon, SuperXTerminal keeps running there
                tauri::RunEvent::ExitRequested { api, code: None, .. } => {
                    if app.state::<AppState>().settings.lock().unwrap().show_in_menu_bar {
                        api.prevent_exit();
                    }
                }
                // the Dock icon clicked with no window open: a new one
                #[cfg(target_os = "macos")]
                tauri::RunEvent::Reopen { has_visible_windows: false, .. } => deliver(app, None),
                _ => {}
            }
        });
}

#[cfg(test)]
pub(crate) mod tests {
    /// An app's state with these settings, the user's folder a temporary one
    pub(crate) fn state_with(settings: crate::settings::AppSettings) -> super::AppState {
        use std::sync::Mutex;
        super::AppState {
            user_folder: std::env::temp_dir().join("sxt-state-test"),
            settings: Mutex::new(settings),
            sessions: Mutex::new(Default::default()),
            profiles: Mutex::new(Default::default()),
            database: Mutex::new(Default::default()),
            db_server: Mutex::new(Vec::new()),
            files_seen: Mutex::new(crate::file_watch::Seen::nothing()),
            files_read: Mutex::new(None),
        }
    }

    use super::{loading_colour, remembered_state, remembers_place, view_checks, window_changes, WindowChanges};
    use crate::settings::AppSettings;
    use tauri_plugin_window_state::StateFlags;

    #[test]
    fn a_window_loads_dark_unless_the_theme_is_light() {
        use tauri::window::Color;
        assert_eq!(loading_colour("light"), Color(0xf5, 0xf5, 0xf5, 0xff));
        assert_eq!(loading_colour("dark"), Color(0x2d, 0x2d, 0x30, 0xff));
        assert_eq!(loading_colour("system"), Color(0x2d, 0x2d, 0x30, 0xff));
    }

    #[test]
    fn the_view_menu_ticks_what_is_shown_and_follows_each_change() {
        let mut settings = AppSettings::default();
        settings.show_sessions_pane = true;
        settings.show_send_commands = false;
        settings.show_button_bar = true;
        settings.show_status_bar = true;
        settings.sessions_auto_hide = false;
        settings.show_session_detail = true;
        settings.show_log_viewer = false;
        settings.always_on_top = true;
        assert_eq!(
            view_checks(&settings),
            [
                ("toggleSessionsPane", true),
                ("toggleSendCommands", false),
                ("toggleButtonBar", true),
                ("toggleStatusBar", true),
                ("toggleSessionsAutoHide", false),
                ("toggleSessionDetail", true),
                ("toggleLogViewer", false),
                ("toggleAlwaysOnTop", true),
                ("toggleGridView", false),
            ]
        );
        // each shown or hidden makes the menu again, so its tick follows
        let mut log = settings.clone();
        log.show_log_viewer = true;
        assert_eq!(window_changes(&settings, &log), WindowChanges { menu: true, theme: false });
        let mut top = settings.clone();
        top.always_on_top = false;
        assert_eq!(window_changes(&settings, &top), WindowChanges { menu: true, theme: false });
        let mut grid = settings.clone();
        grid.grid_view = true;
        assert_eq!(window_changes(&settings, &grid), WindowChanges { menu: true, theme: false });
    }

    #[test]
    fn a_terminal_preference_saved_leaves_the_menu_bar_and_theme_alone() {
        // a checkbox in a terminal tab's menu saves the settings; setting the menu bar again shifted the whole window
        let before = AppSettings::default();
        let mut after = before.clone();
        after.terminal_prefs.insert("copyOnSelect".into(), "false".into());
        assert_eq!(window_changes(&before, &after), WindowChanges { menu: false, theme: false });
        let mut keys = before.clone();
        keys.shortcuts.insert("newTab".into(), "Ctrl+T".into());
        assert_eq!(window_changes(&before, &keys), WindowChanges { menu: true, theme: false });
        let mut overrides = before.clone();
        overrides.use_local_overrides = !before.use_local_overrides;
        assert_eq!(window_changes(&before, &overrides), WindowChanges { menu: true, theme: false });
        let mut theme = before.clone();
        theme.theme = "light".into();
        assert_eq!(window_changes(&before, &theme), WindowChanges { menu: false, theme: true });
    }

    #[test]
    fn only_the_main_window_opens_where_it_was_left() {
        assert!(remembers_place("main"));
        // New Window's windows cascade from the one in front instead
        assert!(!remembers_place("window-2"));
    }

    #[test]
    fn the_main_windows_place_size_maximized_and_full_screen_are_remembered_but_not_whether_it_was_shown() {
        let state = remembered_state();
        assert!(state.contains(StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED | StateFlags::FULLSCREEN));
        assert!(!state.intersects(StateFlags::VISIBLE | StateFlags::DECORATIONS));
    }

    #[test]
    fn a_window_the_page_asks_for_is_made_by_an_async_command_so_windows_draws_it() {
        // Move to New Window (and New Window, Connect in new window) opened a grey window on Windows that never loaded,
        // and the window that asked stopped hearing back: a plain command runs on the main thread inside WebView2's
        // handling of the page's call, and a web view made there never finishes. Nothing here can make a WebView2
        // window (the app's windows aren't tested whole: docs/TESTING.md), so this reads the commands themselves: each
        // that makes a window must be async, which Tauri runs off the main thread.
        // as one kind of line ending, whichever Git checked out
        let source = include_str!("lib.rs").replace("\r\n", "\n");
        // the attribute in two halves, so this test's own text isn't taken for a command
        let commands = source.split(concat!("#[tauri", "::command]")).skip(1);
        let mut checked = 0;
        for command in commands {
            let signature = command.trim_start().lines().next().unwrap_or("");
            // the command's body: up to the first line that closes an item
            let body = command.split("\n}\n").next().unwrap_or("");
            let makes_window = ["new_window(", "main_window(", "WebviewWindowBuilder"].iter().any(|call| body.contains(call));
            if makes_window {
                checked += 1;
                assert!(signature.contains("async fn"), "a command that makes a window isn't async, so it hangs on Windows: {signature}");
            }
        }
        assert!(checked > 0, "found no command that makes a window: has window_new moved?");
    }

}
