//! This computer's clipboard in every form Remote Desktop passes: text, formatted text (HTML and RTF), a picture,
//! and files. [`crate::clipboard`] is the text alone, for a paste into a terminal; this is what
//! [`crate::rdp_clipboard`] reads and writes.
//!
//! The forms are Windows' own (RDP's clipboard is Windows' clipboard sent over a channel), so a picture is a DIB,
//! formatted text is "HTML Format" and "Rich Text Format" as Windows programs put them, and files are a list of
//! paths. On Windows ([`windows`]) they pass through as bytes, unchanged. The Mac's clipboard ([`mac`]) holds a
//! picture as a PNG or TIFF, formatted text as plain HTML and files as their addresses, so each is converted on the
//! way ([`crate::clipboard_forms`]); RTF is the same bytes on both. On Linux only text is passed so far ([`kinds`]
//! says there's nothing else, and the rest gives none).
//!
//! Two things keep copies from costing more than they should:
//!
//! - Nothing is read until it's asked for. [`kinds`] only asks what the clipboard holds; a 30 MB screenshot is read
//!   when the remote desktop pastes it, not each time the tab gets the keyboard.
//! - Files copied in the remote desktop aren't fetched when they're copied, only when they're pasted here; that's
//!   [`crate::clipboard_files`], through Windows' OLE clipboard.
//!
//! [`stamp`] and [`is_ours`] tell a copy made on this computer from what was put on the clipboard here, so what came
//! from the remote desktop isn't offered straight back to it.

use std::path::PathBuf;

/// What a clipboard holds, in the forms passed between this computer and a remote desktop; each as Windows has it
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Contents {
    /// Plain text
    pub text: Option<String>,
    /// Formatted text as "HTML Format": a short header of offsets, then the HTML (as browsers and Office copy it)
    pub html: Option<Vec<u8>>,
    /// Formatted text as "Rich Text Format"
    pub rtf: Option<Vec<u8>>,
    /// A picture as a device-independent bitmap (CF_DIB): its header, then its pixels
    pub image: Option<Vec<u8>>,
    /// Files and folders, by their paths on this computer
    pub files: Vec<PathBuf>,
}

/// Which forms a clipboard holds, without reading any of them
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Kinds {
    pub text: bool,
    pub html: bool,
    pub rtf: bool,
    pub image: bool,
    pub files: bool,
}

#[cfg(windows)]
pub use windows::{files, html, image, is_ours, kinds, mark_ours, put, rtf, stamp, text};

#[cfg(target_os = "macos")]
pub use mac::{files, html, image, is_ours, kinds, mark_ours, put, rtf, stamp, text};

/// What this computer's clipboard holds: text or nothing, on Linux
#[cfg(not(any(windows, target_os = "macos")))]
pub fn kinds() -> Kinds {
    Kinds { text: text().is_some(), ..Kinds::default() }
}

/// The clipboard's text, if it has any
#[cfg(not(any(windows, target_os = "macos")))]
pub fn text() -> Option<String> {
    crate::clipboard::read().filter(|text| !text.is_empty())
}

/// No formatted text is read on Linux yet
#[cfg(not(any(windows, target_os = "macos")))]
pub fn html() -> Option<Vec<u8>> {
    None
}

/// No formatted text is read on Linux yet
#[cfg(not(any(windows, target_os = "macos")))]
pub fn rtf() -> Option<Vec<u8>> {
    None
}

/// No picture is read on Linux yet
#[cfg(not(any(windows, target_os = "macos")))]
pub fn image() -> Option<Vec<u8>> {
    None
}

/// No files are read on Linux yet
#[cfg(not(any(windows, target_os = "macos")))]
pub fn files() -> Vec<PathBuf> {
    Vec::new()
}

/// Put `contents` on the clipboard in place of what it held: its text, on Linux
#[cfg(not(any(windows, target_os = "macos")))]
pub fn put(contents: &Contents) {
    if let Some(text) = &contents.text {
        crate::clipboard::write(text);
    }
}

/// Linux gives no number that changes with the clipboard here: the text itself is compared instead
#[cfg(not(any(windows, target_os = "macos")))]
pub fn stamp() -> Option<u32> {
    None
}

/// Unknown without [`stamp`]
#[cfg(not(any(windows, target_os = "macos")))]
pub fn is_ours() -> bool {
    false
}

