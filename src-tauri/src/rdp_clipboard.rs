//! Remote Desktop's clipboard, as SuperXPuTTY's Clipboard option (RedirectClipboard): what's copied in the remote
//! desktop is on this computer's clipboard, and what's copied here pastes in the remote desktop. Text on every
//! system; on Windows and the Mac also pictures, formatted text (HTML and RTF) and files, both ways. Linux passes
//! text alone so far ([`crate::clipboard_contents`] says why).
//!
//! IronRDP's clipboard channel (CLIPRDR) takes a backend. Its client engine has one only for Windows, which wants a
//! window's message loop of its own, and a do-nothing stub elsewhere; so the engine's is turned off and this one is
//! attached as the session's own channel (`with_static_channel`), on the Mac, Windows and Linux alike. Its messages
//! go back through the engine's input channel, as the engine's own backend's do.
//!
//! How a copy passes, each way:
//!
//! - **Copied here.** The clipboard is looked at when the tab gets the keyboard ([`Shared::announce`]) and what it
//!   holds is offered by kind, nothing read yet. The remote desktop asks for a kind when something there pastes it
//!   (`on_format_data_request`). Files are offered as a list of names and sizes, folders walked; their contents are
//!   asked for a piece at a time (`on_file_contents_request`).
//! - **Copied there.** Windows tells the kinds (`on_remote_copy`). Text, a picture and formatted text are fetched at
//!   once, one after another, and put on this computer's clipboard together. Of files, their names are asked for.
//!   On Windows they go on the clipboard as a promise and each file comes over as the paste here reads it; on the
//!   Mac, whose Finder pastes only files on a disk, they're brought over first and then put on the clipboard
//!   ([`crate::clipboard_files`]).
//!
//! The engine ends the whole session when the channel refuses a message (one sent before the channel is ready, a
//! file request past a file's end, files offered to a server that doesn't take them). So nothing is sent that the
//! channel could refuse: [`State`] keeps what was agreed, and the checks the channel makes are made here first.

use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use ironrdp_client::rdp::RdpInputEvent;
use ironrdp_cliprdr::backend::{ClipboardMessage, CliprdrBackend, CliprdrBackendFactory};
use ironrdp_cliprdr::pdu::{
    ClipboardFileAttributes, ClipboardFormat, ClipboardFormatId, ClipboardFormatName, ClipboardGeneralCapabilityFlags, FileContentsFlags,
    FileContentsRequest, FileContentsResponse, FileDescriptor, FormatDataRequest, FormatDataResponse, LockDataId,
};
use ironrdp_core::IntoOwned as _;
use tokio::sync::mpsc;

use crate::clipboard_contents::{self as contents, Contents, Kinds};
use crate::clipboard_files::{Promised, Source};

/// The number formatted text is offered under as "HTML Format". Windows gives named formats numbers from 0xC000 up,
/// different on each computer; the name is what the two sides go by
const HTML_ID: u32 = 0xC101;
/// ... and as "Rich Text Format"
const RTF_ID: u32 = 0xC102;
/// The name of formatted text as word processors copy it
const RTF_NAME: &str = "Rich Text Format";
/// Windows' own picture formats besides a DIB: either means a DIB can be asked for
const CF_BITMAP: u32 = 2;
/// The longest a file's path among the files copied may be, in characters: the list's field is 260 with its NUL.
/// The channel leaves longer ones out of the list it keeps, so they're left out here first and the two stay in step
const LONGEST_NAME: usize = 259;
/// The most files and folders offered in one copy: a folder of a hundred thousand files would take minutes to
/// walk, on the session's thread
const MOST_FILES: usize = 10_000;
/// How deep folders are walked: deeper than any real folder, shallow enough to end a loop of links
const DEEPEST: usize = 32;
/// The most of a file given in one answer, whatever is asked for (Windows asks for far less)
const MOST_AT_ONCE: u32 = 4 * 1024 * 1024;
/// How long the remote desktop has to answer for a piece of a file before the paste here gives up on it
const ANSWER_WITHIN: Duration = Duration::from_secs(30);
/// The first position in a file that needs the two sides to have agreed on files over 2 GB
const HUGE: u64 = 0x8000_0000;
/// The most pieces of files asked for and not yet answered: the channel allows few, and one paste reads one at a
/// time
const MOST_WAITING: usize = 8;

/// Whether this system's clipboard passes more than text (pictures, formatted text, and files copied here): Windows
/// and the Mac ([`crate::clipboard_contents`]). Linux passes text alone so far.
const MORE_THAN_TEXT: bool = cfg!(any(windows, target_os = "macos"));

