//! Remote Desktop's drives, as SuperXPuTTY's Drives option (RedirectDrives): this computer's drives show in the remote
//! desktop's File Explorer as "C on <this computer>" (`\\tsclient\C`), to open, copy, save, rename and delete files.
//!
//! IronRDP's device channel (RDPDR, MS-RDPEFS) decodes what Windows asks of a drive and takes a backend to answer it.
//! Its own backend (ironrdp-rdpdr-native) is for the Mac and Linux only and shares one folder, so this is ours, the
//! same on every system: plain `std::fs`, each drive a folder ([`Drive`]) that paths can't leave. It's attached as
//! the session's own channel (`with_static_channel` in rdp.rs), as the clipboard is.
//!
//! Windows opens a file or folder (create), gets an id for it, asks with that id (read, write, list, rename...) and
//! closes it. Folders aren't held open here, only remembered by their path: Windows can't open a folder as a file.
//! Every answer is made in [`Backend::answer`] as the channel's own PDU, which the tests read back.
//!
//! IronRDP ends the whole session when a channel can't read a message, and its device channel can't read every
//! request Windows may make of a drive (flush, security). So the channel is wrapped ([`Channel`]): such a request
//! is refused, and the desktop stays.
//!
//! The work is done on the session's thread as each request comes, so a slow drive (a network share that's gone)
//! holds up that tab's desktop, and no other tab's, until it answers.

use std::collections::{HashMap, VecDeque};
use std::fs::{File, Metadata, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use ironrdp_core::impl_as_any;
use ironrdp_pdu::PduResult;
use ironrdp_rdpdr::pdu::efs::*;
use ironrdp_rdpdr::pdu::esc::{ScardCall, ScardIoCtlCode};
use ironrdp_rdpdr::pdu::RdpdrPdu;
use ironrdp_rdpdr::{Rdpdr, RdpdrBackend};
use ironrdp_svc::{CompressionCondition, SvcClientProcessor, SvcMessage, SvcProcessor};

/// The path is a folder, and a file was asked for (STATUS_FILE_IS_A_DIRECTORY)
const FILE_IS_A_DIRECTORY: u32 = 0xC000_00BA;
/// A folder on the way to the path isn't there (STATUS_OBJECT_PATH_NOT_FOUND)
const OBJECT_PATH_NOT_FOUND: u32 = 0xC000_003A;
/// What a create answers when it made the file or folder (FILE_CREATED; IronRDP names the other three)
const FILE_CREATED: u8 = 2;
/// A disk, as FileFsDeviceInformation says it (FILE_DEVICE_DISK)
const FILE_DEVICE_DISK: u32 = 7;
/// The most read in one answer: Windows asks for far less (about 1 MB), and a length made up by a broken server
/// mustn't take that much memory
const MAX_READ: u32 = 8 * 1024 * 1024;
/// The allocation unit drives are described in (FileFsSizeInformation): 8 sectors of 512 bytes
const SECTOR: u32 = 512;
/// See [`SECTOR`]
const SECTORS_PER_UNIT: u32 = 8;

/// A drive shown in the remote desktop: its name there ("C" is "C on <this computer>") and the folder it is here
#[derive(Clone, Debug, PartialEq)]
pub struct Drive {
    pub name: String,
    pub root: PathBuf,
}

/// This computer's drives, as Remote Desktop Connection shares them: on Windows every drive letter, mapped network
/// drives too (those of the user SuperXTerminal runs as: run as administrator, Windows hides the user's mapped
/// drives from it). A drive with nothing in it (an empty card reader) is listed and fails when opened; asking each
/// whether it's ready could wait seconds on a network drive that's gone.
#[cfg(windows)]
pub fn local_drives() -> Vec<Drive> {
    use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetLogicalDrives};
    /// GetDriveTypeW: the letter has no drive (0 is "can't tell")
    const DRIVE_NO_ROOT_DIR: u32 = 1;
    // SAFETY: takes nothing and returns a bit for each drive letter
    let letters = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|i| letters & (1 << i) != 0)
        .filter_map(|i| {
            let letter = char::from(b'A' + i);
            let root = format!("{letter}:\\");
            let wide: Vec<u16> = root.encode_utf16().chain([0]).collect();
            // SAFETY: `wide` is a NUL-terminated UTF-16 string that lives through the call
            let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
            (kind > DRIVE_NO_ROOT_DIR).then(|| Drive { name: letter.to_string(), root: PathBuf::from(root) })
        })
        .collect()
}

/// This computer's drives: the user's home folder as "Home", and on the Mac each disk in /Volumes by its name (the
/// startup disk and any plugged in or mounted), as Finder's sidebar has them; elsewhere the whole file system as
/// "Root"
#[cfg(not(windows))]
pub fn local_drives() -> Vec<Drive> {
    let mut drives = Vec::new();
    if let Some(home) = dirs::home_dir() {
        drives.push(Drive { name: "Home".into(), root: home });
    }
    if cfg!(target_os = "macos") {
        let mut volumes: Vec<Drive> = std::fs::read_dir("/Volumes")
            .into_iter()
            .flatten()
            .flatten()
            .filter(|entry| entry.path().is_dir())
            .map(|entry| Drive { name: entry.file_name().to_string_lossy().into_owned(), root: entry.path() })
            .collect();
        volumes.sort_by(|a, b| a.name.cmp(&b.name));
        drives.extend(volumes);
    } else {
        drives.push(Drive { name: "Root".into(), root: PathBuf::from("/") });
    }
    drives
}

/// This computer's name, the "<this computer>" of "C on <this computer>": without its domain, as Windows shows it
fn computer_name() -> String {
    let name = if cfg!(windows) { std::env::var("COMPUTERNAME").unwrap_or_default() } else { host_name() };
    let short = name.split('.').next().unwrap_or_default().trim();
    if short.is_empty() { "SuperXTerminal".into() } else { short.to_string() }
}

/// The Mac's or Linux's host name ("" when it can't be read)
#[cfg(not(windows))]
fn host_name() -> String {
    let mut buffer = [0u8; 256];
    // SAFETY: gethostname writes at most `len` bytes into `buffer`, which lives through the call
    let ok = unsafe { libc::gethostname(buffer.as_mut_ptr().cast(), buffer.len()) } == 0;
    let end = buffer.iter().position(|b| *b == 0).unwrap_or(buffer.len());
    if ok { String::from_utf8_lossy(&buffer[..end]).into_owned() } else { String::new() }
}

/// Windows has COMPUTERNAME instead ([`computer_name`])
#[cfg(windows)]
fn host_name() -> String {
    String::new()
}

/// The device channel for a connection, offering `drives` to the remote desktop once the user is logged on. Each
/// drive's id on the channel is its place in the list, from 1 ([`Backend::new`] numbers them the same).
pub fn channel(drives: &[Drive]) -> Channel {
    let announced = drives.iter().zip(1u32..).map(|(drive, id)| (id, drive.name.clone())).collect();
    Channel(Rdpdr::new(Box::new(Backend::new(drives)), computer_name()).with_drives(Some(announced)))
}