/// Nothing to mark without [`stamp`]
#[cfg(not(any(windows, target_os = "macos")))]
pub fn mark_ours() {}

/// The Mac's clipboard (the general pasteboard). Every use of it holds [`crate::clipboard::one_at_a_time`]: the
/// pasteboard is for one thread at a time, and each Remote Desktop tab has a thread of its own.
#[cfg(target_os = "macos")]
mod mac {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use objc2::rc::{autoreleasepool, Retained};
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::{
        NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard, NSPasteboardType, NSPasteboardTypeFileURL, NSPasteboardTypeHTML,
        NSPasteboardTypePNG, NSPasteboardTypeRTF, NSPasteboardTypeString, NSPasteboardTypeTIFF, NSPasteboardWriting,
    };
    use objc2_foundation::{NSArray, NSData, NSDictionary, NSString, NSURL};

    use super::{Contents, Kinds};
    use crate::clipboard_forms::{bmp_from_dib, dib_from_png, html_format, html_from_format};

    /// The clipboard's number ([`stamp`]) right after this module last changed it
    static OURS: AtomicU32 = AtomicU32::new(0);

    /// Whether the clipboard holds something of a type
    fn has(board: &NSPasteboard, kind: &NSPasteboardType) -> bool {
        board.types().is_some_and(|types| types.iter().any(|held| &*held == kind))
    }

    /// The bytes the clipboard holds of a type, if any
    fn bytes(board: &NSPasteboard, kind: &NSPasteboardType) -> Option<Vec<u8>> {
        board.dataForType(kind).map(|data| data.to_vec())
    }

    /// A picture in another file format (PNG or TIFF), from one the Mac can read (PNG, TIFF, a bitmap file)
    fn picture_as(picture: &[u8], format: NSBitmapImageFileType) -> Option<Vec<u8>> {
        let image = NSBitmapImageRep::imageRepWithData(&NSData::with_bytes(picture))?;
        // SAFETY: no properties are given, so there are none of the wrong type
        let converted = unsafe { image.representationUsingType_properties(format, &NSDictionary::new()) }?;
        Some(converted.to_vec())
    }

    /// Which forms the clipboard holds, without reading any. Files copied in Finder come with their names as text
    /// and sometimes their icon as a picture; they're files.
    pub fn kinds() -> Kinds {
        let _one = crate::clipboard::one_at_a_time();
        // SAFETY: the pasteboard types are AppKit's own constants
        unsafe {
            let board = NSPasteboard::generalPasteboard();
            Kinds {
                text: has(&board, NSPasteboardTypeString),
                html: has(&board, NSPasteboardTypeHTML),
                rtf: has(&board, NSPasteboardTypeRTF),
                image: has(&board, NSPasteboardTypePNG) || has(&board, NSPasteboardTypeTIFF),
                files: has(&board, NSPasteboardTypeFileURL),
            }
        }
    }

    /// The clipboard's text, if it has any
    pub fn text() -> Option<String> {
        crate::clipboard::read().filter(|text| !text.is_empty())
    }

    /// The clipboard's formatted text as Windows' "HTML Format", if it has any: the Mac holds the HTML alone
    pub fn html() -> Option<Vec<u8>> {
        let _one = crate::clipboard::one_at_a_time();
        // SAFETY: the pasteboard type is AppKit's own constant
        let html = unsafe { NSPasteboard::generalPasteboard().stringForType(NSPasteboardTypeHTML) }?.to_string();
        (!html.is_empty()).then(|| html_format(&html))
    }

    /// The clipboard's formatted text as RTF, if it has any: the same on the Mac as on Windows
    pub fn rtf() -> Option<Vec<u8>> {
        let _one = crate::clipboard::one_at_a_time();
        // SAFETY: the pasteboard type is AppKit's own constant
        unsafe { bytes(&NSPasteboard::generalPasteboard(), NSPasteboardTypeRTF) }
    }

    /// The clipboard's picture as a DIB, if it has one: its PNG, or its TIFF (as Preview copies) made a PNG first
    pub fn image() -> Option<Vec<u8>> {
        let _one = crate::clipboard::one_at_a_time();
        autoreleasepool(|_| {
            let board = NSPasteboard::generalPasteboard();
            // SAFETY: the pasteboard types are AppKit's own constants
            let png = unsafe {
                bytes(&board, NSPasteboardTypePNG)
                    .or_else(|| picture_as(&bytes(&board, NSPasteboardTypeTIFF)?, NSBitmapImageFileType::PNG))
            }?;
            dib_from_png(&png)
        })
    }

