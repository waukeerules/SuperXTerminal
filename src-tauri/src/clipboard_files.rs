//! Files copied in a remote desktop, on this computer's clipboard as a promise: their names are there to paste, and
//! each file's contents are fetched from the remote desktop only as the paste reads them.
//!
//! A list of paths (CF_HDROP, [`crate::clipboard_contents`]) can't be a promise: File Explorer asks for it the
//! moment the clipboard changes, so every copy of a large file in the remote desktop would fetch it at once, pasted
//! or not. Remote Desktop Connection puts the files' names on the clipboard instead (FileGroupDescriptorW) with
//! their contents as streams (FileContents), which Windows only reads when something pastes them; File Explorer
//! then shows its own progress and can be cancelled. That takes Windows' OLE clipboard and a data object
//! ([`ole`]), which is what this is.
//!
//! OLE calls a data object on the thread that made it, through that thread's messages. So the object is made, and
//! lives, on a thread of this module's own, which does nothing but pass messages on: a paste here waits there for
//! each piece of a file to come from the remote desktop, and holds up nothing else.
//!
//! Windows only so far. On the Mac and Linux [`offer`] says no, and files copied in the remote desktop aren't put
//! on the clipboard (text, and on Windows pictures and formatted text, still are).

#[cfg(any(not(windows), test))]
use std::sync::Arc;

/// A file or folder promised: where it goes under the place it's pasted, and what's known of it
#[cfg_attr(not(windows), allow(dead_code))]
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

/// Where promised files' contents come from: the remote desktop, asked a piece at a time. (Nothing asks yet on the
/// Mac and Linux, where files can't be promised.)
#[cfg_attr(not(windows), allow(dead_code))]
pub trait Source: Send + Sync {
    /// `length` bytes of the file at `index` in the list promised, from `offset`; fewer at its end, and None when
    /// they can't be had (the session ended, the remote desktop's clipboard moved on). May wait on the network.
    fn read(&self, index: usize, offset: u64, length: u32) -> Option<Vec<u8>>;
}

/// Put a promise of `files` on the clipboard in place of what it held. Nothing is fetched until something pastes
/// them, when `source` is read. False when it can't be done (and always on the Mac and Linux, for now).
#[cfg(not(windows))]
pub fn offer(_files: Vec<Promised>, _source: Arc<dyn Source>) -> bool {
    false
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

// The promise as a paste sees it: it's put on Windows' real clipboard (put back as it was after), and the test,
// as the program pasting, takes the clipboard's data object, reads the list of files from it, and reads a file's
// contents through its stream. The remote desktop is stood in for by bytes in memory that count how often they're
// asked for. Windows only: there's no such clipboard on the Mac and Linux.
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