/// IronRDP's device channel, kept from ending the session: a message it can't read (a request it doesn't know, as
/// Windows' flush or security ones) would be an error, and the engine drops the connection on a channel's error.
/// Here it's logged, and a request is answered "not supported" so Windows isn't left waiting for it.
#[derive(Debug)]
pub struct Channel(Rdpdr);

impl_as_any!(Channel);

impl SvcProcessor for Channel {
    /// The device channel's name ("rdpdr")
    fn channel_name(&self) -> ironrdp_pdu::gcc::ChannelName {
        self.0.channel_name()
    }

    /// Compressed when the device channel says so
    fn compression_condition(&self) -> CompressionCondition {
        self.0.compression_condition()
    }

    /// What the device channel sends as it starts
    fn start(&mut self) -> PduResult<Vec<SvcMessage>> {
        self.0.start()
    }

    /// A message from the remote desktop: the device channel's answers, or a refusal for one it can't read
    fn process(&mut self, payload: &[u8]) -> PduResult<Vec<SvcMessage>> {
        self.0.process(payload).or_else(|error| {
            crate::applog::warn(format!("Remote Desktop's drives: a request couldn't be read and was refused ({error})"));
            Ok(refusal(payload).into_iter().map(SvcMessage::from).collect())
        })
    }
}

impl SvcClientProcessor for Channel {}

/// "Not supported", for a request on the device channel that couldn't be read: its first bytes are the channel's
/// header for a request ("rD", "RI"), the drive's id, the file's and the id the answer must carry. None for any
/// other message, which wants no answer.
fn refusal(payload: &[u8]) -> Option<RdpdrPdu> {
    /// The header of a request about a device: the core component and its "I/O request" packet id
    const IO_REQUEST: [u8; 4] = [0x72, 0x44, 0x52, 0x49];
    let number = |at: usize| payload.get(at..at + 4).and_then(|bytes| bytes.try_into().ok()).map(u32::from_le_bytes);
    if payload.get(..4)? != IO_REQUEST {
        return None;
    }
    let response = DeviceIoResponse { device_id: number(4)?, completion_id: number(12)?, io_status: NtStatus::NOT_SUPPORTED };
    Some(RdpdrPdu::DeviceCloseResponse(DeviceCloseResponse { device_io_response: response }))
}

/// A file or folder Windows has open on a drive
#[derive(Debug)]
struct Open {
    /// Where it is on this computer; follows a rename
    path: PathBuf,
    /// The file itself, when it was opened for its contents; a folder, or a file opened only to ask about it, has
    /// none (a read then opens it)
    file: Option<File>,
    /// The folder being listed and the names not yet given, one for each query Windows makes
    listing: Option<(PathBuf, VecDeque<String>)>,
    /// Delete it when it's closed: how Windows deletes (it marks the file, then closes it)
    delete_on_close: bool,
}

/// Answers what the remote desktop asks of this computer's drives
#[derive(Debug)]
pub struct Backend {
    /// Each drive's folder, by its id on the channel
    roots: HashMap<u32, Drive>,
    /// The id the next file opened gets
    next_id: u32,
    /// What's open, by the id Windows was given for it
    open: HashMap<u32, Open>,
}

impl_as_any!(Backend);

impl Backend {
    /// A backend for `drives`, numbered from 1 in the order given, as [`channel`] announces them
    pub fn new(drives: &[Drive]) -> Self {
        Self { roots: (1u32..).zip(drives.iter().cloned()).collect(), next_id: 1, open: HashMap::new() }
    }

    /// Where a path Windows gave (`\Folder\file.txt`, from the drive's root) is on this computer. None when it would
    /// leave the drive ("..") or, on Windows, names something other than a file in a folder (a ":" is another drive,
    /// or a file's alternate stream), or the drive isn't one of ours.
    fn resolve(&self, device: u32, path: &str) -> Option<PathBuf> {
        let mut full = self.roots.get(&device)?.root.clone();
        for part in path.split(['\\', '/']).filter(|part| !part.is_empty() && *part != ".") {
            if part == ".." || (cfg!(windows) && part.contains(':')) {
                return None;
            }
            full.push(part);
        }
        Some(full)
    }

    /// Answer one request with the channel's PDUs: one each, except a request to be told when a folder changes
    /// (never answered, as nothing watches the folder: Windows then lists it again when asked to refresh)
    fn answer(&mut self, request: ServerDriveIoRequest) -> Vec<RdpdrPdu> {
        use ServerDriveIoRequest as R;
        vec![match request {
            R::ServerCreateDriveRequest(req) => self.create(req),
            R::DeviceCloseRequest(req) => self.close(req),
            R::DeviceReadRequest(req) => self.read(req),
            R::DeviceWriteRequest(req) => self.write(req),
            R::ServerDriveQueryInformationRequest(req) => self.query_information(req),
            R::ServerDriveQueryVolumeInformationRequest(req) => self.query_volume(req),
            R::ServerDriveQueryDirectoryRequest(req) => self.query_directory(req),
            R::ServerDriveSetInformationRequest(req) => self.set_information(req),
            // no control codes are acted on; an empty success, as FreeRDP's drive answers them
            R::DeviceControlRequest(req) => RdpdrPdu::DeviceControlResponse(DeviceControlResponse::new(req, NtStatus::SUCCESS, None)),
            // byte-range locks aren't kept (one desktop uses the drive); granted, as FreeRDP's drive does. The answer
            // has the close's shape: the request's ids, the status and four bytes of nothing
            R::ServerDriveLockControlRequest(req) => closed(req.device_io_request, NtStatus::SUCCESS),
            R::ServerDriveNotifyChangeDirectoryRequest(_) => return Vec::new(),
        }]
    }