    /// The files and folders on the clipboard, if any: each is held as its address (file://...), which may name the
    /// file by a number of the disk's rather than its path; the path is asked of the address
    pub fn files() -> Vec<PathBuf> {
        let _one = crate::clipboard::one_at_a_time();
        autoreleasepool(|_| {
            let items = NSPasteboard::generalPasteboard().pasteboardItems();
            let mut paths = Vec::new();
            for item in items.iter().flat_map(|items| items.iter()) {
                // SAFETY: the pasteboard type is AppKit's own constant
                let Some(address) = (unsafe { item.stringForType(NSPasteboardTypeFileURL) }) else { continue };
                // the Mac spells an accented letter in an address as the letter and its accent apart; Windows wants
                // them as one, and the Mac's disks find the file by either
                if let Some(path) = NSURL::URLWithString(&address).and_then(|url| url.path()) {
                    paths.push(PathBuf::from(path.precomposedStringWithCanonicalMapping().to_string()));
                }
            }
            paths
        })
    }

    /// Put `contents` on the clipboard in place of what it held, every form it has: a picture as a PNG and a TIFF
    /// (Mac programs paste one or the other), formatted text as the HTML alone, files as their addresses
    pub fn put(contents: &Contents) {
        let one = crate::clipboard::one_at_a_time();
        autoreleasepool(|_| {
            let board = NSPasteboard::generalPasteboard();
            board.clearContents();
            if !contents.files.is_empty() {
                let addresses: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = contents
                    .files
                    .iter()
                    .map(|path| ProtocolObject::from_retained(NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))))
                    .collect();
                board.writeObjects(&NSArray::from_retained_slice(&addresses));
            }
            // SAFETY: the pasteboard types are AppKit's own constants
            unsafe {
                if let Some(text) = &contents.text {
                    board.setString_forType(&NSString::from_str(text), NSPasteboardTypeString);
                }
                if let Some(html) = contents.html.as_deref().and_then(html_from_format) {
                    board.setString_forType(&NSString::from_str(&html), NSPasteboardTypeHTML);
                }
                if let Some(rtf) = &contents.rtf {
                    // Windows programs end it with a NUL, which isn't part of it
                    let end = rtf.iter().rposition(|byte| *byte != 0).map_or(0, |last| last + 1);
                    board.setData_forType(Some(&NSData::with_bytes(&rtf[..end])), NSPasteboardTypeRTF);
                }
                if let Some(bmp) = contents.image.as_deref().and_then(bmp_from_dib) {
                    for (format, kind) in [(NSBitmapImageFileType::PNG, NSPasteboardTypePNG), (NSBitmapImageFileType::TIFF, NSPasteboardTypeTIFF)] {
                        if let Some(picture) = picture_as(&bmp, format) {
                            board.setData_forType(Some(&NSData::with_bytes(&picture)), kind);
                        }
                    }
                }
            }
            // what's there now is this app's ([`mark_ours`], without asking for the clipboard a second time)
            OURS.store(board.changeCount() as u32, Ordering::Relaxed);
        });
        drop(one);
    }

    /// What's on the clipboard now was put there by this app: it isn't a copy made on this computer
    pub fn mark_ours() {
        OURS.store(stamp().unwrap_or(0), Ordering::Relaxed);
    }

    /// A number that changes each time anything changes the clipboard (the pasteboard's change count)
    pub fn stamp() -> Option<u32> {
        let _one = crate::clipboard::one_at_a_time();
        Some(NSPasteboard::generalPasteboard().changeCount() as u32)
    }

    /// Whether what's on the clipboard now was put there by this module, rather than copied on this computer
    pub fn is_ours() -> bool {
        stamp() == Some(OURS.load(Ordering::Relaxed))
    }
}

