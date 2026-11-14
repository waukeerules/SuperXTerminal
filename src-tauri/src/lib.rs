//! SuperXTerminal's Rust core: what every command shares ([`AppState`]) and the app's start ([`run`]), which
//! loads the settings, registers the commands, builds the menu bar and opens the main window. The commands the UI
//! (`src/`) calls through Tauri's `invoke` are in `commands/`, a module for each area of the app; the menu bar is
//! `menu.rs`, the windows `windows.rs`, and what's asked of the app from outside `requests.rs`. Code that isn't
//! the app's set-up belongs in one of those, or a module of its own, not here.

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
mod menu;
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
mod requests;
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
mod windows;
mod settings;
mod saving;
mod sftp;

use std::path::PathBuf;
use std::sync::Mutex;

use tauri::{Emitter, Manager};

use commands::database::DbState;
use commands::sessions::FilesRead;
use menu::{menu, open_notices};
use profiles::Profiles;
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
    /// What the last read of the session files said; None until they've been read ([`commands::sessions::sessions_shown`])
    files_read: Mutex<Option<FilesRead>>,
}

/// The settings folder, or the user's folder when none is chosen, for shared files (ButtonBars.xml, scripts)
fn shared_folder(state: &AppState) -> PathBuf {
    state.settings.lock().unwrap().settings_folder.clone().unwrap_or_else(|| state.user_folder.clone())
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
        requests::PENDING.lock().unwrap().push(request);
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
        builder.plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| requests::deliver(app, cli::from_args(argv.get(1..).unwrap_or(&[])))))
    };
    builder
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // the main window opens where it was last, its size, maximized or full screen as it was left
        .plugin(
            tauri_plugin_window_state::Builder::new()
                .with_state_flags(windows::remembered_state())
                .with_filter(windows::remembers_place)
                .build(),
        )
        .menu({
            let settings = settings.clone();
            move |app| menu(app, &settings)
        })
        // menu items the UI handles arrive there as a "menu" event with the item's id
        .on_menu_event(|app, event| {
            if event.id().0.starts_with("tray:") {
                requests::tray_command(app, event.id().0.as_str());
                return;
            }
            if event.id().0 == "thirdPartyNotices" {
                open_notices(app);
                return;
            }
            // to the window in front only: with several windows, Cmd+T opens one shell, in that window
            let front = windows::FRONT_WINDOW.lock().unwrap().clone().filter(|label| app.get_webview_window(label).is_some());
            let _ = app.emit_to(front.as_deref().unwrap_or("main"), "menu", event.id().0.as_str());
        })
        // a window's tabs go with it: their shells and connections end
        .on_window_event(|window, event| match event {
            tauri::WindowEvent::Focused(true) => *windows::FRONT_WINDOW.lock().unwrap() = Some(window.label().to_string()),
            tauri::WindowEvent::Destroyed => {
                applog::info(format!("Window {} closed; its sessions end", window.label()));
                // kept now, not only when the app quits: it may stay in the menu bar or tray, and be ended there
                if windows::remembers_place(window.label()) {
                    use tauri_plugin_window_state::AppHandleExt;
                    let _ = window.app_handle().save_window_state(windows::remembered_state());
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
            windows::no_siri_badge();
            windows::main_window(app.handle())?;
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
            commands::settings::settings_get,
            commands::settings::settings_set_folder,
            commands::files::folder_writable,
            commands::files::folder_exists,
            commands::settings::settings_save,
            commands::appearance::icons_custom,
            commands::appearance::highlighting_load,
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
            commands::external::session_external,
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
            commands::settings::onepassword_check,
            commands::appearance::ui_themes_load,
            commands::appearance::ui_themes_save,
            windows::window_new,
            commands::layouts::layouts_list,
            commands::logs::applog_since,
            commands::logs::applog_write,
            commands::logs::applog_clear,
            commands::logs::applog_reveal,
            commands::session_files::sessions_reveal,
            commands::session_files::sessions_backups,
            commands::session_files::sessions_restore,
            commands::sending::command_history_load,
            commands::sending::command_history_add,
            requests::requests_take,
            requests::links_make_default,
            commands::settings::shortcuts_keys,
            commands::settings::shortcuts_reserved,
            commands::layouts::layout_read,
            commands::layouts::layout_save,
            commands::layouts::layout_delete,
            commands::layouts::layout_rename,
            windows::window_start,
            commands::importing::sessions_import_plan,
            commands::importing::sessions_import_apply,
            commands::importing::ssh_config_path,
            commands::importing::sessions_import_source,
            commands::settings::ssh_algorithms_available,
            commands::session_files::sessions_export_xml,
            commands::session_files::sessions_edit_externally,
            commands::overrides::overrides_set,
            commands::overrides::override_presets_get,
            commands::profiles::profile_key_age,
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
            commands::appearance::highlighting_user_folder,
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
                        requests::deliver(app, cli::from_url(url.as_str()));
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
                tauri::RunEvent::Reopen { has_visible_windows: false, .. } => requests::deliver(app, None),
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
}