    /// Open a file or folder, or make it, as the request's disposition says (open only, create only, either, or
    /// replace what's there), and give Windows an id for it
    fn create(&mut self, req: DeviceCreateRequest) -> RdpdrPdu {
        let io = req.device_io_request.clone();
        let failed = |status: NtStatus| created(&io, status, 0, 0);
        let Some(path) = self.resolve(io.device_id, &req.path) else { return failed(NtStatus::ACCESS_DENIED) };
        let wants_folder = req.create_options.contains(CreateOptions::FILE_DIRECTORY_FILE);
        let wants_file = req.create_options.contains(CreateOptions::FILE_NON_DIRECTORY_FILE);
        let how = req.create_disposition;
        let replaces = [CreateDisposition::FILE_SUPERSEDE, CreateDisposition::FILE_OVERWRITE, CreateDisposition::FILE_OVERWRITE_IF].contains(&how);
        let (file, information) = match std::fs::metadata(&path) {
            Ok(_) if how == CreateDisposition::FILE_CREATE => return failed(NtStatus::OBJECT_NAME_COLLISION),
            Ok(meta) if meta.is_dir() => {
                if wants_file {
                    return failed(NtStatus::from(FILE_IS_A_DIRECTORY));
                }
                (None, Information::FILE_OPENED.bits())
            }
            Ok(_) => {
                if wants_folder {
                    return failed(NtStatus::NOT_A_DIRECTORY);
                }
                match open_file(&path, &req.desired_access, replaces, false) {
                    Ok(file) => (file, if replaces { Information::FILE_OVERWRITTEN.bits() } else { Information::FILE_OPENED.bits() }),
                    Err(error) => return failed(status_of(&error)),
                }
            }
            Err(_) if how == CreateDisposition::FILE_OPEN || how == CreateDisposition::FILE_OVERWRITE => {
                let folder_there = path.parent().is_some_and(Path::is_dir);
                return failed(if folder_there { NtStatus::NO_SUCH_FILE } else { NtStatus::from(OBJECT_PATH_NOT_FOUND) });
            }
            Err(_) if wants_folder => match std::fs::create_dir(&path) {
                Ok(()) => (None, FILE_CREATED),
                Err(error) => return failed(status_of(&error)),
            },
            Err(_) => match open_file(&path, &req.desired_access, false, true) {
                Ok(file) => (file, FILE_CREATED),
                Err(error) => return failed(status_of(&error)),
            },
        };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1).max(1);
        let delete_on_close = req.create_options.contains(CreateOptions::FILE_DELETE_ON_CLOSE);
        self.open.insert(id, Open { path, file, listing: None, delete_on_close });
        created(&io, NtStatus::SUCCESS, id, information)
    }

    /// Close a file or folder; one marked for deleting is deleted now (a delete that fails isn't told: Windows
    /// already has its answer, and finds the file still there)
    fn close(&mut self, req: DeviceCloseRequest) -> RdpdrPdu {
        if let Some(open) = self.open.remove(&req.device_io_request.file_id) {
            let Open { path, file, delete_on_close, .. } = open;
            // Windows can't delete a file that's still open
            drop(file);
            if delete_on_close {
                let _ = if path.is_dir() { std::fs::remove_dir(&path) } else { std::fs::remove_file(&path) };
            }
        }
        closed(req.device_io_request, NtStatus::SUCCESS)
    }

    /// Read part of a file: as much as is there, none at its end
    fn read(&mut self, req: DeviceReadRequest) -> RdpdrPdu {
        let data = self.open.get_mut(&req.device_io_request.file_id).ok_or(NtStatus::NO_SUCH_FILE).and_then(|open| {
            // a file opened only to ask about it, and read after all
            if open.file.is_none() {
                open.file = Some(File::open(&open.path).map_err(|e| status_of(&e))?);
            }
            let file = open.file.as_mut().ok_or(NtStatus::UNSUCCESSFUL)?;
            file.seek(SeekFrom::Start(req.offset)).map_err(|e| status_of(&e))?;
            let mut data = vec![0u8; req.length.min(MAX_READ) as usize];
            let mut filled = 0;
            while filled < data.len() {
                match file.read(&mut data[filled..]) {
                    Ok(0) => break,
                    Ok(n) => filled += n,
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(error) => return Err(status_of(&error)),
                }
            }
            data.truncate(filled);
            Ok(data)
        });
        let (status, read_data) = match data {
            Ok(data) => (NtStatus::SUCCESS, data),
            Err(status) => (status, Vec::new()),
        };
        RdpdrPdu::DeviceReadResponse(DeviceReadResponse { device_io_reply: DeviceIoResponse::new(req.device_io_request, status), read_data })
    }

    /// Write part of a file, all of it or none
    fn write(&mut self, req: DeviceWriteRequest) -> RdpdrPdu {
        let written = self.open.get_mut(&req.device_io_request.file_id).ok_or(NtStatus::NO_SUCH_FILE).and_then(|open| {
            // not opened for writing: Windows was told so when it opened it
            let file = open.file.as_mut().ok_or(NtStatus::ACCESS_DENIED)?;
            file.seek(SeekFrom::Start(req.offset)).map_err(|e| status_of(&e))?;
            file.write_all(&req.write_data).map_err(|e| status_of(&e))?;
            u32::try_from(req.write_data.len()).map_err(|_| NtStatus::UNSUCCESSFUL)
        });
        let (status, length) = match written {
            Ok(length) => (NtStatus::SUCCESS, length),
            Err(status) => (status, 0),
        };
        RdpdrPdu::DeviceWriteResponse(DeviceWriteResponse { device_io_reply: DeviceIoResponse::new(req.device_io_request, status), length })
    }

    /// What Windows asks about an open file or folder: its times and attributes, its size and whether it's a folder
    fn query_information(&mut self, req: ServerDriveQueryInformationRequest) -> RdpdrPdu {
        let level = req.file_info_class_lvl;
        let found = self.open.get(&req.device_io_request.file_id).ok_or(NtStatus::NO_SUCH_FILE).and_then(|open| {
            let meta = std::fs::metadata(&open.path).map_err(|e| status_of(&e))?;
            let file_attributes = attributes(&meta, &name_of(&open.path));
            let size = i64::try_from(meta.len()).unwrap_or(i64::MAX);
            if level == FileInformationClassLevel::FILE_BASIC_INFORMATION {
                let written = filetime(meta.modified());
                Ok(FileInformationClass::Basic(FileBasicInformation {
                    creation_time: filetime(meta.created()),
                    last_access_time: filetime(meta.accessed()),
                    last_write_time: written,
                    change_time: written,
                    file_attributes,
                }))
            } else if level == FileInformationClassLevel::FILE_STANDARD_INFORMATION {
                Ok(FileInformationClass::Standard(FileStandardInformation {
                    allocation_size: size,
                    end_of_file: size,
                    number_of_links: 1,
                    delete_pending: if open.delete_on_close { Boolean::True } else { Boolean::False },
                    directory: if meta.is_dir() { Boolean::True } else { Boolean::False },
                }))
            } else if level == FileInformationClassLevel::FILE_ATTRIBUTE_TAG_INFORMATION {
                Ok(FileInformationClass::AttributeTag(FileAttributeTagInformation { file_attributes, reparse_tag: 0 }))
            } else {
                Err(NtStatus::NOT_SUPPORTED)
            }
        });
        let (status, buffer) = match found {
            Ok(information) => (NtStatus::SUCCESS, Some(information)),
            Err(status) => (status, None),
        };
        RdpdrPdu::ClientDriveQueryInformationResponse(ClientDriveQueryInformationResponse {
            device_io_response: DeviceIoResponse::new(req.device_io_request, status),
            buffer,
        })
    }

    /// What Windows asks about the drive itself: its name, its size and free space, and what its file system can do.
    /// It's described as FAT32, as FreeRDP's drives are, so Windows doesn't ask for what NTFS has and this hasn't
    /// (permissions, streams, links).
    fn query_volume(&mut self, req: ServerDriveQueryVolumeInformationRequest) -> RdpdrPdu {
        let io = req.device_io_request;
        let level = req.fs_info_class_lvl;
        let found = self.roots.get(&io.device_id).ok_or(NtStatus::NO_SUCH_FILE).and_then(|drive| {
            use FileSystemInformationClass as Class;
            use FileSystemInformationClassLevel as Level;
            let unit = u64::from(SECTOR * SECTORS_PER_UNIT);
            let units = |bytes: u64| i64::try_from(bytes / unit).unwrap_or(i64::MAX);
            if level == Level::FILE_FS_VOLUME_INFORMATION {
                Ok(Class::FileFsVolumeInformation(FileFsVolumeInformation {
                    volume_creation_time: std::fs::metadata(&drive.root).map(|meta| filetime(meta.created())).unwrap_or(0),
                    // any number that tells one drive from another
                    volume_serial_number: drive.name.bytes().fold(0x5358_5400, |sum, b| sum * 31 + u32::from(b)),
                    supports_objects: Boolean::False,
                    volume_label: drive.name.clone(),
                }))
            } else if level == Level::FILE_FS_SIZE_INFORMATION {
                let (total, free) = space(&drive.root);
                Ok(Class::FileFsSizeInformation(FileFsSizeInformation {
                    total_alloc_units: units(total),
                    available_alloc_units: units(free),
                    sectors_per_alloc_unit: SECTORS_PER_UNIT,
                    bytes_per_sector: SECTOR,
                }))
            } else if level == Level::FILE_FS_FULL_SIZE_INFORMATION {
                let (total, free) = space(&drive.root);
                Ok(Class::FileFsFullSizeInformation(FileFsFullSizeInformation {
                    total_alloc_units: units(total),
                    caller_available_alloc_units: units(free),
                    actual_available_alloc_units: units(free),
                    sectors_per_alloc_unit: SECTORS_PER_UNIT,
                    bytes_per_sector: SECTOR,
                }))
            } else if level == Level::FILE_FS_ATTRIBUTE_INFORMATION {
                Ok(Class::FileFsAttributeInformation(FileFsAttributeInformation {
                    file_system_attributes: FileSystemAttributes::FILE_CASE_PRESERVED_NAMES | FileSystemAttributes::FILE_UNICODE_ON_DISK,
                    max_component_name_len: 255,
                    file_system_name: "FAT32".into(),
                }))
            } else if level == Level::FILE_FS_DEVICE_INFORMATION {
                Ok(Class::FileFsDeviceInformation(FileFsDeviceInformation { device_type: FILE_DEVICE_DISK, characteristics: Characteristics::empty() }))
            } else {
                Err(NtStatus::NOT_SUPPORTED)
            }
        });
        let (status, buffer) = match found {
            Ok(information) => (NtStatus::SUCCESS, Some(information)),
            Err(status) => (status, None),
        };
        RdpdrPdu::ClientDriveQueryVolumeInformationResponse(ClientDriveQueryVolumeInformationResponse {
            device_io_reply: DeviceIoResponse::new(io, status),
            buffer,
        })
    }

    /// List a folder, one name for each query: the first query names the folder and a pattern (`\Folder\*`, or one
    /// file's name to ask whether it's there) and gets the first match; each one after gets the next, until there
    /// are no more
    fn query_directory(&mut self, req: ServerDriveQueryDirectoryRequest) -> RdpdrPdu {
        let io = req.device_io_request;
        let level = req.file_info_class_lvl;
        let first = req.initial_query != 0;
        let started = if first {
            let (folder, pattern) = req.path.rsplit_once('\\').unwrap_or(("", req.path.as_str()));
            self.resolve(io.device_id, folder).map(|folder| {
                let names = listing(&folder, pattern);
                (folder, names)
            })
        } else {
            None
        };
        let found = self.open.get_mut(&io.file_id).ok_or(NtStatus::NO_SUCH_FILE).and_then(|open| {
            if first {
                open.listing = Some(started.ok_or(NtStatus::ACCESS_DENIED)?);
            }
            let nothing = if first { NtStatus::NO_SUCH_FILE } else { NtStatus::NO_MORE_FILES };
            let (folder, names) = open.listing.as_mut().ok_or(nothing)?;
            // a name whose file went away since the folder was read is passed over
            while let Some(name) = names.pop_front() {
                if let Some(entry) = entry(&folder.join(&name), name, &level) {
                    return Ok(entry);
                }
            }
            Err(nothing)
        });
        let (status, buffer) = match found {
            Ok(information) => (NtStatus::SUCCESS, Some(information)),
            Err(status) => (status, None),
        };
        RdpdrPdu::ClientDriveQueryDirectoryResponse(ClientDriveQueryDirectoryResponse { device_io_reply: DeviceIoResponse::new(io, status), buffer })
    }

    /// Change an open file or folder: its length, its name (a rename or a move within the drive), whether it's
    /// deleted when closed, or its modified time (kept by a copy from the remote desktop)
    fn set_information(&mut self, req: ServerDriveSetInformationRequest) -> RdpdrPdu {
        let io = req.device_io_request.clone();
        let renamed_to = match &req.set_buffer {
            FileInformationClass::Rename(rename) => Some(self.resolve(io.device_id, &rename.file_name)),
            _ => None,
        };
        let done = self.open.get_mut(&io.file_id).ok_or(NtStatus::NO_SUCH_FILE).and_then(|open| match &req.set_buffer {
            FileInformationClass::EndOfFile(end) => {
                let length = u64::try_from(end.end_of_file).map_err(|_| NtStatus::UNSUCCESSFUL)?;
                let file = open.file.as_ref().ok_or(NtStatus::ACCESS_DENIED)?;
                file.set_len(length).map_err(|e| status_of(&e))
            }
            // room set aside before a copy: nothing to do, the file grows as it's written
            FileInformationClass::Allocation(_) => Ok(()),
            FileInformationClass::Disposition(disposition) => {
                let delete = disposition.delete_pending != 0;
                // told now, as Windows only deletes at the close and has no answer to read then
                if delete && open.path.is_dir() && std::fs::read_dir(&open.path).is_ok_and(|mut entries| entries.next().is_some()) {
                    return Err(NtStatus::DIRECTORY_NOT_EMPTY);
                }
                open.delete_on_close = delete;
                Ok(())
            }
            FileInformationClass::Rename(rename) => {
                let target = renamed_to.clone().flatten().ok_or(NtStatus::ACCESS_DENIED)?;
                if rename.replace_if_exists == Boolean::False && target.exists() {
                    return Err(NtStatus::OBJECT_NAME_COLLISION);
                }
                std::fs::rename(&open.path, &target).map_err(|e| status_of(&e))?;
                open.path = target;
                Ok(())
            }
            FileInformationClass::Basic(basic) => {
                // 0 and -1 leave the time as it is; a time that can't be set (a file opened to read) is let go
                if let (Some(file), Some(time)) = (open.file.as_ref(), system_time(basic.last_write_time)) {
                    let _ = file.set_modified(time);
                }
                Ok(())
            }
            _ => Err(NtStatus::NOT_SUPPORTED),
        });
        let status = done.err().unwrap_or(NtStatus::SUCCESS);
        match ClientDriveSetInformationResponse::new(&req, status) {
            Ok(response) => RdpdrPdu::ClientDriveSetInformationResponse(response),
            // the answer repeats the request's length, which always fits; were it not to, the request still ends
            Err(_) => closed(io, NtStatus::UNSUCCESSFUL),
        }
    }
}

