//! Files copied in a remote desktop, on this computer's clipboard to paste.
//!
//! **On Windows, as a promise**: their names are there to paste, and each file's contents are fetched from the
//! remote desktop only as the paste reads them. A list of paths (CF_HDROP, [`crate::clipboard_contents`]) can't be a
//! promise: File Explorer asks for it the moment the clipboard changes, so every copy of a large file in the remote
//! desktop would fetch it at once, pasted or not. Remote Desktop Connection puts the files' names on the clipboard
//! instead (FileGroupDescriptorW) with their contents as streams (FileContents), which Windows only reads when
//! something pastes them; File Explorer then shows its own progress and can be cancelled. That takes Windows' OLE
//! clipboard and a data object ([`ole`]).
//!
//! OLE calls a data object on the thread that made it, through that thread's messages. So the object is made, and
//! lives, on a thread of this module's own, which does nothing but pass messages on: a paste here waits there for
//! each piece of a file to come from the remote desktop, and holds up nothing else.
//!
//! **On the Mac, brought over first** ([`brought`]). Finder pastes files that are on a disk; it takes no promise
//! from the clipboard (a promise is for a drag). So the files are fetched as they're copied, into a folder of
//! their own among the temporary files, on a thread that holds up nothing, and are put on the clipboard once they're
//! all there. The cost is the one Windows avoids: a copy in the remote desktop is fetched whether it's pasted here
//! or not. So a copy over [`brought::MOST`] isn't brought over (the log says so; the Drives option is for those),
//! and a newer copy, there or here, ends the fetching of the one before.
//!
//! On Linux [`offer`] says no, and files copied in the remote desktop aren't put on the clipboard.

#[cfg(any(not(any(windows, target_os = "macos")), test))]
use std::sync::Arc;

/// A file or folder copied in the remote desktop: where it goes under the place it's pasted, and what's known of it
#[cfg_attr(target_os = "linux", allow(dead_code))]
#[derive(Clone, Debug, PartialEq)]
pub struct Promised {
    /// Its path among the files copied, with `\` between folders ("Reports\2026\q3.xlsx")
    pub path: String,
    /// A folder, which has no contents of its own
    pub folder: bool,
    /// Its size in bytes (0 for a folder)
    pub size: u64,
    /// When it was last changed, as Windows counts time (100-nanosecond ticks since 1601), if the remote said
    pub modified: Option<u64>,
}

/// Where the files' contents come from: the remote desktop, asked a piece at a time. (Nothing asks on Linux, where
/// files aren't passed.)
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub trait Source: Send + Sync {
    /// `length` bytes of the file at `index` in the list promised, from `offset`; fewer at its end, and None when
    /// they can't be had (the session ended, the remote desktop's clipboard moved on). May wait on the network.
    fn read(&self, index: usize, offset: u64, length: u32) -> Option<Vec<u8>>;
}

/// Put `files` on the clipboard in place of what it held, to paste: on Windows a promise, nothing fetched until
/// something pastes them, when `source` is read; on the Mac they're fetched from `source` now, on a thread of
/// their own, and are on the clipboard when they've come. False when it can't be done (and always on Linux, for now).
#[cfg(not(any(windows, target_os = "macos")))]
pub fn offer(_files: Vec<Promised>, _source: Arc<dyn Source>) -> bool {
    false
}

#[cfg(target_os = "macos")]
pub use brought::offer;

/// Files copied in the remote desktop, brought over to this computer and then put on its clipboard as files on
/// disk: the Mac's way (the header says why). Built for the Mac, and for the tests on every system: but for the
/// clipboard at the end, it's files and folders.
#[cfg(any(target_os = "macos", test))]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
mod brought {
    use std::fs::File;
    use std::io::Write as _;
    use std::path::{Path, PathBuf};

    use super::{Promised, Source};

    /// The most brought over for one copy, in bytes: 2 GB. More than that is a long wait and a lot of disk for
    /// something that may never be pasted here
    pub const MOST: u64 = 2 * 1024 * 1024 * 1024;
    /// How much of a file is asked of the remote desktop at a time, as Windows' paste asks
    const PIECE: u32 = 512 * 1024;
    /// Counts the copies brought over: each has a folder of its own by this number, and one that's no longer the
    /// newest stops
    #[cfg(target_os = "macos")]
    static NEWEST: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

    /// Where a file copied goes under `folder`: its path there, each `\` a folder. None for a path that would
    /// leave the folder or isn't a name ("..", an empty part, a `/` or a NUL in a name): the list comes from the
    /// remote computer, and isn't trusted to stay where it's put.
    fn place(folder: &Path, path: &str) -> Option<PathBuf> {
        let mut place = folder.to_path_buf();
        for part in path.split('\\') {
            if part.is_empty() || part == "." || part == ".." || part.contains(['/', '\0']) {
                return None;
            }
            place.push(part);
        }
        Some(place)
    }

