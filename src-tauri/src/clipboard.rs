//! The clipboard's text for a paste (Shift+Insert, the tab menu's Paste, `\v` in a send string, Type Clipboard Text),
//! read by the system rather than by the page.
//!
//! The page's own `navigator.clipboard.readText` made WebView2 ask "localhost wants to see text and images copied to
//! the clipboard", as it does for a website; a program reading the clipboard isn't asked. On Windows it's the Win32
//! clipboard (`CF_UNICODETEXT`), on the Mac the general pasteboard, read as Remote Desktop's clipboard reads it
//! ([`crate::rdp_clipboard`]). Writing stays with the page, which WebView2 allows without asking.

/// The text on the clipboard; empty when there's none (nothing copied, or a picture)
#[tauri::command]
pub fn clipboard_text() -> String {
    read().unwrap_or_default()
}

/// The text on the Mac's clipboard, if any
#[cfg(target_os = "macos")]
fn read() -> Option<String> {
    crate::rdp_clipboard::mac::read()
}

/// The text on Windows' clipboard, if any
#[cfg(windows)]
fn read() -> Option<String> {
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

/// No clipboard to read on other systems (the app ships for the Mac and Windows)
#[cfg(not(any(windows, target_os = "macos")))]
fn read() -> Option<String> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Put text on the system's clipboard, as copying it in another app does
    #[cfg(target_os = "macos")]
    fn copy(text: &str) {
        crate::rdp_clipboard::mac::write(text);
    }

    /// Put text on the system's clipboard, as copying it in another app does
    #[cfg(windows)]
    fn copy(text: &str) {
        use windows_sys::Win32::System::DataExchange::{CloseClipboard, EmptyClipboard, SetClipboardData};
        use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
        use windows_sys::Win32::System::Ole::CF_UNICODETEXT;

        let units: Vec<u16> = text.encode_utf16().chain([0]).collect();
        assert!(open(), "the clipboard could be opened");
        // SAFETY: the clipboard is open; the memory is sized for the text and its NUL, and the clipboard owns it once set
        unsafe {
            EmptyClipboard();
            let handle = GlobalAlloc(GMEM_MOVEABLE, units.len() * 2);
            let data = GlobalLock(handle) as *mut u16;
            std::ptr::copy_nonoverlapping(units.as_ptr(), data, units.len());
            GlobalUnlock(handle);
            SetClipboardData(CF_UNICODETEXT as u32, handle);
            CloseClipboard();
        }
    }

    /// The system's clipboard is the developer's too: what was on it is put back after the test. Only where there's a
    /// clipboard to read: the Mac and Windows
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn a_paste_gets_the_text_copied_in_another_app() {
        let before = clipboard_text();
        copy("FastEthernet0/5 – ünïcode");
        // read as a paste is, a moment after the copy: right after it, Windows 11 had the clipboard without text for an
        // instant (a clipboard watcher, such as clipboard history, taking the new copy)
        let mut read = clipboard_text();
        for _ in 0..50 {
            if !read.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
            read = clipboard_text();
        }
        copy(&before);
        assert_eq!(read, "FastEthernet0/5 – ünïcode");
    }
}