/// Windows' clipboard
#[cfg(windows)]
mod windows {
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    use windows_sys::Win32::Foundation::{GlobalFree, HWND};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber, IsClipboardFormatAvailable, OpenClipboard,
        RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock, GMEM_MOVEABLE};

    use super::{Contents, Kinds};

    /// Plain text, as UTF-16 with a NUL after it
    const CF_UNICODETEXT: u32 = 13;
    /// A picture: a bitmap's header and pixels
    const CF_DIB: u32 = 8;
    /// Files: a DROPFILES header and their paths
    const CF_HDROP: u32 = 15;
    /// The size of a DROPFILES header: where the paths start, a point, and two flags
    const DROPFILES: usize = 20;
    /// The clipboard's number ([`stamp`]) right after this module last changed it
    static OURS: AtomicU32 = AtomicU32::new(0);
    /// The number Windows gives a registered clipboard format, by its name ("HTML Format"); 0 if it can't
    fn registered(name: &str) -> u32 {
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        // SAFETY: `wide` is a NUL-terminated UTF-16 string that lives through the call
        unsafe { RegisterClipboardFormatW(wide.as_ptr()) }
    }

    /// Formatted text as browsers and Office copy it
    fn html_format() -> u32 {
        registered("HTML Format")
    }

    /// Formatted text as word processors copy it
    fn rtf_format() -> u32 {
        registered("Rich Text Format")
    }

    /// Open the clipboard for this thread, trying for a moment: another program may have it open just then
    fn open(owner: HWND) -> bool {
        for _ in 0..10 {
            // SAFETY: `owner` is null or this module's own window
            if unsafe { OpenClipboard(owner) } != 0 {
                return true;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        false
    }

    /// The bytes the clipboard holds in a format, if it holds any. The clipboard must be open.
    unsafe fn bytes(format: u32) -> Option<Vec<u8>> {
        // SAFETY: the clipboard is open (the caller's promise); the handle is the clipboard's, locked only while it's
        // read, and read no further than its size
        unsafe {
            if format == 0 || IsClipboardFormatAvailable(format) == 0 {
                return None;
            }
            let handle = GetClipboardData(format);
            if handle.is_null() {
                return None;
            }
            let data = GlobalLock(handle) as *const u8;
            if data.is_null() {
                return None;
            }
            let copy = std::slice::from_raw_parts(data, GlobalSize(handle)).to_vec();
            GlobalUnlock(handle);
            Some(copy)
        }
    }

    /// Give the clipboard bytes in a format. The clipboard must be open and emptied by this thread, or this the
    /// answer to its asking for the format.
    unsafe fn set(format: u32, data: &[u8]) {
        // SAFETY: the memory is sized for the data (at least a byte, as Windows refuses none) and is the clipboard's
        // once set, or freed here when it wasn't taken
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1));
            let to = if handle.is_null() { std::ptr::null_mut() } else { GlobalLock(handle) } as *mut u8;
            if to.is_null() {
                if !handle.is_null() {
                    GlobalFree(handle);
                }
                return;
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), to, data.len());
            GlobalUnlock(handle);
            if format == 0 || SetClipboardData(format, handle).is_null() {
                GlobalFree(handle);
            }
        }
    }

    /// Read one format off the clipboard, opening and closing it
    fn read(format: u32) -> Option<Vec<u8>> {
        let _one = crate::clipboard::one_at_a_time();
        if !open(std::ptr::null_mut()) {
            return None;
        }
        // SAFETY: the clipboard is open, and closed again whatever was read
        unsafe {
            let data = bytes(format);
            CloseClipboard();
            data
        }
    }

    /// Text as the clipboard holds it: UTF-16, with a NUL after it
    fn to_wide(text: &str) -> Vec<u8> {
        text.encode_utf16().chain([0]).flat_map(u16::to_le_bytes).collect()
    }

    /// The clipboard's UTF-16 as text, up to its NUL
    fn from_wide(data: &[u8]) -> String {
        let units: Vec<u16> = data.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).take_while(|unit| *unit != 0).collect();
        String::from_utf16_lossy(&units)
    }

    /// Paths as the clipboard holds files (DROPFILES): a header saying where the paths start and that they're
    /// UTF-16, then each path with a NUL after it, and a NUL after the last
    pub(super) fn to_drop(paths: &[PathBuf]) -> Vec<u8> {
        let mut data = Vec::new();
        data.extend_from_slice(&(DROPFILES as u32).to_le_bytes());
        // the point files were dropped at (none), then "not in the window's frame" and "wide characters"
        data.extend_from_slice(&[0; 12]);
        data.extend_from_slice(&1u32.to_le_bytes());
        for path in paths {
            data.extend(to_wide(&path.to_string_lossy()));
        }
        data.extend_from_slice(&[0, 0]);
        data
    }

    /// The paths in the clipboard's files (DROPFILES); none when it's not as expected (the old one-byte characters
    /// aren't read: Windows has given wide ones since Windows NT)
    pub(super) fn from_drop(data: &[u8]) -> Vec<PathBuf> {
        let number = |at: usize| data.get(at..at + 4).and_then(|bytes| bytes.try_into().ok()).map(u32::from_le_bytes);
        let (Some(start), Some(wide)) = (number(0), number(16)) else { return Vec::new() };
        let Some(paths) = data.get(start as usize..) else { return Vec::new() };
        if wide == 0 {
            return Vec::new();
        }
        let units: Vec<u16> = paths.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).collect();
        units.split(|unit| *unit == 0).take_while(|path| !path.is_empty()).map(|path| PathBuf::from(String::from_utf16_lossy(path))).collect()
    }

    /// Which forms the clipboard holds, without reading any
    pub fn kinds() -> Kinds {
        // SAFETY: asks whether a format is there, by its number; needs no open clipboard
        let has = |format: u32| format != 0 && unsafe { IsClipboardFormatAvailable(format) } != 0;
        Kinds { text: has(CF_UNICODETEXT), html: has(html_format()), rtf: has(rtf_format()), image: has(CF_DIB), files: has(CF_HDROP) }
    }

    /// The clipboard's text, if it has any
    pub fn text() -> Option<String> {
        read(CF_UNICODETEXT).map(|data| from_wide(&data)).filter(|text| !text.is_empty())
    }

    /// The clipboard's formatted text as "HTML Format", if it has any
    pub fn html() -> Option<Vec<u8>> {
        read(html_format())
    }

    /// The clipboard's formatted text as "Rich Text Format", if it has any
    pub fn rtf() -> Option<Vec<u8>> {
        read(rtf_format())
    }

    /// The clipboard's picture as a DIB, if it has one (Windows makes one from any bitmap copied)
    pub fn image() -> Option<Vec<u8>> {
        read(CF_DIB)
    }

    /// The files and folders on the clipboard, if any
    pub fn files() -> Vec<PathBuf> {
        read(CF_HDROP).map(|data| from_drop(&data)).unwrap_or_default()
    }

    /// Put `contents` on the clipboard in place of what it held, every form it has. Left as it was when the
    /// clipboard can't be opened (another program has it).
    pub fn put(contents: &Contents) {
        let one = crate::clipboard::one_at_a_time();
        if !open(std::ptr::null_mut()) {
            return;
        }
        // SAFETY: the clipboard is open and emptied by this thread, and closed again after
        unsafe {
            EmptyClipboard();
            if let Some(text) = &contents.text {
                set(CF_UNICODETEXT, &to_wide(text));
            }
            if let Some(html) = &contents.html {
                set(html_format(), html);
            }
            if let Some(rtf) = &contents.rtf {
                set(rtf_format(), rtf);
            }
            if let Some(image) = &contents.image {
                set(CF_DIB, image);
            }
            if !contents.files.is_empty() {
                set(CF_HDROP, &to_drop(&contents.files));
            }
            CloseClipboard();
        }
        mark_ours();
        drop(one);
    }

    /// What's on the clipboard now was put there by this app (here, or as files promised by
    /// [`crate::clipboard_files`]): it isn't a copy made on this computer
    pub fn mark_ours() {
        OURS.store(stamp().unwrap_or(0), Ordering::Relaxed);
    }

    /// A number that changes each time anything changes the clipboard
    pub fn stamp() -> Option<u32> {
        // SAFETY: takes nothing and returns the clipboard's sequence number
        Some(unsafe { GetClipboardSequenceNumber() })
    }

    /// Whether what's on the clipboard now was put there by this module, rather than copied on this computer
    pub fn is_ours() -> bool {
        stamp() == Some(OURS.load(Ordering::Relaxed))
    }

}