    /// Bring `files` over into `folder`, each read from `source` a piece at a time until its size is there (or, its
    /// size unknown, until the remote desktop has no more). Stops when `ended` says so (asked before each piece).
    /// @returns what was copied, as it's pasted: the files and folders that are in no folder of the list. None when
    /// it stopped, a piece couldn't be had, or a file couldn't be written; what was written is left to the caller.
    pub fn bring(files: &[Promised], source: &dyn Source, folder: &Path, ended: &dyn Fn() -> bool) -> Option<Vec<PathBuf>> {
        let mut copied = Vec::new();
        for (index, file) in files.iter().enumerate() {
            let place = place(folder, &file.path)?;
            if !file.path.contains('\\') {
                copied.push(place.clone());
            }
            if file.folder {
                std::fs::create_dir_all(&place).ok()?;
                continue;
            }
            std::fs::create_dir_all(place.parent()?).ok()?;
            let mut written = File::create(&place).ok()?;
            let mut at = 0u64;
            loop {
                if ended() {
                    return None;
                }
                let piece = source.read(index, at, PIECE)?;
                if piece.is_empty() {
                    break;
                }
                written.write_all(&piece).ok()?;
                at += piece.len() as u64;
            }
            // the remote desktop said how long it is: shorter is a file that wasn't all had
            if at < file.size {
                return None;
            }
            // dated as it is there, as a file copied in Finder keeps its date
            let modified = file.modified.and_then(|ticks| i64::try_from(ticks).ok()).and_then(crate::rdp_drives::system_time);
            if let Some(modified) = modified {
                let _ = written.set_modified(modified);
            }
        }
        Some(copied)
    }

    /// Bring `files` over from `source` on a thread of their own and put them on the clipboard when they've all
    /// come. True when that's begun; false, and nothing done, for a copy over [`MOST`] (the log says so). A newer
    /// copy in the remote desktop, or anything copied on this computer meanwhile, ends it, and what had come is
    /// deleted. The copy before this one is deleted too: its files were pasted or they weren't.
    #[cfg(target_os = "macos")]
    pub fn offer(files: Vec<Promised>, source: std::sync::Arc<dyn Source>) -> bool {
        use std::sync::atomic::Ordering;

        use crate::clipboard_contents as contents;

        let size: u64 = files.iter().map(|file| file.size).sum();
        if size > MOST {
            crate::applog::warn(format!(
                "The files copied in the remote desktop ({} MB) are too large to bring over by the clipboard; the Drives option is for those",
                size / (1024 * 1024)
            ));
            return false;
        }
        let number = NEWEST.fetch_add(1, Ordering::SeqCst) + 1;
        let folders = std::env::temp_dir().join(format!("SuperXTerminal-clipboard-{}", std::process::id()));
        let folder = folders.join(number.to_string());
        let copied_here = contents::stamp();
        let begun = std::thread::Builder::new().name("rdp-clipboard-files".into()).spawn(move || {
            let _ = std::fs::remove_dir_all(folders.join((number - 1).to_string()));
            let ended = || NEWEST.load(Ordering::SeqCst) != number || contents::stamp() != copied_here;
            let count = files.iter().filter(|file| !file.folder).count();
            match bring(&files, source.as_ref(), &folder, &ended) {
                Some(copied) if !ended() => {
                    contents::put(&contents::Contents { files: copied, ..contents::Contents::default() });
                    crate::applog::info(format!("{count} file(s) copied in the remote desktop are on this computer's clipboard"));
                }
                _ => {
                    let _ = std::fs::remove_dir_all(&folder);
                    if !ended() {
                        crate::applog::warn("The files copied in the remote desktop couldn't be brought over");
                    }
                }
            }
        });
        begun.is_ok()
    }
}

#[cfg(windows)]
pub use ole::offer;

/// Windows' OLE clipboard: the data object that is the promise, and the thread it lives on
#[cfg(windows)]
mod ole {
    use std::mem::ManuallyDrop;
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Duration;

    use windows::core::{implement, Ref, BOOL, HRESULT};
    use windows::Win32::Foundation::{
        DATA_S_SAMEFORMATETC, DV_E_FORMATETC, E_FAIL, E_NOTIMPL, E_POINTER, HGLOBAL, OLE_E_ADVISENOTSUPPORTED, STG_E_ACCESSDENIED, STG_E_INVALIDFUNCTION,
        S_FALSE, S_OK,
    };
    use windows::Win32::System::Com::{
        IAdviseSink, IDataObject, IDataObject_Impl, IEnumFORMATETC, IEnumSTATDATA, ISequentialStream_Impl, IStream, IStream_Impl, DATADIR_GET,
        DVASPECT_CONTENT, FORMATETC, LOCKTYPE, STATFLAG, STATSTG, STGC, STGMEDIUM, STGMEDIUM_0, STGTY_STREAM, STREAM_SEEK, STREAM_SEEK_CUR,
        STREAM_SEEK_END, STREAM_SEEK_SET, TYMED_HGLOBAL, TYMED_ISTREAM,
    };
    use windows::Win32::System::Ole::{OleInitialize, OleSetClipboard};
    use windows::Win32::UI::Shell::SHCreateStdEnumFmtEtc;
    use windows_sys::Win32::Foundation::GlobalFree;
    use windows_sys::Win32::System::DataExchange::RegisterClipboardFormatW;
    use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
    use windows_sys::Win32::System::Threading::GetCurrentThreadId;
    use windows_sys::Win32::UI::WindowsAndMessaging::{DispatchMessageW, GetMessageW, PeekMessageW, PostThreadMessageW, MSG, PM_NOREMOVE, WM_APP, WM_USER};