/// What a session's clipboard keeps between calls: what the two sides agreed, what was last passed, and the files
/// in play either way
#[derive(Debug, Default)]
struct State {
    /// The channel is set up: only then does it take a copy, a paste or a piece of a file
    ready: bool,
    /// Both sides pass files
    files: bool,
    /// ... and files over 2 GB
    huge: bool,
    /// The text last passed either way, where the clipboard has no number that changes with it (Linux):
    /// the same text isn't offered back and forth
    last: String,
    /// The clipboard's number when it was last offered to the remote desktop, or written from it (Windows, the Mac)
    announced: Option<u32>,
    /// The files offered to the remote desktop, in the order of the list it was given: a request names one by its
    /// place
    offered: Vec<PathBuf>,
    /// Lists the remote desktop asked to have kept (by the number it gave), so a paste still running there finishes
    /// after something else is copied here
    locked: HashMap<u32, Vec<PathBuf>>,
    /// The file last read for the remote desktop, kept open between its pieces
    open: Option<(PathBuf, File)>,
    /// Pieces of files asked of the remote desktop and not yet answered: where each answer goes, by the request's
    /// number
    waiting: HashMap<u32, std::sync::mpsc::Sender<Option<Vec<u8>>>>,
    /// The last request's number
    stream: u32,
}

/// Where the backend's messages go (the session's input channel, known once the engine has been made), and what
/// the session's clipboard keeps ([`State`])
#[derive(Clone, Debug, Default)]
pub struct Shared {
    pub sender: Arc<Mutex<Option<mpsc::UnboundedSender<RdpInputEvent>>>>,
    state: Arc<Mutex<State>>,
}

impl Shared {
    /// Send a message to the clipboard channel, once the engine's there to take it
    fn send(&self, message: ClipboardMessage) {
        if let Some(sender) = self.sender.lock().unwrap().as_ref() {
            let _ = sender.send(RdpInputEvent::Clipboard(message));
        }
    }

    /// What's kept for the session, whatever a panic elsewhere left of it
    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The tab got the keyboard, or ⌘V on the Mac: what was copied on this computer since is offered to the remote
    /// desktop (it asks for it when pasted); whether there was anything new. What came from the remote desktop
    /// isn't offered back to it.
    pub fn announce(&self) -> bool {
        let mut state = self.state();
        if !state.ready {
            return false;
        }
        match contents::stamp() {
            Some(stamp) => {
                if contents::is_ours() || state.announced == Some(stamp) {
                    return false;
                }
                state.announced = Some(stamp);
            }
            None => {
                let text = contents::text().unwrap_or_default();
                if text.is_empty() || state.last == text {
                    return false;
                }
                state.last = text;
            }
        }
        let kinds = contents::kinds();
        if kinds.files && state.files {
            let (list, paths) = describe(&contents::files());
            if !list.is_empty() {
                state.offered = paths;
                state.open = None;
                drop(state);
                self.send(ClipboardMessage::SendInitiateFileCopy(list));
                return true;
            }
        }
        let offered = formats(kinds);
        if offered.is_empty() {
            return false;
        }
        state.offered.clear();
        drop(state);
        self.send(ClipboardMessage::SendInitiateCopy(offered));
        true
    }
}

/// The kinds a clipboard holds as the formats the remote desktop is told: text and a picture by Windows' own
/// numbers, formatted text by name. Files are told another way (a list of them).
fn formats(kinds: Kinds) -> Vec<ClipboardFormat> {
    let mut formats = Vec::new();
    if kinds.text {
        formats.push(ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT));
    }
    if kinds.image {
        formats.push(ClipboardFormat::new(ClipboardFormatId::CF_DIB));
    }
    if kinds.html {
        formats.push(ClipboardFormat::new(ClipboardFormatId::new(HTML_ID)).with_name(ClipboardFormatName::HTML));
    }
    if kinds.rtf {
        formats.push(ClipboardFormat::new(ClipboardFormatId::new(RTF_ID)).with_name(ClipboardFormatName::new(RTF_NAME)));
    }
    formats
}

/// The files and folders copied, as the list the remote desktop is given and the path of each entry in it: a folder
/// is followed by everything in it, each with the folders it's under ("Reports\2026"). What can't be read, or has a
/// path too long for the list, is left out of both.
fn describe(copied: &[PathBuf]) -> (Vec<FileDescriptor>, Vec<PathBuf>) {
    /// Add `path` (under `folder` among the files copied) and, if it's a folder, what's in it
    fn add(path: &Path, folder: Option<&str>, depth: usize, list: &mut Vec<FileDescriptor>, paths: &mut Vec<PathBuf>) {
        let (Ok(meta), Some(name)) = (std::fs::metadata(path), path.file_name().map(|name| name.to_string_lossy().into_owned())) else { return };
        let within = match folder {
            Some(folder) => format!("{folder}\\{name}"),
            None => name.clone(),
        };
        if list.len() >= MOST_FILES || depth > DEEPEST || within.chars().count() > LONGEST_NAME {
            return;
        }
        let modified = u64::try_from(crate::rdp_drives::filetime(meta.modified())).unwrap_or(0);
        let mut entry = FileDescriptor::new(name).with_last_write_time(modified);
        if let Some(folder) = folder {
            entry = entry.with_relative_path(folder);
        }
        paths.push(path.to_path_buf());
        if !meta.is_dir() {
            list.push(entry.with_attributes(ClipboardFileAttributes::NORMAL).with_file_size(meta.len()));
            return;
        }
        list.push(entry.with_attributes(ClipboardFileAttributes::DIRECTORY));
        let mut inside: Vec<PathBuf> = std::fs::read_dir(path).into_iter().flatten().flatten().map(|entry| entry.path()).collect();
        inside.sort();
        for path in inside {
            add(&path, Some(&within), depth + 1, list, paths);
        }
    }
    let (mut list, mut paths) = (Vec::new(), Vec::new());
    for path in copied {
        add(path, None, 0, &mut list, &mut paths);
    }
    (list, paths)
}

