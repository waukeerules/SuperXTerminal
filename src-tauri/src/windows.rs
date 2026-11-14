//! The app's windows: the main one and those the page asks for (New Window, Move to New Window, Connect in new
//! window), what each is to open as it starts, which is in front, where the main window was left, and the theme
//! of their title bars and menus. On the Mac, also turning off Writing Tools and Siri's badge in the web view.
//! A command that makes a window must be async, or WebView2 never finishes it on Windows (see the test here).

use std::sync::Mutex;

use tauri::{AppHandle, Manager};

use crate::{demo, AppState};

/// What a new window starts with: a session to open ("store:id", Connect in new window), or a running tab to take over
/// (Move to New Window: its session's pty id, title and how its tab looks, as the window that had it describes it)
#[derive(Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct WindowStart {
    open: Option<String>,
    adopt: Option<serde_json::Value>,
    /// Load in New Window: a layout to open
    layout: Option<String>,
}

/// What each new window starts with, by its label; taken once by `window_start`
pub(crate) static WINDOW_OPENS: Mutex<Option<std::collections::HashMap<String, WindowStart>>> = Mutex::new(None);

/// The window last brought to the front, which the menu bar's commands go to
pub(crate) static FRONT_WINDOW: Mutex<Option<String>> = Mutex::new(None);

/// The number of the next window ("window-2" ...)
pub(crate) static NEXT_WINDOW: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(2);

/// File > New Window, and Connect in new window (`open` the session, "store:id"), as SuperXPuTTY's new instance: another
/// window with its own Sessions tree and tabs
///
/// Async, so it runs off the main thread. On Windows Tauri runs a plain command on the main thread, inside WebView2's
/// handling of the page's call, and `run_on_main_thread` from there makes the window at once, in that handler: WebView2
/// never finished the new web view, which stayed blank (about:blank, the window's grey), and the page that asked never
/// heard back. Move to New Window, New Window and Connect in new window all did this on Windows; the Mac didn't mind.
#[tauri::command]
pub(crate) async fn window_new(app: AppHandle, open: Option<String>, adopt: Option<serde_json::Value>, layout: Option<String>) -> Result<(), String> {
    new_window(&app, open, adopt, layout)
}

/// Make another window, starting with `open`, `adopt` or `layout` (see [`WindowStart`]), on the main thread. Never
/// from a plain (sync) command: see [`window_new`].
pub(crate) fn new_window(app: &AppHandle, open: Option<String>, adopt: Option<serde_json::Value>, layout: Option<String>) -> Result<(), String> {
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
pub(crate) fn window_start(window: tauri::WebviewWindow) -> Option<WindowStart> {
    WINDOW_OPENS.lock().unwrap().as_mut().and_then(|opens| opens.remove(window.label()))
}

/// The main window, made here rather than in tauri.conf.json so its web view can have Writing Tools off: macOS put
/// its "Write with Siri" badge by the terminal's cursor, and offered to rewrite what's typed into a session
pub(crate) fn main_window(app: &tauri::AppHandle) -> tauri::Result<()> {
    window(app, "main", None)
}

/// A window's own light or dark for the app's theme (Settings > General > Theme); None follows the system. On
/// Windows it's what Windows draws the window's menu bar and title bar in, which otherwise follow the system whatever
/// the app's theme; on the Mac the menu bar is the system's, so windows are left to it.
#[cfg(windows)]
pub(crate) fn window_theme(theme: &str) -> Option<tauri::Theme> {
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
pub(crate) fn apply_menu_theme(theme: &str) {
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
pub(crate) fn apply_window_theme(app: &AppHandle, theme: &str) {
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
pub(crate) fn loading_colour(theme: &str) -> tauri::window::Color {
    if theme == "light" { tauri::window::Color(0xf5, 0xf5, 0xf5, 0xff) } else { tauri::window::Color(0x2d, 0x2d, 0x30, 0xff) }
}

/// Whether a window opens where it was last left: the main window only, as the one SuperXTerminal starts with; New
/// Window's windows each start down and to the right of the one in front, as a Mac app cascades them
pub(crate) fn remembers_place(label: &str) -> bool {
    label == "main" && !demo::on()
}

/// What's remembered of the main window (tauri-plugin-window-state, in the app's config folder as .window-state.json):
/// its position and size, and whether it was maximized or full screen. Not whether it was shown, since it's always
/// shown at the start, nor its frame, which is always the system's.
pub(crate) fn remembered_state() -> tauri_plugin_window_state::StateFlags {
    use tauri_plugin_window_state::StateFlags;
    StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED | StateFlags::FULLSCREEN
}

/// A SuperXTerminal window: the first ("main"), or another made by New Window ("window-2", ...) at `at` (logical)
pub(crate) fn window(app: &tauri::AppHandle, label: &str, at: Option<(f64, f64)>) -> tauri::Result<()> {
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
pub(crate) fn writing_tools_off() -> objc2::rc::Retained<objc2_web_kit::WKWebViewConfiguration> {
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
pub(crate) fn no_siri_badge() {
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

#[cfg(test)]
mod tests {
    use super::{loading_colour, remembered_state, remembers_place};
    use tauri_plugin_window_state::StateFlags;

    #[test]
    fn a_window_loads_dark_unless_the_theme_is_light() {
        use tauri::window::Color;
        assert_eq!(loading_colour("light"), Color(0xf5, 0xf5, 0xf5, 0xff));
        assert_eq!(loading_colour("dark"), Color(0x2d, 0x2d, 0x30, 0xff));
        assert_eq!(loading_colour("system"), Color(0x2d, 0x2d, 0x30, 0xff));
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
        // Every source file is read, wherever the commands are (they were all in lib.rs once), so a command that
        // makes a window is found whichever module it's written in.
        let folder = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let files = [folder.clone(), folder.join("commands")].into_iter().flat_map(|dir| std::fs::read_dir(dir).unwrap().flatten()).map(|entry| entry.path());
        // as one kind of line ending, whichever Git checked out
        let source: String = files.filter(|path| path.extension().is_some_and(|e| e == "rs")).map(|path| std::fs::read_to_string(path).unwrap().replace("\r\n", "\n")).collect();
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