// This computer's clipboard, the real one (put back as it was after each test, in the forms read here): what's put on
// it is read back form by form. On Windows and the Mac, each through its own module above; Linux's clipboard passes
// text alone so far, which clipboard.rs's tests cover. What's compared is what both systems give back: Windows may
// round bytes up in size, and the Mac writes formatted text's header again, so HTML is compared as the HTML in it.
#[cfg(all(test, any(windows, target_os = "macos")))]
mod tests {
    use super::*;
    use crate::clipboard_forms::{html_format, html_from_format};

    /// What the clipboard holds now, to put back
    fn snapshot() -> Contents {
        Contents { text: text(), html: html(), rtf: rtf(), image: image(), files: files() }
    }

    /// A one-pixel picture as a DIB: a 40-byte header (1 × 1, 32 bits) and its pixel
    fn dib() -> Vec<u8> {
        let mut header = Vec::new();
        for (value, size) in [(40u32, 4), (1, 4), (1, 4), (1, 2), (32, 2), (0, 4), (4, 4), (0, 4), (0, 4), (0, 4), (0, 4)] {
            header.extend_from_slice(&value.to_le_bytes()[..size]);
        }
        header.extend_from_slice(&[0x10, 0x20, 0x30, 0xff]);
        header
    }

    /// What Windows adds to bytes it holds: it may round their size up, so only their start is compared
    fn starts_with(read: Option<Vec<u8>>, written: &[u8]) -> bool {
        read.is_some_and(|read| read.starts_with(written))
    }

