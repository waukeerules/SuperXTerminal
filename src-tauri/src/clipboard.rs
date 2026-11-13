//! The clipboard's text for a paste (Shift+Insert, the tab menu's Paste, `\v` in a send string, Type Clipboard Text),
//! read by the system rather than by the page.
//!
//! The page's own `navigator.clipboard.readText` made WebView2 ask "localhost wants to see text and images copied to
//! the clipboard", as it does for a website; a program reading the clipboard isn't asked. On Windows it's the Win32
//! clipboard (`CF_UNICODETEXT`), on the Mac the general pasteboard, and on Linux the desktop's (X11's, or Wayland's)
//! through the arboard crate ([`desktop`]). Writing stays with the page, which WebView2 allows without asking.
//!
//! Remote Desktop's clipboard ([`crate::rdp_clipboard`]) reads and writes this computer's clipboard here too
//! ([`read`], [`write`]), on every system.

/// The text on the clipboard; empty when there's none (nothing copied, or a picture)
#[tauri::command]
pub fn clipboard_text() -> String {
    read().unwrap_or_default()
}

/// The text on the Mac's clipboard, if any
#[cfg(target_os = "macos")]
pub(crate) fn read() -> Option<String> {
    crate::rdp_clipboard::mac::read()
}

/// The text on Windows' clipboard, if any
#[cfg(windows)]
pub(crate) fn read() -> Option<String> {
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, GetClipboardData, IsClipboardFormatAvailable};
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    if !open() {
        return None;
    }
    // SAFETY: the clipboard is open (closed below on every path); the handle is the clipboard's, locked only while
    // read, and read no further than its size or the first NUL
    unsafe {
        let mut text = None;
        if IsClipboardFormatAvailable(CF_UNICODETEXT as u32) != 0 {
            let handle = GetClipboardData(CF_UNICODETEXT as u32);
            let data = if handle.is_null() { std::ptr::null_mut() } else { GlobalLock(handle) } as *const u16;
            if !data.is_null() {
                let units = std::slice::from_raw_parts(data, GlobalSize(handle) / 2);
                let end = units.iter().position(|&u| u == 0).unwrap_or(units.len());
                text = Some(String::from_utf16_lossy(&units[..end]));
                GlobalUnlock(handle);
            }
        }
        CloseClipboard();
        text
    }
}

/// Open Windows' clipboard for this program, trying for a moment: another program (a clipboard manager, Remote
/// Desktop) may have it open just then, and OpenClipboard fails rather than waits
#[cfg(windows)]
fn open() -> bool {
    use windows_sys::Win32::System::DataExchange::OpenClipboard;
    for _ in 0..10 {
        // SAFETY: no window: the clipboard is opened for this thread
        if unsafe { OpenClipboard(std::ptr::null_mut()) } != 0 {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    false
}

/// The text on Linux's clipboard, if any
#[cfg(target_os = "linux")]
pub(crate) fn read() -> Option<String> {
    desktop::read()
}

/// No clipboard to read on other systems (the app is built for the Mac, Windows and Linux)
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub(crate) fn read() -> Option<String> {
    None
}

/// Linux's clipboard, through arboard: X11's, or Wayland's where the desktop lets a program at it without a window
/// in front (the data-control protocol; elsewhere arboard goes through XWayland).
///
/// On Linux a copy isn't handed to the system: the program that copied keeps the text and gives it to whoever
/// pastes. So arboard's clipboard is made once and kept for as long as the app runs ([`CLIPBOARD`]); let go after
/// each copy, the text would be gone before anything pasted it.
///
/// Linux only: arboard's Windows side brings a Boost-licensed crate (clipboard-win), which the licence check
/// (deny.toml) doesn't allow, and Windows and the Mac have their own code above.
#[cfg(target_os = "linux")]
mod desktop {
    use std::sync::Mutex;

    /// arboard's clipboard, made at the first read or copy and kept; None until then, and while there's no desktop
    /// to ask (no display: tried again next time)
    static CLIPBOARD: Mutex<Option<arboard::Clipboard>> = Mutex::new(None);

    /// Do something with the clipboard; None when there's none to ask, or it failed (no text on it, for a read)
    fn with<T>(act: impl FnOnce(&mut arboard::Clipboard) -> Result<T, arboard::Error>) -> Option<T> {
        let mut kept = CLIPBOARD.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if kept.is_none() {
            *kept = arboard::Clipboard::new().ok();
        }
        act(kept.as_mut()?).ok()
    }

    /// The text on the clipboard, if any
    pub fn read() -> Option<String> {
        with(|clipboard| clipboard.get_text())
    }

    /// Put text on the clipboard, in place of what it held; left as it was when there's no desktop to ask
    pub fn write(text: &str) {
        with(|clipboard| clipboard.set_text(text.to_owned()));
    }
}

/// Put text on the Mac's clipboard, in place of what it held
#[cfg(target_os = "macos")]
pub(crate) fn write(text: &str) {
    crate::rdp_clipboard::mac::write(text);
}

/// Put text on Windows' clipboard (`CF_UNICODETEXT`), in place of what it held. Left as it was when the clipboard
/// can't be opened (another program has it) or there's no memory for the text
#[cfg(windows)]
pub(crate) fn write(text: &str) {
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, SetClipboardData};
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

    let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
    if !open() {
        return;
    }
    // SAFETY: the clipboard is open (closed below on every path); the memory is sized for the text and its NUL, and
    // is the clipboard's once set, or freed here when it wasn't taken
    unsafe {
        let handle = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2);
        let data = if handle.is_null() { std::ptr::null_mut() } else { GlobalLock(handle) } as *mut u16;
        if !data.is_null() {
            std::ptr::copy_nonoverlapping(units.as_ptr(), data, units.len());
            GlobalUnlock(handle);
            EmptyClipboard();
            if SetClipboardData(CF_UNICODETEXT as u32, handle).is_null() {
                GlobalFree(handle);
            }
        } else if !handle.is_null() {
            GlobalFree(handle);
        }
        CloseClipboard();
    }
}