/// Makes the backend for each connection (and reconnection)
pub struct Factory(pub Shared);

impl CliprdrBackendFactory for Factory {
    /// A backend for a new connection, sharing the channel; what the last connection agreed and had in play is
    /// forgotten
    fn build_cliprdr_backend(&self) -> Box<dyn CliprdrBackend> {
        Box::new(Backend::new(self.0.clone(), Instant::now()))
    }
}

/// What's being fetched from the remote desktop after a copy there
#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Text,
    Image,
    Html,
    Rtf,
    /// The list of files copied (their contents come later, as they're pasted)
    Files,
}

/// The clipboard channel's backend for one connection
#[derive(Debug)]
pub struct Backend {
    shared: Shared,
    /// When the channel started: what Windows' clipboard already held then isn't fetched
    started: Instant,
    /// What's still to fetch of the copy made in the remote desktop, the first being asked for now, each with the
    /// remote desktop's number for it
    fetching: VecDeque<(Kind, ClipboardFormatId)>,
    /// What's been fetched of it so far
    fetched: Contents,
    /// Where the remote desktop may put files of its own (it's told a folder as the channel starts; nothing here
    /// uses it)
    folder: String,
}

impl Backend {
    /// A backend for a connection started at `started`
    fn new(shared: Shared, started: Instant) -> Self {
        *shared.state() = State::default();
        Self { shared, started, fetching: VecDeque::new(), fetched: Contents::default(), folder: std::env::temp_dir().to_string_lossy().into_owned() }
    }

    /// Ask the remote desktop for the next kind of its copy, or, with them all in, put the copy on this computer's
    /// clipboard
    fn fetch_next(&mut self) {
        if let Some((_, format)) = self.fetching.front() {
            self.shared.send(ClipboardMessage::SendInitiatePaste(*format));
            return;
        }
        let fetched = std::mem::take(&mut self.fetched);
        if fetched == Contents::default() {
            return;
        }
        contents::put(&fetched);
        let mut state = self.shared.state();
        state.last = fetched.text.unwrap_or_default();
        state.announced = contents::stamp();
    }
}

impl Drop for Backend {
    /// The connection is gone: nothing more can be sent, and a paste here waiting on a file is told so
    fn drop(&mut self) {
        let mut state = self.shared.state();
        state.ready = false;
        state.waiting.clear();
    }
}

impl ironrdp_core::AsAny for Backend {
    /// Itself, as IronRDP asks every backend for
    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    /// Itself, as IronRDP asks every backend for
    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

impl CliprdrBackend for Backend {
    /// A folder on this computer the remote desktop is told of as the channel starts
    fn temporary_directory(&self) -> &str {
        &self.folder
    }

    /// Long format names, as Windows' own clients ask for; and on Windows and the Mac files, as streams of their contents (no
    /// paths), over 2 GB too, kept for a paste that's still running when something else is copied
    fn client_capabilities(&self) -> ClipboardGeneralCapabilityFlags {
        let files = ClipboardGeneralCapabilityFlags::STREAM_FILECLIP_ENABLED
            | ClipboardGeneralCapabilityFlags::FILECLIP_NO_FILE_PATHS
            | ClipboardGeneralCapabilityFlags::HUGE_FILE_SUPPORT_ENABLED
            | ClipboardGeneralCapabilityFlags::CAN_LOCK_CLIPDATA;
        if MORE_THAN_TEXT {
            ClipboardGeneralCapabilityFlags::USE_LONG_FORMAT_NAMES | files
        } else {
            ClipboardGeneralCapabilityFlags::USE_LONG_FORMAT_NAMES
        }
    }

    /// The channel is set up: copies and pastes can pass
    fn on_ready(&mut self) {
        self.shared.state().ready = true;
    }

    /// The channel's start: what this computer's clipboard holds is offered, by kind. A list must go even when
    /// there's nothing (it's part of setting the channel up). Files can't be offered this early, so with files on
    /// the clipboard nothing is counted as offered, and they are when the tab next gets the keyboard.
    fn on_request_format_list(&mut self) {
        let kinds = contents::kinds();
        let mut state = self.shared.state();
        state.announced = if kinds.files { None } else { contents::stamp() };
        if contents::stamp().is_none() {
            state.last = contents::text().unwrap_or_default();
        }
        drop(state);
        self.shared.send(ClipboardMessage::SendInitiateCopy(formats(Kinds { files: false, ..kinds })));
    }

    /// What the two sides agreed: whether files pass, and files over 2 GB
    fn on_process_negotiated_capabilities(&mut self, capabilities: ClipboardGeneralCapabilityFlags) {
        let mut state = self.shared.state();
        state.files = MORE_THAN_TEXT && capabilities.contains(ClipboardGeneralCapabilityFlags::STREAM_FILECLIP_ENABLED);
        state.huge = capabilities.contains(ClipboardGeneralCapabilityFlags::HUGE_FILE_SUPPORT_ENABLED);
    }

