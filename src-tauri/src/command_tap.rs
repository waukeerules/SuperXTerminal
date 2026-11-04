//! ⌘ tapped on its own (pressed and let go with no other key, click or modifier in between), told to the window that
//! has the keyboard as a `command-tap` event: a Remote Desktop tab sends it as the Windows key (Start). The web page
//! can't see it, since a modifier pressed alone never reaches the web view as a key event, so it's watched here with
//! an NSEvent local monitor (this app's own events only, so it needs no permission).

use tauri::AppHandle;

/// Start watching; once, from the app's setup (on the main thread)
#[cfg(target_os = "macos")]
pub fn install(app: &AppHandle) {
    use std::ptr::NonNull;
    use std::sync::atomic::{AtomicBool, Ordering};

    use block2::RcBlock;
    use objc2_app_kit::{NSEvent, NSEventMask, NSEventModifierFlags, NSEventType};
    use tauri::{Emitter, Manager};

    /// Whether Command is down now
    static DOWN: AtomicBool = AtomicBool::new(false);
    /// Whether it's been alone since it went down (no other key, modifier or click): a tap when it goes up
    static ALONE: AtomicBool = AtomicBool::new(false);

    let app = app.clone();
    let handler = RcBlock::new(move |event: NonNull<NSEvent>| -> *mut NSEvent {
        // SAFETY: AppKit hands the monitor a valid event for the duration of the call
        let e = unsafe { event.as_ref() };
        let kind = e.r#type();
        if kind == NSEventType::FlagsChanged {
            let flags = e.modifierFlags();
            let command = flags.contains(NSEventModifierFlags::Command);
            let others = flags.intersects(NSEventModifierFlags::Shift | NSEventModifierFlags::Control | NSEventModifierFlags::Option);
            if command && !DOWN.load(Ordering::Relaxed) {
                DOWN.store(true, Ordering::Relaxed);
                ALONE.store(!others, Ordering::Relaxed);
            } else if !command && DOWN.load(Ordering::Relaxed) {
                DOWN.store(false, Ordering::Relaxed);
                if ALONE.swap(false, Ordering::Relaxed) {
                    if let Some(window) = app.webview_windows().into_values().find(|w| w.is_focused().unwrap_or(false)) {
                        let _ = app.emit_to(window.label(), "command-tap", ());
                    }
                }
            } else if others {
                ALONE.store(false, Ordering::Relaxed);
            }
        } else {
            // a key or a click with ⌘ held: a shortcut, not a tap
            ALONE.store(false, Ordering::Relaxed);
        }
        event.as_ptr()
    });
    let mask = NSEventMask::FlagsChanged
        | NSEventMask::KeyDown
        | NSEventMask::LeftMouseDown
        | NSEventMask::RightMouseDown
        | NSEventMask::OtherMouseDown
        | NSEventMask::ScrollWheel;
    // SAFETY: the handler returns the event it was given; the monitor lasts as long as the app
    let monitor = unsafe { NSEvent::addLocalMonitorForEventsMatchingMask_handler(mask, &handler) };
    std::mem::forget(monitor);
}

/// Elsewhere there's no Command key to watch
#[cfg(not(target_os = "macos"))]
pub fn install(_app: &AppHandle) {}