    use super::{Promised, Source};

    /// The thread's message to put the promise waiting in [`OFFERED`] on the clipboard
    const OFFER: u32 = WM_APP + 2;
    /// One entry of the list of files (FILEDESCRIPTORW), in bytes: flags, a class, a size and a point (none used),
    /// attributes, three times, the file's size, and its name in 260 UTF-16 characters
    const ENTRY: usize = 592;
    /// The characters a name has room for, with its NUL
    const NAME: usize = 260;
    /// An entry's flags: it says the attributes, the last change, the size; show progress while pasting; the name
    /// is UTF-16
    const HAS_ATTRIBUTES: u32 = 0x0000_0004;
    /// See [`HAS_ATTRIBUTES`]
    const HAS_CHANGED: u32 = 0x0000_0020;
    /// See [`HAS_ATTRIBUTES`]
    const HAS_SIZE: u32 = 0x0000_0040;
    /// See [`HAS_ATTRIBUTES`]
    const PROGRESS: u32 = 0x0000_4000;
    /// See [`HAS_ATTRIBUTES`]
    const UNICODE: u32 = 0x8000_0000;
    /// A folder's attribute, and a plain file's
    const FOLDER: u32 = 0x10;
    /// See [`FOLDER`]
    const PLAIN: u32 = 0x80;
    /// The most asked of the remote desktop for one piece of a file
    const PIECE: u32 = 512 * 1024;

    /// A promise on its way to the thread, and where to say whether it was put on the clipboard
    type Offer = (Vec<Promised>, Arc<dyn Source>, std::sync::mpsc::Sender<bool>);
    /// See [`Offer`]
    static OFFERED: Mutex<Option<Offer>> = Mutex::new(None);
    /// The thread the promises live on (its id, to post to); 0 when it couldn't be started
    static THREAD: OnceLock<u32> = OnceLock::new();

    /// See the other [`super::offer`]. The clipboard counts as this app's afterwards, when the caller marks it so.
    pub fn offer(files: Vec<Promised>, source: Arc<dyn Source>) -> bool {
        let thread = *THREAD.get_or_init(|| {
            let (ready, started) = std::sync::mpsc::channel();
            let spawned = std::thread::Builder::new().name("clipboard-files".into()).spawn(move || run(ready));
            spawned.ok().and_then(|_| started.recv().ok()).unwrap_or(0)
        });
        if thread == 0 {
            return false;
        }
        let (done, result) = std::sync::mpsc::channel();
        *OFFERED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some((files, source, done));
        // SAFETY: posts a message with no pointers in it to this module's own thread
        if unsafe { PostThreadMessageW(thread, OFFER, 0, 0) } == 0 {
            return false;
        }
        result.recv_timeout(Duration::from_secs(5)).unwrap_or(false)
    }