    /// Copied in the remote desktop: its text, picture and formatted text are fetched at once, one after another,
    /// for this computer's clipboard; of files, only the list. Windows also announces what its clipboard holds as
    /// the session starts; that isn't a copy of the user's, so it doesn't replace this computer's clipboard
    fn on_remote_copy(&mut self, available_formats: &[ClipboardFormat]) {
        self.fetching.clear();
        self.fetched = Contents::default();
        let (ready, files) = {
            let state = self.shared.state();
            (state.ready, state.files)
        };
        if !ready || self.started.elapsed() < Duration::from_secs(3) {
            return;
        }
        let has = |id: u32| available_formats.iter().any(|format| format.id.value() == id);
        let named = |name: &str| available_formats.iter().find(|format| format.name.as_ref().is_some_and(|n| n.value() == name)).map(|format| format.id);
        match named(ClipboardFormatName::FILE_LIST.value()).filter(|_| files) {
            Some(list) => self.fetching.push_back((Kind::Files, list)),
            None => {
                if has(ClipboardFormatId::CF_UNICODETEXT.value()) {
                    self.fetching.push_back((Kind::Text, ClipboardFormatId::CF_UNICODETEXT));
                }
                // Linux's clipboard takes text alone so far
                if MORE_THAN_TEXT {
                    // a bitmap copied in any of Windows' forms can be had as a DIB, listed or not
                    if [ClipboardFormatId::CF_DIB.value(), ClipboardFormatId::CF_DIBV5.value(), CF_BITMAP].into_iter().any(has) {
                        self.fetching.push_back((Kind::Image, ClipboardFormatId::CF_DIB));
                    }
                    if let Some(html) = named(ClipboardFormatName::HTML.value()) {
                        self.fetching.push_back((Kind::Html, html));
                    }
                    if let Some(rtf) = named(RTF_NAME) {
                        self.fetching.push_back((Kind::Rtf, rtf));
                    }
                }
            }
        }
        self.fetch_next();
    }

    /// Pasted in the remote desktop: what this computer's clipboard holds of the kind asked for, read now
    fn on_format_data_request(&mut self, request: FormatDataRequest) {
        let data = match request.format.value() {
            id if id == ClipboardFormatId::CF_UNICODETEXT.value() => contents::text().map(|text| FormatDataResponse::new_unicode_string(&text)),
            id if id == ClipboardFormatId::CF_DIB.value() => contents::image().map(FormatDataResponse::new_data),
            HTML_ID => contents::html().map(FormatDataResponse::new_data),
            RTF_ID => contents::rtf().map(FormatDataResponse::new_data),
            _ => None,
        };
        if self.shared.state().ready {
            self.shared.send(ClipboardMessage::SendFormatData(data.unwrap_or_else(FormatDataResponse::new_error).into_owned()));
        }
    }

    /// A kind of the remote desktop's copy has come: it's kept, and the next asked for (the last puts them all on
    /// this computer's clipboard). One that couldn't be had is passed over.
    fn on_format_data_response(&mut self, response: FormatDataResponse<'_>) {
        let Some((kind, _)) = self.fetching.pop_front() else { return };
        if !response.is_error() {
            match kind {
                Kind::Text => self.fetched.text = response.to_unicode_string().ok().map(|text| text.trim_end_matches('\0').to_string()),
                Kind::Image => self.fetched.image = Some(response.data().to_vec()),
                Kind::Html => self.fetched.html = Some(response.data().to_vec()),
                Kind::Rtf => self.fetched.rtf = Some(response.data().to_vec()),
                // a list of files comes to `on_remote_file_list`; here it's one the channel couldn't read
                Kind::Files => {}
            }
        }
        self.fetch_next();
    }

    /// The list of files copied in the remote desktop: put on this computer's clipboard as a promise, each file
    /// fetched as a paste here reads it, or on the Mac brought over now and put there when they've come; either
    /// way read through [`Remote`]
    fn on_remote_file_list(&mut self, files: &[FileDescriptor], clip_data_id: Option<u32>) {
        self.fetching.clear();
        let promised: Vec<Promised> = files
            .iter()
            .map(|file| Promised {
                path: match file.relative_path.as_deref().filter(|folder| !folder.is_empty()) {
                    Some(folder) => format!("{folder}\\{}", file.name),
                    None => file.name.clone(),
                },
                folder: file.attributes.is_some_and(|attributes| attributes.contains(ClipboardFileAttributes::DIRECTORY)),
                size: file.file_size.unwrap_or(0),
                modified: file.last_write_time,
            })
            .collect();
        let sizes = files.iter().map(|file| file.file_size).collect();
        let source = Arc::new(Remote { shared: self.shared.clone(), lock: clip_data_id, sizes });
        if !promised.is_empty() && crate::clipboard_files::offer(promised, source) {
            contents::mark_ours();
            self.shared.state().announced = contents::stamp();
        }
    }