impl RdpdrBackend for Backend {
    /// Windows took a drive, or refused it (then it isn't shown there; the log says which)
    fn handle_server_device_announce_response(&mut self, pdu: ServerDeviceAnnounceResponse) -> PduResult<()> {
        if pdu.result_code != NtStatus::SUCCESS {
            let name = self.roots.get(&pdu.device_id).map(|drive| drive.name.as_str()).unwrap_or("?");
            crate::applog::warn(format!("Remote Desktop refused drive {name}: {:?}", pdu.result_code));
        }
        Ok(())
    }

    /// Smart cards aren't shared, so none is ever asked for
    fn handle_scard_call(&mut self, _req: DeviceControlRequest<ScardIoCtlCode>, _call: ScardCall) -> PduResult<()> {
        Ok(())
    }

    /// One request about a drive, answered at once
    fn handle_drive_io_request(&mut self, req: ServerDriveIoRequest) -> PduResult<Vec<SvcMessage>> {
        Ok(self.answer(req).into_iter().map(SvcMessage::from).collect())
    }
}

/// The answer to a create: the status, the id for what was opened, and whether it was opened, made or replaced
fn created(io: &DeviceIoRequest, status: NtStatus, file_id: u32, information: u8) -> RdpdrPdu {
    RdpdrPdu::DeviceCreateResponse(DeviceCreateResponse {
        device_io_reply: DeviceIoResponse::new(io.clone(), status),
        file_id,
        information: Information::from_bits_retain(information),
    })
}