/// Put text on Linux's clipboard, in place of what it held
#[cfg(target_os = "linux")]
pub(crate) fn write(text: &str) {
    desktop::write(text);
}

/// No clipboard to write on other systems (the app is built for the Mac, Windows and Linux)
#[cfg(not(any(windows, target_os = "macos", target_os = "linux")))]
pub(crate) fn write(_text: &str) {}

/// The system's clipboard is one for every test: a test that copies holds this while it does, so another's copy
/// doesn't land in between
#[cfg(test)]
pub(crate) fn hold_for_test() -> std::sync::MutexGuard<'static, ()> {
    static HELD: std::sync::Mutex<()> = std::sync::Mutex::new(());
    HELD.lock().unwrap_or_else(|e| e.into_inner())
}

/// The clipboard's text once it's `text`, for a test: right after a copy, Windows 11 had the clipboard without text
/// for an instant (a clipboard watcher, such as clipboard history, taking the new copy), so it's read for up to a second
#[cfg(test)]
pub(crate) fn read_for_test(text: &str) -> String {
    let mut read = read().unwrap_or_default();
    for _ in 0..50 {
        if read == text {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
        read = self::read().unwrap_or_default();
    }
    read
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The system's clipboard is the developer's too: what was on it is put back after the test. Only where there's a
    /// clipboard to read: the Mac and Windows
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn a_paste_gets_the_text_copied_in_another_app() {
        let _held = hold_for_test();
        let before = clipboard_text();
        write("FastEthernet0/5 – ünïcode");
        let read = read_for_test("FastEthernet0/5 – ünïcode");
        write(&before);
        assert_eq!(read, "FastEthernet0/5 – ünïcode");
    }

    /// Linux's clipboard, on Linux alone (the Mac and Windows have the test above for theirs): text put on it is
    /// read back, as a copy in the remote desktop is pasted here and a copy here is offered there. It needs a
    /// desktop: a build machine has no display, and the test then has nothing to check.
    #[cfg(target_os = "linux")]
    #[test]
    fn the_linux_clipboard_gives_back_the_text_it_was_given() {
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            return;
        }
        let _held = hold_for_test();
        let before = clipboard_text();
        write("GigabitEthernet1/0/1 – ünïcode");
        let read = read_for_test("GigabitEthernet1/0/1 – ünïcode");
        write(&before);
        assert_eq!(read, "GigabitEthernet1/0/1 – ünïcode");
    }
}