    #[test]
    fn a_picture_and_formatted_text_put_on_the_clipboard_are_read_back() {
        let _held = crate::clipboard::hold_for_test();
        let before = snapshot();
        let as_rtf = b"{\\rtf1 {\\b bold}}".to_vec();
        put(&Contents {
            text: Some("bold \u{2013} text".into()),
            html: Some(html_format("<b>bold</b> \u{2013} text")),
            rtf: Some(as_rtf.clone()),
            image: Some(dib()),
            files: Vec::new(),
        });
        crate::clipboard::read_for_test("bold \u{2013} text");
        let (found, read_text, read_html, read_rtf, read_image, ours) = (kinds(), text(), html(), rtf(), image(), is_ours());
        put(&before);

        assert_eq!(found, Kinds { text: true, html: true, rtf: true, image: true, files: false });
        assert_eq!(read_text.as_deref(), Some("bold \u{2013} text"));
        assert_eq!(read_html.and_then(|html| html_from_format(&html)).as_deref(), Some("<b>bold</b> \u{2013} text"), "the HTML as it was put");
        assert!(starts_with(read_rtf, &as_rtf), "the RTF as it was put");
        assert!(starts_with(read_image, &dib()), "the picture as it was put");
        assert!(ours, "what was put here isn't a copy made on this computer");
    }

    #[test]
    fn files_on_the_clipboard_are_read_as_their_paths() {
        let _held = crate::clipboard::hold_for_test();
        let before = snapshot();
        // paths as each system writes them; the files needn't be there to be on the clipboard
        let paths = if cfg!(windows) {
            vec![PathBuf::from("C:\\Temp\\r\u{00e9}sum\u{00e9}.docx"), PathBuf::from("C:\\Temp\\Folder")]
        } else {
            vec![PathBuf::from("/tmp/sxt r\u{00e9}sum\u{00e9}.docx"), PathBuf::from("/tmp/sxt Folder")]
        };
        put(&Contents { files: paths.clone(), ..Contents::default() });
        let (found, read, ours) = (kinds(), files(), is_ours());
        put(&before);
        // the Mac holds each file's path as text beside it, as Finder's own copy does
        assert_eq!(Kinds { text: false, ..found }, Kinds { files: true, ..Kinds::default() });
        assert_eq!(read, paths);
        assert!(ours);
    }

    #[test]
    fn a_copy_made_on_this_computer_isnt_taken_for_one_put_here() {
        let _held = crate::clipboard::hold_for_test();
        let before = snapshot();
        put(&Contents { text: Some("from the remote desktop".into()), ..Contents::default() });
        let (ours, stamped) = (is_ours(), stamp());
        // copied in another program
        crate::clipboard::write("copied here");
        crate::clipboard::read_for_test("copied here");
        let (after, changed) = (is_ours(), stamp());
        put(&before);
        assert!(ours && !after, "a copy made here is told from what was put on the clipboard");
        assert_ne!(stamped, changed, "the clipboard's number changes with it");
    }
}