/// The answer to a close, and to any request answered with its status alone
fn closed(io: DeviceIoRequest, status: NtStatus) -> RdpdrPdu {
    RdpdrPdu::DeviceCloseResponse(DeviceCloseResponse { device_io_response: DeviceIoResponse::new(io, status) })
}

/// Open a file for what Windows wants of it (`access`): to write (or `replace` its contents, or `create` it) opens
/// it to read and write; to read, to read; "whatever I'm allowed" tries both. One wanted only for its attributes or
/// to delete isn't opened at all (None), so a file another program has locked can still be listed and asked about.
fn open_file(path: &Path, access: &DesiredAccess, replace: bool, create: bool) -> io::Result<Option<File>> {
    let writes = DesiredAccess::FILE_WRITE_DATA_OR_FILE_ADD_FILE
        | DesiredAccess::FILE_APPEND_DATA_OR_FILE_ADD_SUBDIRECTORY
        | DesiredAccess::GENERIC_WRITE
        | DesiredAccess::GENERIC_ALL;
    let reads = DesiredAccess::FILE_READ_DATA_OR_FILE_LIST_DIRECTORY | DesiredAccess::GENERIC_READ | DesiredAccess::FILE_EXECUTE;
    let both = || OpenOptions::new().read(true).write(true).create_new(create).truncate(replace).open(path);
    if create || replace || access.intersects(writes) {
        both().map(Some)
    } else if access.contains(DesiredAccess::MAXIMUM_ALLOWED) {
        Ok(both().or_else(|_| File::open(path)).ok())
    } else if access.intersects(reads) {
        File::open(path).map(Some)
    } else {
        Ok(None)
    }
}

/// An error from this computer's file system as Windows' status for it
fn status_of(error: &io::Error) -> NtStatus {
    match error.kind() {
        io::ErrorKind::NotFound => NtStatus::NO_SUCH_FILE,
        io::ErrorKind::PermissionDenied => NtStatus::ACCESS_DENIED,
        io::ErrorKind::AlreadyExists => NtStatus::OBJECT_NAME_COLLISION,
        io::ErrorKind::DirectoryNotEmpty => NtStatus::DIRECTORY_NOT_EMPTY,
        io::ErrorKind::NotADirectory => NtStatus::NOT_A_DIRECTORY,
        _ => NtStatus::UNSUCCESSFUL,
    }
}

/// A path's last part: the file's or folder's own name ("" for a drive's root)
fn name_of(path: &Path) -> String {
    path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
}

/// A file's attributes as Windows has them. On Windows they're the file's own (hidden, system, read-only...);
/// elsewhere a folder or a file, read-only when it can't be written, and hidden when its name starts with a dot.
fn attributes(meta: &Metadata, name: &str) -> FileAttributes {
    #[cfg(windows)]
    let mut attributes = {
        use std::os::windows::fs::MetadataExt;
        let _ = name;
        FileAttributes::from_bits_retain(meta.file_attributes())
    };
    #[cfg(not(windows))]
    let mut attributes = {
        let mut attributes = if meta.is_dir() { FileAttributes::FILE_ATTRIBUTE_DIRECTORY } else { FileAttributes::FILE_ATTRIBUTE_ARCHIVE };
        if name.starts_with('.') {
            attributes |= FileAttributes::FILE_ATTRIBUTE_HIDDEN;
        }
        if meta.permissions().readonly() {
            attributes |= FileAttributes::FILE_ATTRIBUTE_READONLY;
        }
        attributes
    };
    if attributes.is_empty() {
        attributes = FileAttributes::FILE_ATTRIBUTE_NORMAL;
    }
    attributes
}

/// The 100-nanosecond ticks between Windows' first day (1601) and Unix's (1970)
const FILETIME_EPOCH: i64 = 116_444_736_000_000_000;

/// A time as Windows counts it (100-nanosecond ticks since 1601); 0 when the file system has none
fn filetime(time: io::Result<SystemTime>) -> i64 {
    time.ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .and_then(|since| i64::try_from(since.as_nanos() / 100).ok())
        .map_or(0, |ticks| ticks.saturating_add(FILETIME_EPOCH))
}

/// A Windows time as this computer's; None for 0 and -1 (Windows' "leave it as it is") and anything before 1970
fn system_time(filetime: i64) -> Option<SystemTime> {
    let ticks = u64::try_from(filetime.checked_sub(FILETIME_EPOCH)?).ok()?;
    UNIX_EPOCH.checked_add(Duration::from_nanos(ticks.checked_mul(100)?))
}

/// A drive's size and free space in bytes (0, 0 when it can't be asked: nothing in the drive)
#[cfg(windows)]
fn space(root: &Path) -> (u64, u64) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
    let wide: Vec<u16> = root.as_os_str().encode_wide().chain([0]).collect();
    let (mut free, mut total) = (0u64, 0u64);
    // SAFETY: `wide` is a NUL-terminated UTF-16 path and the two numbers are valid to write to, all through the call
    let ok = unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, std::ptr::null_mut()) } != 0;
    if ok { (total, free) } else { (0, 0) }
}

/// A drive's size and free space in bytes (0, 0 when it can't be asked)
#[cfg(not(windows))]
fn space(root: &Path) -> (u64, u64) {
    use std::os::unix::ffi::OsStrExt;
    let Ok(path) = std::ffi::CString::new(root.as_os_str().as_bytes()) else { return (0, 0) };
    // SAFETY: a zeroed statvfs is a valid value for the call to fill in
    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: `path` is a NUL-terminated string and `stat` is valid to write to, both through the call
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return (0, 0);
    }
    // the fields' widths differ between the Mac and Linux
    #[allow(clippy::unnecessary_cast)]
    let (block, blocks, available) = (stat.f_frsize as u64, stat.f_blocks as u64, stat.f_bavail as u64);
    (block.saturating_mul(blocks), block.saturating_mul(available))
}

/// The names in a folder that match a pattern, in name order; none when the folder can't be read
fn listing(folder: &Path, pattern: &str) -> VecDeque<String> {
    let mut names: Vec<String> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| matches(pattern, name))
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names.into()
}