    /// The thread: readies OLE on it (which makes it a single-threaded apartment, as the OLE clipboard needs), then
    /// passes messages on for as long as the program runs. OLE's calls into a data object arrive as messages.
    fn run(ready: std::sync::mpsc::Sender<u32>) {
        // SAFETY: a zeroed MSG is a valid one to fill in; OLE is readied on this thread before anything uses it; the
        // peek makes Windows create the thread's message queue, so messages can be posted to it
        unsafe {
            let mut message: MSG = std::mem::zeroed();
            PeekMessageW(&mut message, std::ptr::null_mut(), WM_USER, WM_USER, PM_NOREMOVE);
            if OleInitialize(None).is_err() {
                let _ = ready.send(0);
                return;
            }
            let _ = ready.send(GetCurrentThreadId());
            while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                if message.hwnd.is_null() && message.message == OFFER {
                    put_on_clipboard();
                } else {
                    DispatchMessageW(&message);
                }
            }
        }
    }

    /// Put the promise waiting in [`OFFERED`] on the clipboard, and say whether it worked
    fn put_on_clipboard() {
        let Some((files, source, done)) = OFFERED.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take() else { return };
        let object: IDataObject = Files { files, source }.into();
        let _one = crate::clipboard::one_at_a_time();
        // SAFETY: OLE was readied on this thread; it keeps the object for as long as it's the clipboard's
        let put = unsafe { OleSetClipboard(&object) }.is_ok();
        let _ = done.send(put);
    }

    /// The number Windows gives a registered clipboard format, by its name; 0 if it can't
    fn registered(name: &str) -> u16 {
        let wide: Vec<u16> = name.encode_utf16().chain([0]).collect();
        // SAFETY: `wide` is a NUL-terminated UTF-16 string that lives through the call
        u16::try_from(unsafe { RegisterClipboardFormatW(wide.as_ptr()) }).unwrap_or(0)
    }

    /// The list of files' names and sizes
    pub(super) fn list_format() -> u16 {
        registered("FileGroupDescriptorW")
    }

    /// One file's contents, the file named by the request's index
    pub(super) fn contents_format() -> u16 {
        registered("FileContents")
    }

    /// Whether the files are to be copied or moved when pasted
    fn effect_format() -> u16 {
        registered("Preferred DropEffect")
    }

    /// The list of files as Windows has it (FILEGROUPDESCRIPTORW): how many, then an entry each
    pub(super) fn list(files: &[Promised]) -> Vec<u8> {
        let mut data = Vec::with_capacity(4 + files.len() * ENTRY);
        data.extend_from_slice(&u32::try_from(files.len()).unwrap_or(0).to_le_bytes());
        for file in files {
            let mut flags = HAS_ATTRIBUTES | PROGRESS | UNICODE;
            if !file.folder {
                flags |= HAS_SIZE;
            }
            if file.modified.is_some() {
                flags |= HAS_CHANGED;
            }
            let start = data.len();
            data.extend_from_slice(&flags.to_le_bytes());
            // the class, the size and the point: for other kinds of object
            data.extend_from_slice(&[0; 32]);
            data.extend_from_slice(&(if file.folder { FOLDER } else { PLAIN }).to_le_bytes());
            // made and last opened aren't said; last changed, as its two halves, low first
            data.extend_from_slice(&[0; 16]);
            data.extend_from_slice(&file.modified.unwrap_or(0).to_le_bytes());
            let size = if file.folder { 0 } else { file.size };
            data.extend_from_slice(&((size >> 32) as u32).to_le_bytes());
            data.extend_from_slice(&(size as u32).to_le_bytes());
            data.extend(file.path.encode_utf16().take(NAME - 1).flat_map(u16::to_le_bytes));
            data.resize(start + ENTRY, 0);
        }
        data
    }

    /// Bytes as memory the clipboard's taker frees (a movable global block); None when there's no memory
    fn global(data: &[u8]) -> Option<HGLOBAL> {
        // SAFETY: the block is sized for the data (at least a byte), written while locked, and handed on whole; it's
        // freed here if it couldn't be locked
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, data.len().max(1));
            if handle.is_null() {
                return None;
            }
            let to = GlobalLock(handle) as *mut u8;
            if to.is_null() {
                GlobalFree(handle);
                return None;
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), to, data.len());
            GlobalUnlock(handle);
            Some(HGLOBAL(handle))
        }
    }

    /// What's asked for, as memory or as a stream
    fn medium(tymed: i32, u: STGMEDIUM_0) -> STGMEDIUM {
        STGMEDIUM { tymed: tymed as u32, u, pUnkForRelease: ManuallyDrop::new(None) }
    }

    /// The promise: the files copied in the remote desktop, as the clipboard's data object
    #[implement(IDataObject)]
    struct Files {
        files: Vec<Promised>,
        source: Arc<dyn Source>,
    }

    impl Files_Impl {
        /// Whether a request is for something this object has, in a form it gives it in
        fn has(&self, request: &FORMATETC) -> bool {
            let memory = request.tymed & TYMED_HGLOBAL.0 as u32 != 0;
            let stream = request.tymed & TYMED_ISTREAM.0 as u32 != 0;
            request.dwAspect == DVASPECT_CONTENT.0
                && ((request.cfFormat == list_format() && memory)
                    || (request.cfFormat == effect_format() && memory)
                    || (request.cfFormat == contents_format() && stream && self.file(request.lindex).is_some()))
        }

        /// The file a request's index names, if it's a file and not a folder
        fn file(&self, index: i32) -> Option<(usize, &Promised)> {
            let index = usize::try_from(index).ok()?;
            self.files.get(index).filter(|file| !file.folder).map(|file| (index, file))
        }
    }

    impl IDataObject_Impl for Files_Impl {
        /// The list of files, how they're to be pasted (copied), or one file's contents as a stream
        fn GetData(&self, pformatetcin: *const FORMATETC) -> windows::core::Result<STGMEDIUM> {
            // SAFETY: OLE gives a valid request for the length of the call
            let request = unsafe { pformatetcin.as_ref() }.ok_or(E_POINTER)?;
            if !self.has(request) {
                return Err(DV_E_FORMATETC.into());
            }
            if request.cfFormat == contents_format() {
                let (index, file) = self.file(request.lindex).ok_or(DV_E_FORMATETC)?;
                let stream: IStream = Contents { index, size: file.size, source: Arc::clone(&self.source), at: Mutex::new(0) }.into();
                return Ok(medium(TYMED_ISTREAM.0, STGMEDIUM_0 { pstm: ManuallyDrop::new(Some(stream)) }));
            }
            // 1 is "copy": the files stay in the remote desktop
            let data = if request.cfFormat == list_format() { list(&self.files) } else { 1u32.to_le_bytes().to_vec() };
            let memory = global(&data).ok_or(E_FAIL)?;
            Ok(medium(TYMED_HGLOBAL.0, STGMEDIUM_0 { hGlobal: memory }))
        }

        /// Nothing is written into memory the asker brings
        fn GetDataHere(&self, _pformatetc: *const FORMATETC, _pmedium: *mut STGMEDIUM) -> windows::core::Result<()> {
            Err(E_NOTIMPL.into())
        }

        /// Whether [`Self::GetData`] would give what's asked
        fn QueryGetData(&self, pformatetc: *const FORMATETC) -> HRESULT {
            // SAFETY: OLE gives a valid request for the length of the call
            match unsafe { pformatetc.as_ref() } {
                Some(request) if self.has(request) => S_OK,
                Some(_) => DV_E_FORMATETC,
                None => E_POINTER,
            }
        }

        /// Every request is its own simplest form
        fn GetCanonicalFormatEtc(&self, _pformatectin: *const FORMATETC, pformatetcout: *mut FORMATETC) -> HRESULT {
            // SAFETY: OLE gives somewhere valid to write the answer, or null
            if let Some(out) = unsafe { pformatetcout.as_mut() } {
                out.ptd = std::ptr::null_mut();
            }
            DATA_S_SAMEFORMATETC
        }

        /// Nothing can be put into the promise (File Explorer tries, to note how a paste went)
        fn SetData(&self, _pformatetc: *const FORMATETC, _pmedium: *const STGMEDIUM, _frelease: BOOL) -> windows::core::Result<()> {
            Err(E_NOTIMPL.into())
        }

        /// What this object has: the list, how to paste, and contents
        fn EnumFormatEtc(&self, dwdirection: u32) -> windows::core::Result<IEnumFORMATETC> {
            if dwdirection != DATADIR_GET.0 as u32 {
                return Err(E_NOTIMPL.into());
            }
            let format = |format: u16, index: i32, tymed: i32| FORMATETC {
                cfFormat: format,
                ptd: std::ptr::null_mut(),
                dwAspect: DVASPECT_CONTENT.0,
                lindex: index,
                tymed: tymed as u32,
            };
            let formats =
                [format(list_format(), -1, TYMED_HGLOBAL.0), format(effect_format(), -1, TYMED_HGLOBAL.0), format(contents_format(), -1, TYMED_ISTREAM.0)];
            // SAFETY: the formats live through the call, which copies them
            unsafe { SHCreateStdEnumFmtEtc(&formats) }
        }

        /// Nobody is told of changes: a promise doesn't change
        fn DAdvise(&self, _pformatetc: *const FORMATETC, _advf: u32, _padvsink: Ref<IAdviseSink>) -> windows::core::Result<u32> {
            Err(OLE_E_ADVISENOTSUPPORTED.into())
        }

        /// See [`Self::DAdvise`]
        fn DUnadvise(&self, _dwconnection: u32) -> windows::core::Result<()> {
            Err(OLE_E_ADVISENOTSUPPORTED.into())
        }

        /// See [`Self::DAdvise`]
        fn EnumDAdvise(&self) -> windows::core::Result<IEnumSTATDATA> {
            Err(OLE_E_ADVISENOTSUPPORTED.into())
        }
    }

    /// One promised file's contents, read from the remote desktop as the paste reads the stream
    #[implement(IStream)]
    struct Contents {
        /// Its place in the list promised
        index: usize,
        size: u64,
        source: Arc<dyn Source>,
        /// Where the next read starts
        at: Mutex<u64>,
    }

    impl Contents_Impl {
        /// Where the next read starts
        fn at(&self) -> std::sync::MutexGuard<'_, u64> {
            self.at.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
        }
    }

    impl ISequentialStream_Impl for Contents_Impl {
        /// Read the next `cb` bytes: asked of the remote desktop a piece at a time, and waited for. Fewer at the
        /// file's end; a failure when the remote desktop can't give them (the paste then stops with an error).
        fn Read(&self, pv: *mut core::ffi::c_void, cb: u32, pcbread: *mut u32) -> HRESULT {
            if pv.is_null() {
                return E_POINTER;
            }
            let mut at = self.at();
            let mut read = 0u32;
            let mut failed = false;
            while read < cb {
                let Some(piece) = self.source.read(self.index, *at, (cb - read).min(PIECE)) else {
                    failed = true;
                    break;
                };
                if piece.is_empty() {
                    break;
                }
                // never more than was asked for, whatever came
                let took = piece.len().min((cb - read) as usize);
                // SAFETY: the asker's buffer holds `cb` bytes, of which `read + took` at most are written
                unsafe { std::ptr::copy_nonoverlapping(piece.as_ptr(), pv.cast::<u8>().add(read as usize), took) };
                read += took as u32;
                *at += took as u64;
            }
            // SAFETY: OLE gives somewhere valid to write the count, or null
            if let Some(count) = unsafe { pcbread.as_mut() } {
                *count = read;
            }
            match (failed && read == 0, read == cb) {
                (true, _) => E_FAIL,
                (false, true) => S_OK,
                (false, false) => S_FALSE,
            }
        }

        /// A promised file can't be written to
        fn Write(&self, _pv: *const core::ffi::c_void, _cb: u32, _pcbwritten: *mut u32) -> HRESULT {
            STG_E_ACCESSDENIED
        }
    }

    impl IStream_Impl for Contents_Impl {
        /// Move where the next read starts: from the file's start, from here, or from its end
        fn Seek(&self, dlibmove: i64, dworigin: STREAM_SEEK, plibnewposition: *mut u64) -> windows::core::Result<()> {
            let mut at = self.at();
            let from = match dworigin {
                STREAM_SEEK_SET => 0,
                STREAM_SEEK_CUR => *at,
                STREAM_SEEK_END => self.size,
                _ => return Err(STG_E_INVALIDFUNCTION.into()),
            };
            *at = from.checked_add_signed(dlibmove).ok_or(STG_E_INVALIDFUNCTION)?;
            // SAFETY: OLE gives somewhere valid to write the new place, or null
            if let Some(place) = unsafe { plibnewposition.as_mut() } {
                *place = *at;
            }
            Ok(())
        }

        /// A promised file can't be changed
        fn SetSize(&self, _libnewsize: u64) -> windows::core::Result<()> {
            Err(STG_E_ACCESSDENIED.into())
        }

        /// The asker reads and writes for itself
        fn CopyTo(&self, _pstm: Ref<IStream>, _cb: u64, _pcbread: *mut u64, _pcbwritten: *mut u64) -> windows::core::Result<()> {
            Err(E_NOTIMPL.into())
        }

        /// Nothing to save
        fn Commit(&self, _grfcommitflags: &STGC) -> windows::core::Result<()> {
            Ok(())
        }

        /// Nothing to undo
        fn Revert(&self) -> windows::core::Result<()> {
            Ok(())
        }

        /// Parts of a promised file aren't locked
        fn LockRegion(&self, _liboffset: u64, _cb: u64, _dwlocktype: &LOCKTYPE) -> windows::core::Result<()> {
            Err(STG_E_INVALIDFUNCTION.into())
        }

        /// See [`Self::LockRegion`]
        fn UnlockRegion(&self, _liboffset: u64, _cb: u64, _dwlocktype: u32) -> windows::core::Result<()> {
            Err(STG_E_INVALIDFUNCTION.into())
        }

        /// What's known of the stream: that it is one, and its size (no name: the list has it)
        fn Stat(&self, pstatstg: *mut STATSTG, _grfstatflag: &STATFLAG) -> windows::core::Result<()> {
            // SAFETY: OLE gives somewhere valid to write the answer
            let stat = unsafe { pstatstg.as_mut() }.ok_or(E_POINTER)?;
            *stat = STATSTG { r#type: STGTY_STREAM.0 as u32, cbSize: self.size, ..Default::default() };
            Ok(())
        }

        /// A second stream over the same file isn't given: the asker asks for the file again
        fn Clone(&self) -> windows::core::Result<IStream> {
            Err(E_NOTIMPL.into())
        }
    }
}