    /// The remote desktop is pasting a file copied here: its size, or a piece of it
    fn on_file_contents_request(&mut self, request: FileContentsRequest) {
        let mut state = self.shared.state();
        let answer = file_piece(&mut state, &request).unwrap_or_else(|| FileContentsResponse::new_error(request.stream_id));
        let ready = state.ready;
        drop(state);
        if ready {
            self.shared.send(ClipboardMessage::SendFileContentsResponse(answer));
        }
    }

    /// A piece of a file being pasted here has come from the remote desktop: it goes to the paste waiting for it
    fn on_file_contents_response(&mut self, response: FileContentsResponse<'_>) {
        if let Some(waiting) = self.shared.state().waiting.remove(&response.stream_id()) {
            let _ = waiting.send((!response.is_error()).then(|| response.data().to_vec()));
        }
    }

    /// The remote desktop is pasting files copied here and asks that the list be kept under this number, whatever
    /// is copied here meanwhile
    fn on_lock(&mut self, data_id: LockDataId) {
        let mut state = self.shared.state();
        let offered = state.offered.clone();
        state.locked.insert(data_id.0, offered);
    }

    /// ... and is done with it
    fn on_unlock(&mut self, data_id: LockDataId) {
        self.shared.state().locked.remove(&data_id.0);
    }
}

/// What the remote desktop asked of a file copied here: its size, or the bytes asked for (fewer at its end). None
/// when the file isn't one that was offered, or can't be read now.
fn file_piece(state: &mut State, request: &FileContentsRequest) -> Option<FileContentsResponse<'static>> {
    let place = usize::try_from(request.index).ok()?;
    let list = request.data_id.and_then(|id| state.locked.get(&id)).unwrap_or(&state.offered);
    let path = list.get(place)?.clone();
    if request.flags.contains(FileContentsFlags::SIZE) {
        return Some(FileContentsResponse::new_size_response(request.stream_id, std::fs::metadata(&path).ok()?.len()));
    }
    if state.open.as_ref().is_none_or(|(open, _)| *open != path) {
        state.open = Some((path.clone(), File::open(&path).ok()?));
    }
    let (_, file) = state.open.as_mut()?;
    file.seek(SeekFrom::Start(request.position)).ok()?;
    let mut data = vec![0u8; request.requested_size.min(MOST_AT_ONCE) as usize];
    let mut filled = 0;
    while filled < data.len() {
        match file.read(&mut data[filled..]) {
            Ok(0) => break,
            Ok(read) => filled += read,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
    data.truncate(filled);
    Some(FileContentsResponse::new_data_response(request.stream_id, data))
}

/// The files copied in the remote desktop, read a piece at a time as a paste here asks: each piece is asked of the
/// remote desktop over the clipboard channel and waited for
struct Remote {
    shared: Shared,
    /// The number the remote desktop's copy is kept under there, if the two sides agreed to keep copies
    lock: Option<u32>,
    /// Each file's size, where the remote desktop said
    sizes: Vec<Option<u64>>,
}

impl Source for Remote {
    /// A piece of a file: asked for, and waited for up to [`ANSWER_WITHIN`]. Only what the channel will take is
    /// asked: a piece within the file's size, and past 2 GB only where that was agreed.
    fn read(&self, index: usize, offset: u64, length: u32) -> Option<Vec<u8>> {
        let size = *self.sizes.get(index)?;
        let length = match size {
            Some(size) => u32::try_from(size.saturating_sub(offset).min(u64::from(length))).ok()?,
            None => length,
        };
        if length == 0 {
            return Some(Vec::new());
        }
        let (answer, answered) = std::sync::mpsc::channel();
        let stream = {
            let mut state = self.shared.state();
            if !state.ready || state.waiting.len() >= MOST_WAITING || (!state.huge && offset.saturating_add(u64::from(length)) > HUGE) {
                return None;
            }
            state.stream = state.stream.wrapping_add(1);
            let stream = state.stream;
            state.waiting.insert(stream, answer);
            stream
        };
        self.shared.send(ClipboardMessage::SendFileContentsRequest(FileContentsRequest {
            stream_id: stream,
            index: i32::try_from(index).ok()?,
            flags: FileContentsFlags::RANGE,
            position: offset,
            requested_size: length,
            data_id: self.lock,
        }));
        let piece = answered.recv_timeout(ANSWER_WITHIN).ok().flatten();
        self.shared.state().waiting.remove(&stream);
        piece
    }
}

/// The Mac's general pasteboard, as text, used by one thread at a time ([`crate::clipboard::one_at_a_time`]).
/// Windows' clipboard is in [`crate::clipboard`], which both systems go through
#[cfg(target_os = "macos")]
pub(crate) mod mac {
    use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
    use objc2_foundation::NSString;

    /// The text on the Mac's clipboard, if any
    pub fn read() -> Option<String> {
        let _one = crate::clipboard::one_at_a_time();
        let board = NSPasteboard::generalPasteboard();
        let text = unsafe { board.stringForType(NSPasteboardTypeString) }?;
        Some(text.to_string())
    }

    /// Put text on the Mac's clipboard, in place of what it held (for the tests: [`crate::clipboard::write`])
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn write(text: &str) {
        let _one = crate::clipboard::one_at_a_time();
        let board = NSPasteboard::generalPasteboard();
        board.clearContents();
        let _ = unsafe { board.setString_forType(&NSString::from_str(text), NSPasteboardTypeString) };
    }
}

#[cfg(test)]
mod tests {
    //! The backend driven as IronRDP's clipboard channel drives it, against this computer's real clipboard (put back
    //! after each test); the session's input channel is a channel the test reads. On the Mac and Windows, the two
    //! systems with a clipboard to test against; formatted text is compared as the HTML in it, since the Mac writes
    //! its header again.
    use super::*;

    /// A backend for a connection made `age` ago, set up and agreed on files, and where its messages for the remote
    /// desktop arrive
    fn backend(age: Duration) -> (Backend, mpsc::UnboundedReceiver<RdpInputEvent>) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let shared = Shared::default();
        *shared.sender.lock().unwrap() = Some(sender);
        let mut backend = Backend::new(shared, Instant::now() - age);
        backend.on_process_negotiated_capabilities(backend.client_capabilities());
        backend.on_ready();
        (backend, receiver)
    }

    /// The backend's next message for the remote desktop
    fn message(receiver: &mut mpsc::UnboundedReceiver<RdpInputEvent>) -> ClipboardMessage {
        match receiver.try_recv() {
            Ok(RdpInputEvent::Clipboard(message)) => message,
            _ => panic!("the backend sent no clipboard message"),
        }
    }

    /// Windows to a Windows Server lost the clipboard both ways: the backend read and wrote only the Mac's. Only
    /// where there's a clipboard: the Mac and Windows
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn text_copied_on_this_computer_pastes_in_the_remote_desktop() {
        let _held = crate::clipboard::hold_for_test();
        let before = crate::clipboard::read().unwrap_or_default();
        crate::clipboard::write("show running-config – ünïcode");
        let copied = crate::clipboard::read_for_test("show running-config – ünïcode");
        let (mut backend, mut receiver) = backend(Duration::ZERO);

        backend.on_request_format_list();
        let offered = message(&mut receiver);
        backend.on_format_data_request(FormatDataRequest { format: ClipboardFormatId::CF_UNICODETEXT });
        let pasted = message(&mut receiver);
        crate::clipboard::write(&before);

        assert_eq!(copied, "show running-config – ünïcode");
        let ClipboardMessage::SendInitiateCopy(formats) = offered else { panic!("the text wasn't offered") };
        assert!(formats.iter().any(|f| f.id == ClipboardFormatId::CF_UNICODETEXT), "text is offered to the remote desktop");
        let ClipboardMessage::SendFormatData(response) = pasted else { panic!("the paste wasn't answered") };
        assert_eq!(response.to_unicode_string().unwrap().trim_end_matches('\0'), "show running-config – ünïcode");
    }

    /// The other way. Only where there's a clipboard: the Mac and Windows
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn text_copied_in_the_remote_desktop_is_on_this_computers_clipboard() {
        let _held = crate::clipboard::hold_for_test();
        let before = crate::clipboard::read().unwrap_or_default();
        let (mut backend, mut receiver) = backend(Duration::from_secs(10));

        backend.on_remote_copy(&[ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]);
        let asked = message(&mut receiver);
        backend.on_format_data_response(FormatDataResponse::new_unicode_string("GigabitEthernet1/0/1 – ünïcode"));
        let read = crate::clipboard::read_for_test("GigabitEthernet1/0/1 – ünïcode");
        crate::clipboard::write(&before);

        assert!(matches!(asked, ClipboardMessage::SendInitiatePaste(ClipboardFormatId::CF_UNICODETEXT)), "the copy's text is asked for");
        assert_eq!(read, "GigabitEthernet1/0/1 – ünïcode");
    }

    /// Nothing may go to a channel that isn't set up: the engine ends the session when the channel refuses a message
    #[test]
    fn nothing_is_asked_of_a_channel_that_isnt_ready() {
        let (sender, mut receiver) = mpsc::unbounded_channel();
        let shared = Shared::default();
        *shared.sender.lock().unwrap() = Some(sender);
        let mut backend = Backend::new(shared.clone(), Instant::now() - Duration::from_secs(10));
        backend.on_remote_copy(&[ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT)]);
        backend.on_format_data_request(FormatDataRequest { format: ClipboardFormatId::CF_UNICODETEXT });
        assert!(!shared.announce());
        assert!(receiver.try_recv().is_err(), "a message went to a channel that wasn't ready");
    }