/// Whether a name matches a pattern as Windows writes them: `*` any run of characters, `?` any one, and letters
/// whatever their case. `*.*` is every name, with a dot or not, and `<`, `>` and `"` are the `*`, `?` and `.` that
/// Windows rewrites patterns with before sending them.
fn matches(pattern: &str, name: &str) -> bool {
    if pattern == "*" || pattern == "*.*" {
        return true;
    }
    let pattern: Vec<char> = pattern
        .to_lowercase()
        .chars()
        .map(|c| match c {
            '<' => '*',
            '>' => '?',
            '"' => '.',
            c => c,
        })
        .collect();
    let name: Vec<char> = name.to_lowercase().chars().collect();
    // where the pattern and the name are, and where to go back to (the last `*`, taking one more character)
    let (mut p, mut n, mut star) = (0, 0, None);
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            star = Some((p, n));
            p += 1;
        } else if let Some((star_p, star_n)) = star {
            p = star_p + 1;
            n = star_n + 1;
            star = Some((star_p, star_n + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|c| *c == '*')
}

/// One name of a folder's listing with what Windows asked to know of it (`level`: its name alone, or its times,
/// size and attributes too); None when it's gone, or for a level a listing doesn't have. A link that leads nowhere
/// is listed as itself.
fn entry(path: &Path, name: String, level: &FileInformationClassLevel) -> Option<FileInformationClass> {
    let meta = std::fs::metadata(path).or_else(|_| std::fs::symlink_metadata(path)).ok()?;
    let (created, accessed, written) = (filetime(meta.created()), filetime(meta.accessed()), filetime(meta.modified()));
    let size = i64::try_from(meta.len()).unwrap_or(i64::MAX);
    let file_attributes = attributes(&meta, &name);
    if *level == FileInformationClassLevel::FILE_BOTH_DIRECTORY_INFORMATION {
        Some(FileBothDirectoryInformation::new(created, accessed, written, written, size, file_attributes, name).into())
    } else if *level == FileInformationClassLevel::FILE_FULL_DIRECTORY_INFORMATION {
        Some(FileFullDirectoryInformation::new(created, accessed, written, written, size, file_attributes, name).into())
    } else if *level == FileInformationClassLevel::FILE_DIRECTORY_INFORMATION {
        Some(FileDirectoryInformation::new(created, accessed, written, written, size, file_attributes, name).into())
    } else if *level == FileInformationClassLevel::FILE_NAMES_INFORMATION {
        Some(FileNamesInformation::new(name).into())
    } else {
        None
    }
}

// A drive as the remote desktop uses it: the backend is asked what Windows asks over the channel (open, list, read,
// write, rename, delete, close) and its answers read back, against a real folder made in the temporary folder for
// each test. The channel itself is driven once, with the "user logged on" message a server sends, to see the drives
// offered. Nothing here needs a server, and the same requests run on Windows, the Mac and Linux.
#[cfg(test)]
mod tests {
    use super::*;

    /// A folder of its own for a test, empty, shared as drive 1 ("T")
    fn drive(test: &str) -> (Backend, PathBuf) {
        let root = std::env::temp_dir().join(format!("sxt-rdp-drives-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        (Backend::new(&[Drive { name: "T".into(), root: root.clone() }]), root)
    }

    /// A request's header, as Windows sends it for an open file (0 for a create) on drive 1
    fn io(file_id: u32, major_function: MajorFunction) -> DeviceIoRequest {
        DeviceIoRequest { device_id: 1, file_id, completion_id: 42, major_function, minor_function: MinorFunction::from(0) }
    }

    /// The one answer to a request
    fn answer(backend: &mut Backend, request: impl Into<ServerDriveIoRequest>) -> RdpdrPdu {
        let mut answers = backend.answer(request.into());
        assert_eq!(answers.len(), 1, "one answer for each request");
        answers.remove(0)
    }

    /// Open or make a file or folder; the status, and the id to ask with
    fn open(backend: &mut Backend, path: &str, how: CreateDisposition, options: CreateOptions, access: DesiredAccess) -> (NtStatus, u32) {
        let request = DeviceCreateRequest {
            device_io_request: io(0, MajorFunction::Create),
            desired_access: access,
            allocation_size: 0,
            file_attributes: FileAttributes::empty(),
            shared_access: SharedAccess::empty(),
            create_disposition: how,
            create_options: options,
            path: path.into(),
        };
        match answer(backend, request) {
            RdpdrPdu::DeviceCreateResponse(response) => (response.device_io_reply.io_status, response.file_id),
            other => panic!("a create is answered with a create's answer, not {other:?}"),
        }
    }

    /// Open what's there, to read it or list it
    fn open_existing(backend: &mut Backend, path: &str) -> (NtStatus, u32) {
        open(backend, path, CreateDisposition::FILE_OPEN, CreateOptions::empty(), DesiredAccess::GENERIC_READ)
    }

    /// Close a file or folder
    fn close(backend: &mut Backend, id: u32) {
        answer(backend, DeviceCloseRequest { device_io_request: io(id, MajorFunction::Close) });
    }

    /// Whether an answer carries this status (for the answers whose status can't be read as a field)
    fn says(answer: &RdpdrPdu, status: NtStatus) -> bool {
        format!("{answer:?}").contains(&format!("{status:?}"))
    }

    /// List an open folder as File Explorer does (`pattern` is the folder's path and `*`): each name, and whether
    /// it's a folder, until Windows is told there are no more
    fn list(backend: &mut Backend, id: u32, pattern: &str) -> Vec<(String, bool)> {
        let mut names = Vec::new();
        for first in [1u8].into_iter().chain(std::iter::repeat(0)) {
            let request = ServerDriveQueryDirectoryRequest {
                device_io_request: io(id, MajorFunction::DirectoryControl),
                file_info_class_lvl: FileInformationClassLevel::FILE_BOTH_DIRECTORY_INFORMATION,
                initial_query: first,
                path: pattern.into(),
            };
            let RdpdrPdu::ClientDriveQueryDirectoryResponse(response) = answer(backend, request) else { panic!("not a listing's answer") };
            match response.buffer {
                Some(FileInformationClass::BothDirectory(entry)) => {
                    names.push((entry.file_name, entry.file_attributes.contains(FileAttributes::FILE_ATTRIBUTE_DIRECTORY)));
                }
                _ => {
                    let end = if first == 1 { NtStatus::NO_SUCH_FILE } else { NtStatus::NO_MORE_FILES };
                    assert_eq!(response.device_io_reply.io_status, end);
                    break;
                }
            }
        }
        names
    }

    /// Read part of an open file
    fn read(backend: &mut Backend, id: u32, offset: u64, length: u32) -> (NtStatus, Vec<u8>) {
        let request = DeviceReadRequest { device_io_request: io(id, MajorFunction::Read), length, offset };
        let RdpdrPdu::DeviceReadResponse(response) = answer(backend, request) else { panic!("not a read's answer") };
        (response.device_io_reply.io_status, response.read_data)
    }

    /// Change an open file (rename it, mark it to delete, set its length); the answer
    fn set(backend: &mut Backend, id: u32, change: FileInformationClass) -> RdpdrPdu {
        answer(backend, ServerDriveSetInformationRequest { device_io_request: io(id, MajorFunction::SetInformation), set_buffer: change })
    }

    /// A name with whether it's a folder, as [`list`] gives them
    fn named(name: &str, folder: bool) -> (String, bool) {
        (name.to_string(), folder)
    }

    #[test]
    fn a_folder_here_is_listed_in_the_remote_desktop_with_its_files_and_folders() {
        let (mut backend, root) = drive("list");
        std::fs::write(root.join("notes.txt"), "hello").unwrap();
        std::fs::write(root.join("Report.pdf"), "pdf").unwrap();
        std::fs::create_dir(root.join("Photos")).unwrap();
        std::fs::write(root.join("Photos").join("one.jpg"), "jpg").unwrap();

        let (opened, id) = open_existing(&mut backend, "\\");
        assert_eq!(opened, NtStatus::SUCCESS);
        assert_eq!(list(&mut backend, id, "\\*"), [named("notes.txt", false), named("Photos", true), named("Report.pdf", false)]);
        close(&mut backend, id);

        // a folder inside it, and only the names a pattern asks for
        let (_, id) = open_existing(&mut backend, "\\Photos");
        assert_eq!(list(&mut backend, id, "\\Photos\\*"), [named("one.jpg", false)]);
        assert_eq!(list(&mut backend, id, "\\Photos\\*.png"), []);
        // File Explorer asks for one name to see whether it's there, whatever its case
        assert_eq!(list(&mut backend, id, "\\Photos\\ONE.JPG"), [named("one.jpg", false)]);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_here_is_read_in_the_remote_desktop_part_by_part() {
        let (mut backend, root) = drive("read");
        std::fs::write(root.join("notes.txt"), "hello from this computer").unwrap();
        let (opened, id) = open_existing(&mut backend, "\\notes.txt");
        assert_eq!(opened, NtStatus::SUCCESS);
        assert_eq!(read(&mut backend, id, 0, 5), (NtStatus::SUCCESS, b"hello".to_vec()));
        assert_eq!(read(&mut backend, id, 11, 1024), (NtStatus::SUCCESS, b"this computer".to_vec()));
        // at its end there's nothing more, and that's no error
        assert_eq!(read(&mut backend, id, 24, 1024), (NtStatus::SUCCESS, Vec::new()));

        // its size, asked before the copy starts
        let request = ServerDriveQueryInformationRequest {
            device_io_request: io(id, MajorFunction::QueryInformation),
            file_info_class_lvl: FileInformationClassLevel::FILE_STANDARD_INFORMATION,
        };
        let RdpdrPdu::ClientDriveQueryInformationResponse(response) = answer(&mut backend, request) else { panic!("not information") };
        let Some(FileInformationClass::Standard(standard)) = response.buffer else { panic!("no size") };
        assert_eq!((standard.end_of_file, standard.directory), (24, Boolean::False));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_saved_from_the_remote_desktop_lands_here() {
        let (mut backend, root) = drive("write");
        let (made, folder) =
            open(&mut backend, "\\From Windows", CreateDisposition::FILE_CREATE, CreateOptions::FILE_DIRECTORY_FILE, DesiredAccess::GENERIC_READ);
        assert_eq!(made, NtStatus::SUCCESS);
        close(&mut backend, folder);

        let path = "\\From Windows\\saved.txt";
        let (made, id) = open(&mut backend, path, CreateDisposition::FILE_CREATE, CreateOptions::FILE_NON_DIRECTORY_FILE, DesiredAccess::GENERIC_WRITE);
        assert_eq!(made, NtStatus::SUCCESS);
        // a copy writes in pieces, each at its place
        for (offset, piece) in [(0u64, "saved in "), (9, "the remote desktop")] {
            let request = DeviceWriteRequest { device_io_request: io(id, MajorFunction::Write), offset, write_data: piece.as_bytes().to_vec() };
            let RdpdrPdu::DeviceWriteResponse(response) = answer(&mut backend, request) else { panic!("not a write's answer") };
            assert_eq!((response.device_io_reply.io_status, response.length as usize), (NtStatus::SUCCESS, piece.len()));
        }
        close(&mut backend, id);
        assert_eq!(std::fs::read_to_string(root.join("From Windows").join("saved.txt")).unwrap(), "saved in the remote desktop");

        // saved again over itself, shorter: nothing of the old file is left
        let (again, id) = open(&mut backend, path, CreateDisposition::FILE_OVERWRITE_IF, CreateOptions::empty(), DesiredAccess::GENERIC_WRITE);
        assert_eq!(again, NtStatus::SUCCESS);
        answer(&mut backend, DeviceWriteRequest { device_io_request: io(id, MajorFunction::Write), offset: 0, write_data: b"short".to_vec() });
        close(&mut backend, id);
        assert_eq!(std::fs::read_to_string(root.join("From Windows").join("saved.txt")).unwrap(), "short");
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_renamed_or_deleted_in_the_remote_desktop_is_renamed_or_deleted_here() {
        let (mut backend, root) = drive("rename");
        std::fs::write(root.join("old.txt"), "kept").unwrap();
        std::fs::write(root.join("taken.txt"), "other").unwrap();
        std::fs::create_dir(root.join("Full")).unwrap();
        std::fs::write(root.join("Full").join("inside.txt"), "x").unwrap();

        let (_, id) = open(&mut backend, "\\old.txt", CreateDisposition::FILE_OPEN, CreateOptions::empty(), DesiredAccess::DELETE);
        // not onto a name that's taken, unless Windows says to replace it
        let onto = |name: &str| FileInformationClass::Rename(FileRenameInformation { replace_if_exists: Boolean::False, file_name: name.into() });
        assert!(says(&set(&mut backend, id, onto("\\taken.txt")), NtStatus::OBJECT_NAME_COLLISION));
        assert!(says(&set(&mut backend, id, onto("\\new.txt")), NtStatus::SUCCESS));
        assert!(!root.join("old.txt").exists());
        assert_eq!(std::fs::read_to_string(root.join("new.txt")).unwrap(), "kept");

        // deleting marks the file, and it goes when it's closed
        let delete = || FileInformationClass::Disposition(FileDispositionInformation { delete_pending: 1 });
        assert!(says(&set(&mut backend, id, delete()), NtStatus::SUCCESS));
        assert!(root.join("new.txt").exists());
        close(&mut backend, id);
        assert!(!root.join("new.txt").exists());

        // a folder with files in it isn't deleted, and Windows is told why
        let (_, folder) = open(&mut backend, "\\Full", CreateDisposition::FILE_OPEN, CreateOptions::FILE_DIRECTORY_FILE, DesiredAccess::DELETE);
        assert!(says(&set(&mut backend, folder, delete()), NtStatus::DIRECTORY_NOT_EMPTY));
        close(&mut backend, folder);
        assert!(root.join("Full").join("inside.txt").exists());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn nothing_outside_the_drive_can_be_opened_made_or_renamed_to() {
        let (mut backend, root) = drive("escape");
        let outside = root.with_file_name(format!("{}-outside.txt", root.file_name().unwrap().to_string_lossy()));
        std::fs::write(&outside, "not shared").unwrap();
        std::fs::write(root.join("mine.txt"), "shared").unwrap();
        let name = outside.file_name().unwrap().to_string_lossy().into_owned();

        assert_eq!(open_existing(&mut backend, &format!("\\..\\{name}")).0, NtStatus::ACCESS_DENIED);
        assert_eq!(open_existing(&mut backend, &format!("\\sub\\..\\..\\{name}")).0, NtStatus::ACCESS_DENIED);
        let made = open(&mut backend, "\\..\\sxt-made.txt", CreateDisposition::FILE_CREATE, CreateOptions::empty(), DesiredAccess::GENERIC_WRITE);
        assert_eq!(made.0, NtStatus::ACCESS_DENIED);
        assert!(!root.with_file_name("sxt-made.txt").exists());

        let (_, id) = open(&mut backend, "\\mine.txt", CreateDisposition::FILE_OPEN, CreateOptions::empty(), DesiredAccess::DELETE);
        let out = FileInformationClass::Rename(FileRenameInformation { replace_if_exists: Boolean::True, file_name: format!("\\..\\{name}") });
        assert!(says(&set(&mut backend, id, out), NtStatus::ACCESS_DENIED));
        assert_eq!(std::fs::read_to_string(&outside).unwrap(), "not shared");
        assert!(root.join("mine.txt").exists());

        // and a drive that isn't shared isn't there at all
        let mut header = io(0, MajorFunction::Create);
        header.device_id = 9;
        let elsewhere = DeviceCreateRequest {
            device_io_request: header,
            desired_access: DesiredAccess::GENERIC_READ,
            allocation_size: 0,
            file_attributes: FileAttributes::empty(),
            shared_access: SharedAccess::empty(),
            create_disposition: CreateDisposition::FILE_OPEN,
            create_options: CreateOptions::empty(),
            path: "\\".into(),
        };
        assert!(says(&answer(&mut backend, elsewhere), NtStatus::ACCESS_DENIED));
        let _ = std::fs::remove_file(outside);
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn a_file_that_isnt_there_is_said_not_to_be_and_one_that_is_isnt_made_again() {
        let (mut backend, root) = drive("missing");
        std::fs::write(root.join("here.txt"), "x").unwrap();
        assert_eq!(open_existing(&mut backend, "\\gone.txt").0, NtStatus::NO_SUCH_FILE);
        assert_eq!(open_existing(&mut backend, "\\No Folder\\gone.txt").0, NtStatus::from(OBJECT_PATH_NOT_FOUND));
        let again = open(&mut backend, "\\here.txt", CreateDisposition::FILE_CREATE, CreateOptions::empty(), DesiredAccess::GENERIC_WRITE);
        assert_eq!(again.0, NtStatus::OBJECT_NAME_COLLISION);
        assert_eq!(std::fs::read_to_string(root.join("here.txt")).unwrap(), "x");
        // a file opened as a folder, and a folder as a file
        let as_folder = open(&mut backend, "\\here.txt", CreateDisposition::FILE_OPEN, CreateOptions::FILE_DIRECTORY_FILE, DesiredAccess::GENERIC_READ);
        assert_eq!(as_folder.0, NtStatus::NOT_A_DIRECTORY);
        let as_file = open(&mut backend, "\\", CreateDisposition::FILE_OPEN, CreateOptions::FILE_NON_DIRECTORY_FILE, DesiredAccess::GENERIC_READ);
        assert_eq!(as_file.0, NtStatus::from(FILE_IS_A_DIRECTORY));
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn the_drives_are_offered_to_the_remote_desktop_once_the_user_is_logged_on() {
        let drives = [Drive { name: "C".into(), root: std::env::temp_dir() }, Drive { name: "Z".into(), root: std::env::temp_dir() }];
        let mut channel = channel(&drives);
        // "user logged on", as the server sends it: the core component (rD) and its packet id (LU), nothing more
        let offered = channel.process(&[0x72, 0x44, 0x4c, 0x55]).unwrap();
        assert_eq!(offered.len(), 1, "one list of devices");
        let bytes = offered[0].encode_unframed_pdu().unwrap();
        // the device list (rD, AD), two devices, the first a file system (8) with id 1
        assert_eq!(&bytes[..4], &[0x72, 0x44, 0x41, 0x44]);
        assert_eq!(&bytes[4..8], &2u32.to_le_bytes());
        assert_eq!(&bytes[8..12], &8u32.to_le_bytes());
        assert_eq!(&bytes[12..16], &1u32.to_le_bytes());
        // then 8 bytes of DOS name, the full name's length, and the name with a NUL after it
        assert_eq!(&bytes[24..30], &[2, 0, 0, 0, b'C', 0]);
        assert_eq!(&bytes[bytes.len() - 2..], &[b'Z', 0]);
        // told once: the message coming again doesn't list them twice
        assert!(channel.process(&[0x72, 0x44, 0x4c, 0x55]).unwrap().is_empty());
    }

    #[test]
    fn a_request_the_channel_cant_read_is_refused_and_the_desktop_stays() {
        let mut channel = channel(&[Drive { name: "C".into(), root: std::env::temp_dir() }]);
        // a flush (major function 9, which IronRDP doesn't know) of file 3 on drive 1, to be answered as 77
        let mut flush = vec![0x72, 0x44, 0x52, 0x49];
        for number in [1u32, 3, 77, 9, 0] {
            flush.extend_from_slice(&number.to_le_bytes());
        }
        // an error here would end the session
        let answers = channel.process(&flush).expect("the session goes on");
        assert_eq!(answers.len(), 1);
        let bytes = answers[0].encode_unframed_pdu().unwrap();
        // an I/O completion (rD, CI) for drive 1 and request 77: not supported
        assert_eq!(&bytes[..4], &[0x72, 0x44, 0x43, 0x49]);
        assert_eq!(&bytes[4..8], &1u32.to_le_bytes());
        assert_eq!(&bytes[8..12], &77u32.to_le_bytes());
        assert_eq!(&bytes[12..16], &0xC000_00BBu32.to_le_bytes());
        // a message that makes no sense at all is passed over
        assert!(channel.process(&[0xff; 3]).expect("the session goes on").is_empty());
    }

    #[test]
    fn this_computers_drives_are_its_drive_letters_on_windows_and_the_home_folder_elsewhere() {
        let drives = local_drives();
        if cfg!(windows) {
            // the drive Windows is on (C, nearly always), by its letter alone
            let system = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
            let letter = system.trim_end_matches(':').to_uppercase();
            let found = drives.iter().find(|drive| drive.name == letter).expect("the system drive is shared");
            assert_eq!(found.root, PathBuf::from(format!("{letter}:\\")));
        } else {
            assert_eq!(drives[0], Drive { name: "Home".into(), root: dirs::home_dir().unwrap() });
        }
        assert!(drives.iter().all(|drive| drive.root.is_absolute() && !drive.name.is_empty()));
    }

    #[test]
    fn a_pattern_matches_names_as_windows_does() {
        for (pattern, name, expected) in [
            ("*", "anything", true),
            ("*.*", "no-dot", true),
            ("*.txt", "Notes.TXT", true),
            ("*.txt", "notes.txt.bak", false),
            ("report?.pdf", "report1.pdf", true),
            ("report?.pdf", "report10.pdf", false),
            ("a*b*c", "a-b-b-c", true),
            ("a*b*c", "a-b-b", false),
            ("desktop.ini", "Desktop.ini", true),
            ("<.txt", "notes.txt", true),
        ] {
            assert_eq!(matches(pattern, name), expected, "{pattern} against {name}");
        }
    }
}