// Files brought over, as the Mac pastes them: the remote desktop is stood in for by bytes in memory, and the files
// land in a folder of the test's own. No clipboard, so the same on Windows, the Mac and Linux; the Mac's clipboard at
// the end is the test after.
#[cfg(test)]
mod brought_tests {
    use std::sync::Mutex;

    use super::brought::bring;
    use super::*;

    /// A remote desktop's files, and what was asked of them: (index, offset, length)
    struct Files(Vec<Vec<u8>>, Mutex<Vec<(usize, u64, u32)>>);

    impl Source for Files {
        fn read(&self, index: usize, offset: u64, length: u32) -> Option<Vec<u8>> {
            self.1.lock().unwrap().push((index, offset, length));
            let file = self.0.get(index)?;
            let from = (offset as usize).min(file.len());
            Some(file[from..(from + length as usize).min(file.len())].to_vec())
        }
    }

    /// A file in the list, `size` bytes long
    fn file(path: &str, size: u64) -> Promised {
        Promised { path: path.into(), folder: false, size, modified: None }
    }

    /// An empty folder of the test's own among the temporary files
    fn folder(name: &str) -> std::path::PathBuf {
        let folder = std::env::temp_dir().join(format!("sxt-brought-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(&folder).unwrap();
        folder
    }

    #[test]
    fn files_copied_in_the_remote_desktop_are_brought_over_whole_in_their_folders() {
        let into = folder("whole");
        // a file longer than one piece, a folder, a file two folders down, and an empty file
        let long: Vec<u8> = (0..700_000u32).map(|n| (n % 251) as u8).collect();
        let source = Files(vec![long.clone(), Vec::new(), Vec::new(), b"a,b\n1,2\n".to_vec(), Vec::new()], Mutex::new(Vec::new()));
        let list = [
            Promised { modified: Some(133_000_000_000_000_000), ..file("notes.bin", 700_000) },
            Promised { path: "Reports".into(), folder: true, size: 0, modified: None },
            Promised { path: "Reports\\2026".into(), folder: true, size: 0, modified: None },
            file("Reports\\2026\\q3.csv", 8),
            file("empty.txt", 0),
        ];
        let copied = bring(&list, &source, &into, &|| false);
        let read = |path: &str| std::fs::read(into.join(path)).ok();
        let (notes, nested, empty) = (read("notes.bin"), read("Reports/2026/q3.csv"), read("empty.txt"));
        let dated = std::fs::metadata(into.join("notes.bin")).and_then(|meta| meta.modified()).ok();
        let _ = std::fs::remove_dir_all(&into);

        // what's pasted is what was copied: the two files and the folder, not what's inside it
        assert_eq!(copied, Some(vec![into.join("notes.bin"), into.join("Reports"), into.join("empty.txt")]));
        assert_eq!(notes, Some(long));
        assert_eq!(nested.as_deref(), Some(&b"a,b\n1,2\n"[..]));
        assert_eq!(empty.as_deref(), Some(&b""[..]));
        // dated as in the remote desktop (June 2022, as Windows counts it)
        assert_eq!(dated, crate::rdp_drives::system_time(133_000_000_000_000_000));
    }

    #[test]
    fn a_file_that_doesnt_all_come_or_would_leave_its_folder_isnt_put_on_the_clipboard() {
        let into = folder("short");
        // the remote desktop has less of the file than it said
        let source = Files(vec![b"half".to_vec()], Mutex::new(Vec::new()));
        let short = bring(&[file("big.iso", 4096)], &source, &into, &|| false);
        // a newer copy ends it before anything is asked
        let ended = bring(&[file("big.iso", 4)], &source, &into, &|| true);
        let asked = source.1.lock().unwrap().len();
        // a list from the remote computer that names somewhere else
        let outside: Vec<Option<Vec<std::path::PathBuf>>> =
            ["..\\..\\evil.txt", "a\\..\\..\\evil.txt", "a/b.txt", "\\evil.txt"].iter().map(|path| bring(&[file(path, 4)], &source, &into, &|| false)).collect();
        let left = std::fs::read_dir(into.parent().unwrap()).unwrap().flatten().any(|entry| entry.file_name() == "evil.txt");
        let _ = std::fs::remove_dir_all(&into);

        assert_eq!((short, ended), (None, None));
        assert_eq!(asked, 2, "the short file was read to its end, and nothing after a newer copy");
        assert!(outside.iter().all(Option::is_none) && !left, "nothing is written outside the folder");
    }

    /// The Mac's clipboard at the end, the real one (put back as it was after): Finder's paste finds the files
    /// there, on disk. The Mac alone brings files over; Windows promises them (the test below).
    #[cfg(target_os = "macos")]
    #[test]
    fn files_copied_in_the_remote_desktop_are_on_the_macs_clipboard_once_brought_over() {
        use crate::clipboard_contents as contents;
        let _held = crate::clipboard::hold_for_test();
        let before = contents::Contents { text: contents::text(), html: contents::html(), rtf: contents::rtf(), image: contents::image(), files: contents::files() };
        let source = Arc::new(Files(vec![b"hello from the remote desktop".to_vec()], Mutex::new(Vec::new())));
        let offered = offer(vec![file("sxt-hello.txt", 29)], source);
        let mut pasted = Vec::new();
        for _ in 0..100 {
            pasted = contents::files();
            if pasted.iter().any(|path| path.ends_with("sxt-hello.txt")) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let read = pasted.first().and_then(|path| std::fs::read(path).ok());
        let ours = contents::is_ours();
        // too much to bring over: nothing is begun
        let too_large = offer(vec![file("huge.iso", brought::MOST + 1)], Arc::new(Files(Vec::new(), Mutex::new(Vec::new()))));
        contents::put(&before);
        if let Some(folder) = pasted.first().and_then(|path| path.parent()) {
            let _ = std::fs::remove_dir_all(folder);
        }

        assert!(offered && !too_large);
        assert_eq!(pasted.len(), 1, "the file is on the clipboard: {pasted:?}");
        assert_eq!(read.as_deref(), Some(&b"hello from the remote desktop"[..]));
        assert!(ours, "what came from the remote desktop isn't offered back to it");
    }
}

// The promise as a paste sees it: it's put on Windows' real clipboard (put back as it was after), and the test,
// as the program pasting, takes the clipboard's data object, reads the list of files from it, and reads a file's
// contents through its stream. The remote desktop is stood in for by bytes in memory that count how often they're
// asked for. Windows only: the promise is Windows' own (the Mac brings the files over, tested above).
#[cfg(all(test, windows))]
mod tests {
    use std::sync::atomic::{AtomicU32, Ordering};

    use windows::Win32::System::Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL, TYMED_ISTREAM};
    use windows::Win32::System::Ole::{OleGetClipboard, OleInitialize, ReleaseStgMedium};
    use windows_sys::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};

    use super::*;
    use crate::clipboard_contents::{self as contents, Contents};

    /// Files in the remote desktop: their contents by place in the list, and how many pieces were asked for
    struct Files {
        contents: Vec<Vec<u8>>,
        asked: AtomicU32,
    }

    impl Source for Files {
        fn read(&self, index: usize, offset: u64, length: u32) -> Option<Vec<u8>> {
            self.asked.fetch_add(1, Ordering::Relaxed);
            let file = self.contents.get(index)?;
            let from = (offset as usize).min(file.len());
            Some(file[from..(from + length as usize).min(file.len())].to_vec())
        }
    }

    /// A request of the clipboard's data object
    fn request(format: u16, index: i32, tymed: i32) -> FORMATETC {
        FORMATETC { cfFormat: format, ptd: std::ptr::null_mut(), dwAspect: DVASPECT_CONTENT.0, lindex: index, tymed: tymed as u32 }
    }

    /// The names in the list of files a paste is given, in order
    fn names(object: &IDataObject) -> Vec<String> {
        // SAFETY: the data is a block of memory the object gives, read while locked and within its size, then let go
        unsafe {
            let mut medium = object.GetData(&request(ole::list_format(), -1, TYMED_HGLOBAL.0)).expect("the list of files");
            let handle = medium.u.hGlobal.0;
            let data = std::slice::from_raw_parts(GlobalLock(handle) as *const u8, GlobalSize(handle)).to_vec();
            GlobalUnlock(handle);
            ReleaseStgMedium(&mut medium);
            let count = u32::from_le_bytes(data[..4].try_into().unwrap()) as usize;
            (0..count)
                .map(|entry| {
                    let name = &data[4 + entry * 592 + 72..4 + (entry + 1) * 592];
                    let units: Vec<u16> = name.as_chunks::<2>().0.iter().map(|pair| u16::from_le_bytes(*pair)).take_while(|unit| *unit != 0).collect();
                    String::from_utf16_lossy(&units)
                })
                .collect()
        }
    }

    /// A file's contents as a paste reads them: through the stream, in small reads, to its end
    fn read(object: &IDataObject, index: i32) -> Option<Vec<u8>> {
        // SAFETY: the stream is the object's, read into a buffer of the size said, and let go with its medium
        unsafe {
            let mut medium = object.GetData(&request(ole::contents_format(), index, TYMED_ISTREAM.0)).ok()?;
            let stream = (*medium.u.pstm).clone()?;
            let mut all = Vec::new();
            loop {
                let (mut piece, mut got) = ([0u8; 7], 0u32);
                let _ = stream.Read(piece.as_mut_ptr().cast(), piece.len() as u32, Some(&mut got));
                if got == 0 {
                    break;
                }
                all.extend_from_slice(&piece[..got as usize]);
            }
            drop(stream);
            ReleaseStgMedium(&mut medium);
            Some(all)
        }
    }

    #[test]
    fn files_copied_in_the_remote_desktop_are_only_fetched_when_pasted_here() {
        let _held = crate::clipboard::hold_for_test();
        let before = Contents { text: contents::text(), html: contents::html(), rtf: contents::rtf(), image: contents::image(), files: contents::files() };
        // SAFETY: OLE is readied on this thread, the one that pastes
        unsafe { OleInitialize(None) }.expect("OLE on the test's thread");
        let remote = Arc::new(Files { contents: vec![Vec::new(), b"quarter three, 2026".to_vec(), b"notes".to_vec()], asked: AtomicU32::new(0) });
        let promised = vec![
            Promised { path: "Reports".into(), folder: true, size: 0, modified: None },
            Promised { path: "Reports\\q3 \u{2013} final.txt".into(), folder: false, size: 19, modified: Some(133_000_000_000_000_000) },
            Promised { path: "notes.txt".into(), folder: false, size: 5, modified: None },
        ];
        let offered = offer(promised, Arc::clone(&remote) as Arc<dyn Source>);
        // File Explorer looks at the clipboard as it changes; that mustn't fetch anything
        std::thread::sleep(std::time::Duration::from_millis(500));
        let asked_before = remote.asked.load(Ordering::Relaxed);
        let has_paths = contents::kinds().files;
        // SAFETY: OLE is readied on this thread
        let object = unsafe { OleGetClipboard() }.expect("the clipboard's data object");
        let listed = names(&object);
        let asked_after_list = remote.asked.load(Ordering::Relaxed);
        let (nested, plain, folder) = (read(&object, 1), read(&object, 2), read(&object, 0));
        drop(object);
        contents::put(&before);

        assert!(offered, "the promise was put on the clipboard");
        assert_eq!(listed, ["Reports", "Reports\\q3 \u{2013} final.txt", "notes.txt"]);
        assert_eq!((asked_before, asked_after_list), (0, 0), "nothing is fetched for a copy, or for the list of names");
        assert!(!has_paths, "no list of paths for File Explorer to fetch at once");
        assert_eq!(nested.as_deref(), Some(&b"quarter three, 2026"[..]));
        assert_eq!(plain.as_deref(), Some(&b"notes"[..]));
        assert_eq!(folder, None, "a folder has no contents to read");
        assert!(remote.asked.load(Ordering::Relaxed) > 0, "the contents came from the remote desktop");
    }

    #[test]
    fn the_list_of_files_is_as_windows_reads_it() {
        let data = ole::list(&[
            Promised { path: "a.txt".into(), folder: false, size: 0x1_0000_0002, modified: Some(7) },
            Promised { path: "Folder".into(), folder: true, size: 99, modified: None },
        ]);
        assert_eq!(data.len(), 4 + 2 * 592);
        assert_eq!(&data[..4], &2u32.to_le_bytes());
        let number = |at: usize| u32::from_le_bytes(data[at..at + 4].try_into().unwrap());
        // the file: attributes, last change, size and progress said, a UTF-16 name; a plain file; 4 GB and 2 bytes
        assert_eq!(number(4), 0x8000_4064);
        assert_eq!((number(4 + 36), number(4 + 56), number(4 + 64), number(4 + 68)), (0x80, 7, 1, 2));
        assert_eq!(&data[4 + 72..4 + 82], &[b'a', 0, b'.', 0, b't', 0, b'x', 0, b't', 0]);
        // the folder: no size said, and none given
        assert_eq!(number(4 + 592), 0x8000_4004);
        assert_eq!((number(4 + 592 + 36), number(4 + 592 + 64), number(4 + 592 + 68)), (0x10, 0, 0));
    }
}