    /// What this computer's clipboard holds now, to put back
    #[cfg(any(windows, target_os = "macos"))]
    fn snapshot() -> Contents {
        Contents { text: contents::text(), html: contents::html(), rtf: contents::rtf(), image: contents::image(), files: contents::files() }
    }

    /// A one-pixel picture as a DIB: a 40-byte header (1 × 1, 32 bits) and its pixel
    #[cfg(any(windows, target_os = "macos"))]
    fn dib() -> Vec<u8> {
        let mut header = Vec::new();
        for (value, size) in [(40u32, 4), (1, 4), (1, 4), (1, 2), (32, 2), (0, 4), (4, 4), (0, 4), (0, 4), (0, 4), (0, 4)] {
            header.extend_from_slice(&value.to_le_bytes()[..size]);
        }
        header.extend_from_slice(&[0x10, 0x20, 0x30, 0xff]);
        header
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn a_picture_and_formatted_text_copied_here_paste_in_the_remote_desktop() {
        let _held = crate::clipboard::hold_for_test();
        let before = snapshot();
        let html = crate::clipboard_forms::html_format("<b>bold</b>");
        let rtf = b"{\\rtf1 {\\b bold}}".to_vec();
        let (mut backend, mut receiver) = backend(Duration::ZERO);
        contents::put(&Contents { text: Some("bold".into()), html: Some(html.clone()), rtf: Some(rtf.clone()), image: Some(dib()), files: Vec::new() });
        // Windows 11 has the clipboard without its new contents for an instant after a copy (clipboard.rs)
        crate::clipboard::read_for_test("bold");
        let kinds = contents::kinds();
        let offered = formats(kinds);
        let mut pasted = Vec::new();
        for format in &offered {
            backend.on_format_data_request(FormatDataRequest { format: format.id });
            let ClipboardMessage::SendFormatData(response) = message(&mut receiver) else { panic!("the paste wasn't answered") };
            assert!(!response.is_error(), "{format:?} couldn't be pasted");
            pasted.push(response.data().to_vec());
        }
        contents::put(&before);

        let names: Vec<Option<&str>> = offered.iter().map(|format| format.name.as_ref().map(|name| name.value())).collect();
        assert_eq!(names, [None, None, Some("HTML Format"), Some("Rich Text Format")]);
        assert_eq!((offered[0].id, offered[1].id), (ClipboardFormatId::CF_UNICODETEXT, ClipboardFormatId::CF_DIB));
        assert!(pasted[1].starts_with(&dib()), "the picture as it was copied");
        assert_eq!(crate::clipboard_forms::html_from_format(&pasted[2]).as_deref(), Some("<b>bold</b>"), "the HTML as it was copied");
        assert!(pasted[3].starts_with(&rtf), "the RTF as it was copied");
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn a_picture_and_formatted_text_copied_in_the_remote_desktop_are_on_this_computers_clipboard() {
        let _held = crate::clipboard::hold_for_test();
        let before = snapshot();
        let html = crate::clipboard_forms::html_format("<i>italic</i>");
        let (mut backend, mut receiver) = backend(Duration::from_secs(10));
        // as Windows lists a copy from a browser: its own numbers for the named formats, and a bitmap with no DIB
        let remote_html = ClipboardFormatId::new(0xC0A1);
        backend.on_remote_copy(&[
            ClipboardFormat::new(ClipboardFormatId::CF_UNICODETEXT),
            ClipboardFormat::new(ClipboardFormatId::new(CF_BITMAP)),
            ClipboardFormat::new(remote_html).with_name(ClipboardFormatName::HTML),
        ]);
        // each kind is asked for once the one before has come
        let mut asked = Vec::new();
        for answer in [FormatDataResponse::new_unicode_string("italic"), FormatDataResponse::new_data(dib()), FormatDataResponse::new_data(html.clone())] {
            let ClipboardMessage::SendInitiatePaste(format) = message(&mut receiver) else { panic!("the next kind wasn't asked for") };
            asked.push(format);
            backend.on_format_data_response(answer);
        }
        crate::clipboard::read_for_test("italic");
        let (found, text, picture, formatted, ours) = (contents::kinds(), contents::text(), contents::image(), contents::html(), contents::is_ours());
        let offered_back = backend.shared.announce();
        contents::put(&before);

        assert_eq!(asked, [ClipboardFormatId::CF_UNICODETEXT, ClipboardFormatId::CF_DIB, remote_html]);
        assert_eq!(found, Kinds { text: true, html: true, rtf: false, image: true, files: false });
        assert_eq!(text.as_deref(), Some("italic"));
        assert!(picture.is_some_and(|picture| picture.starts_with(&dib())));
        assert_eq!(formatted.and_then(|formatted| crate::clipboard_forms::html_from_format(&formatted)).as_deref(), Some("<i>italic</i>"));
        assert!(ours && !offered_back, "what came from the remote desktop isn't offered back to it");
    }

    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn files_copied_here_are_listed_for_the_remote_desktop_and_read_by_it_piece_by_piece() {
        let _held = crate::clipboard::hold_for_test();
        let before = snapshot();
        let folder = std::env::temp_dir().join(format!("sxt-rdp-clipboard-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("Reports").join("2026")).unwrap();
        std::fs::write(folder.join("notes.txt"), "hello from this computer").unwrap();
        std::fs::write(folder.join("Reports").join("2026").join("q3.csv"), "a,b\n1,2\n").unwrap();
        let (mut backend, mut receiver) = backend(Duration::ZERO);
        // copied in File Explorer: a file and a folder
        contents::put(&Contents { files: vec![folder.join("notes.txt"), folder.join("Reports")], ..Contents::default() });
        let (list, paths) = describe(&contents::files());
        backend.shared.state().offered = paths;

        let named: Vec<(String, Option<String>, Option<u64>, bool)> = list
            .iter()
            .map(|file| {
                let folder = file.attributes.is_some_and(|a| a.contains(ClipboardFileAttributes::DIRECTORY));
                (file.name.clone(), file.relative_path.clone(), file.file_size, folder)
            })
            .collect();
        let ask = |backend: &mut Backend, receiver: &mut mpsc::UnboundedReceiver<RdpInputEvent>, index: i32, flags, position, requested_size, data_id| {
            backend.on_file_contents_request(FileContentsRequest { stream_id: 9, index, flags, position, requested_size, data_id });
            let ClipboardMessage::SendFileContentsResponse(response) = message(receiver) else { panic!("the request wasn't answered") };
            response
        };
        let size = ask(&mut backend, &mut receiver, 0, FileContentsFlags::SIZE, 0, 8, None);
        let start = ask(&mut backend, &mut receiver, 0, FileContentsFlags::RANGE, 0, 5, None);
        let rest = ask(&mut backend, &mut receiver, 0, FileContentsFlags::RANGE, 11, 4096, None);
        let nested = ask(&mut backend, &mut receiver, 3, FileContentsFlags::RANGE, 0, 4096, None);
        let beyond = ask(&mut backend, &mut receiver, 99, FileContentsFlags::RANGE, 0, 16, None);
        // the remote desktop keeps the list while its paste runs, though something else is copied here meanwhile
        backend.on_lock(LockDataId(5));
        backend.shared.state().offered.clear();
        let kept = ask(&mut backend, &mut receiver, 0, FileContentsFlags::RANGE, 0, 5, Some(5));
        backend.on_unlock(LockDataId(5));
        let let_go = ask(&mut backend, &mut receiver, 0, FileContentsFlags::RANGE, 0, 5, Some(5));
        contents::put(&before);
        let _ = std::fs::remove_dir_all(&folder);

        assert_eq!(
            named,
            [
                ("notes.txt".into(), None, Some(24), false),
                ("Reports".into(), None, None, true),
                ("2026".into(), Some("Reports".into()), None, true),
                ("q3.csv".into(), Some("Reports\\2026".into()), Some(8), false),
            ]
        );
        assert_eq!(size.data_as_size().unwrap(), 24);
        assert_eq!((start.data(), rest.data()), (&b"hello"[..], &b"this computer"[..]));
        assert_eq!(nested.data(), b"a,b\n1,2\n");
        assert!(beyond.is_error(), "a file that wasn't offered");
        assert_eq!(kept.data(), b"hello");
        assert!(let_go.is_error(), "the list is let go when the remote desktop is done with it");
    }

    /// A piece of a file is only asked of the remote desktop when the channel will take the request: the engine
    /// ends the session otherwise
    #[test]
    fn a_file_in_the_remote_desktop_is_read_piece_by_piece_and_never_past_its_end() {
        let (backend, mut receiver) = backend(Duration::from_secs(10));
        let shared = backend.shared.clone();
        let remote = Arc::new(Remote { shared: shared.clone(), lock: Some(7), sizes: vec![Some(10), None] });
        // the remote desktop, answering each request with the bytes asked for of "0123456789"
        let answering = std::sync::Arc::new(Mutex::new(backend));
        let server = {
            let answering = Arc::clone(&answering);
            std::thread::spawn(move || {
                let mut asked = Vec::new();
                while let Some(RdpInputEvent::Clipboard(ClipboardMessage::SendFileContentsRequest(request))) = receiver.blocking_recv() {
                    let from = request.position as usize;
                    let data = b"0123456789"[from..(from + request.requested_size as usize).min(10)].to_vec();
                    asked.push((request.index, request.position, request.requested_size, request.data_id));
                    answering.lock().unwrap().on_file_contents_response(FileContentsResponse::new_data_response(request.stream_id, data));
                }
                asked
            })
        };
        let first = remote.read(0, 0, 4);
        let last = remote.read(0, 8, 4096);
        let at_end = remote.read(0, 10, 4096);
        let not_listed = remote.read(5, 0, 4);
        // past 2 GB without both sides having agreed to it
        shared.state().huge = false;
        let too_far = remote.read(1, HUGE, 16);
        // and once the connection is gone, a paste isn't left waiting
        drop(answering.lock().unwrap().shared.sender.lock().unwrap().take());
        shared.state().ready = false;
        let after = remote.read(0, 0, 4);
        let asked = server.join().unwrap();

        assert_eq!((first.as_deref(), last.as_deref()), (Some(&b"0123"[..]), Some(&b"89"[..])));
        assert_eq!(at_end.as_deref(), Some(&b""[..]), "nothing is asked for at a file's end");
        assert_eq!((not_listed, too_far, after), (None, None, None));
        assert_eq!(asked, [(0, 0, 4, Some(7)), (0, 8, 2, Some(7))], "only what the channel takes was asked");
    }
}
