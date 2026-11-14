//! What the app is asked to open from outside its windows: the command line SuperXPuTTY takes, a second start of
//! the app, an ssh:// or telnet:// link, and the tray's menu. Each request waits here until the window in front
//! takes it (`requests_take`), since it may arrive before any window is ready. Also makes the app the system's
//! default for those links. The command line itself is read in `crate::cli`, the tray drawn in `crate::tray`.

use std::sync::Mutex;

use tauri::{AppHandle, Emitter, Manager};

use crate::{cli, windows};

/// What's been asked to open from outside (the command line, a second start, a link), waiting for the window in
/// front to take it (`requests_take`)
pub(crate) static PENDING: Mutex<Vec<cli::Request>> = Mutex::new(Vec::new());

/// The menu bar icon's items ("tray:..."): a session opens in the window in front, or in a new one when none is open
pub(crate) fn tray_command(app: &AppHandle, id: &str) {
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
            let _ = windows::new_window(app, None, None, None);
        }
        "tray:newLocalShell" | "tray:openSession" => {
            deliver(app, None);
            let front = windows::FRONT_WINDOW.lock().unwrap().clone().unwrap_or_else(|| "main".into());
            let _ = app.emit_to(front.as_str(), "menu", id.trim_start_matches("tray:"));
        }
        _ => deliver(app, None),
    }
}

/// Something to open from outside: kept for the window in front, which is told and brought forward; with no window
/// open (SuperXTerminal kept in the menu bar), a new one, which takes it as it starts
pub(crate) fn deliver(app: &AppHandle, request: Option<cli::Request>) {
    if app.webview_windows().is_empty() {
        if let Some(request) = request {
            PENDING.lock().unwrap().push(request);
        }
        let handle = app.clone();
        let _ = app.run_on_main_thread(move || {
            let _ = windows::main_window(&handle);
        });
        return;
    }
    let front = windows::FRONT_WINDOW.lock().unwrap().clone().filter(|label| app.get_webview_window(label).is_some()).unwrap_or_else(|| "main".into());
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
pub(crate) fn requests_take() -> Vec<cli::Request> {
    std::mem::take(&mut *PENDING.lock().unwrap())
}

/// Settings > General: SuperXTerminal opens ssh:// and telnet:// links (Terminal does until then), as LaunchServices
/// records it for the user
#[tauri::command]
pub(crate) fn links_make_default(app: AppHandle) -> Result<(), String> {
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
