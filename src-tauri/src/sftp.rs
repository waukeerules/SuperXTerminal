//! File transfer, as SuperXPuTTY's File transfer (pscp): a session's files beside this Mac's, copied either way.
//!
//! The connection is OpenSSH's own (`ssh -s <host> sftp`, as sshfs does), started with the same arguments as the
//! session's terminal tab (`launch.rs`), so keys, ssh-agent, ~/.ssh/config, jump hosts, older algorithms and
//! known_hosts behave the same. The session's password, and any question ssh asks (a new host key, a passphrase),
//! go through `askpass.rs`, asked in a dialog. SFTP itself is spoken over ssh's pipes by russh-sftp (Apache-2.0).
//! Windows ships OpenSSH too, so this carries over to a Windows build.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, UNIX_EPOCH};

use futures_util::stream::{self, StreamExt};
use russh_sftp::client::error::Error as SftpError;
use russh_sftp::client::RawSftpSession;
use russh_sftp::protocol::{FileAttributes, OpenFlags, StatusCode};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, State};

use crate::sessions::Store;
use crate::AppState;

/// Bytes a read or write asks for, and how many are in flight at once
const CHUNK: u32 = 32 * 1024;
/// How many reads or writes of [`CHUNK`] are sent before waiting for the first answer
const IN_FLIGHT: usize = 16;

/// One open file transfer: its connection, and the window that asks the user its questions
struct Conn {
    /// SFTP: the protocol over ssh, and the ssh
    raw: Option<Arc<RawSftpSession>>,
    child: Option<tokio::sync::Mutex<tokio::process::Child>>,
    /// SCP (a device with no SFTP): the session's ssh arguments, run again for each listing and copy
    scp: Option<Vec<String>>,
    token: String,
    window: String,
}

impl Conn {
    /// The SFTP session, or why there's none: a device with only SCP can copy but not rename, delete or make folders
    fn raw(&self) -> Result<&RawSftpSession, String> {
        self.raw.as_deref().ok_or_else(|| "This device only offers SCP, which can copy files but not do this.".to_string())
    }
}

/// Every open file transfer, by the id given to its tab, and the transfers that can be cancelled
#[derive(Default)]
pub struct Sftps {
    conns: Mutex<HashMap<u32, Arc<Conn>>>,
    cancels: Mutex<HashMap<u32, Arc<AtomicBool>>>,
}

/// The next id given to a tab's connection, a question or a transfer
static NEXT: AtomicU32 = AtomicU32::new(1);

/// What a tab opens on
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    id: u32,
    /// The remote home folder, and where the remote pane starts (the session's remote path, else home)
    home: String,
    remote: String,
    /// Where the local pane starts (the session's local path, else this Mac's home)
    local: String,
    /// user@host, for the tab
    address: String,
    /// SCP instead of SFTP: files listed with the device's `dir`, copied one at a time, nothing else
    scp: bool,
}

/// One file or folder in a pane
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Entry {
    name: String,
    dir: bool,
    link: bool,
    size: u64,
    /// Seconds since 1970; 0 when unknown
    modified: u64,
    /// The time as the device wrote it, when it isn't a time (SCP's `dir`)
    date: String,
    /// As `ls -l` shows them (drwxr-xr-x)
    permissions: String,
    owner: String,
    group: String,
}

/// A folder's contents, at its full path
#[derive(Serialize)]
pub struct Listing {
    path: String,
    entries: Vec<Entry>,
}

/// A transfer's progress: its total once counted, then the bytes done and the file it's on
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Progress {
    Total { bytes: u64, files: u64 },
    Done { bytes: u64, file: String },
}

/// A question ssh asks while connecting, for the window to put to the user (`sftp_answer` replies)
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Ask {
    ask_id: u32,
    title: String,
    prompt: String,
    secret: bool,
}

/// The questions waiting for the user's answer, by id
static ASKS: Mutex<Option<HashMap<u32, std::sync::mpsc::Sender<Option<String>>>>> = Mutex::new(None);

/// Ask the user in the window, waiting up to ten minutes
fn ask_user(app: &AppHandle, window: &str, title: &str, prompt: &str, secret: bool) -> Option<String> {
    let ask_id = NEXT.fetch_add(1, Ordering::Relaxed);
    let (tx, rx) = std::sync::mpsc::channel();
    ASKS.lock().unwrap().get_or_insert_with(HashMap::new).insert(ask_id, tx);
    let _ = app.emit_to(window, "sftp-ask", Ask { ask_id, title: title.into(), prompt: prompt.trim().into(), secret });
    let answer = rx.recv_timeout(Duration::from_secs(600)).ok().flatten();
    if let Some(asks) = ASKS.lock().unwrap().as_mut() {
        asks.remove(&ask_id);
    }
    answer
}

/// The user's answer to an `sftp-ask` (None: cancelled)
#[tauri::command]
pub fn sftp_answer(ask_id: u32, answer: Option<String>) {
    if let Some(tx) = ASKS.lock().unwrap().as_mut().and_then(|asks| asks.remove(&ask_id)) {
        let _ = tx.send(answer);
    }
}

/// An SFTP error in the user's words: "No such file or folder" rather than the status code
fn plain(error: SftpError) -> String {
    match error {
        SftpError::Status(status) => match status.status_code {
            StatusCode::NoSuchFile => "No such file or folder".into(),
            StatusCode::PermissionDenied => "Permission denied".into(),
            _ if !status.error_message.is_empty() => status.error_message,
            code => code.to_string(),
        },
        other => other.to_string(),
    }
}

/// The tab's connection, or say it's closed
fn conn(sftps: &Sftps, id: u32) -> Result<Arc<Conn>, String> {
    sftps.conns.lock().unwrap().get(&id).cloned().ok_or_else(|| "The file transfer has closed.".to_string())
}

/// Connect to a session's files
#[tauri::command]
pub async fn sftp_open(
    app: AppHandle,
    window: tauri::Window,
    state: State<'_, AppState>,
    sftps: State<'_, Sftps>,
    store: Store,
    id: String,
) -> Result<Opened, String> {
    if crate::demo::on() {
        let sessions = state.sessions.lock().unwrap();
        let session = sessions.find(store, &id).ok_or_else(|| format!("The session {id} is gone."))?;
        return demo_open(session);
    }
    let (launch, title, local, remote) = {
        let profiles = state.profiles.lock().unwrap().clone();
        let values = crate::variables::values(&state.user_folder);
        let (serial_devices, overrides_on) = {
            let settings = state.settings.lock().unwrap();
            (settings.serial_devices.clone(), settings.use_local_overrides)
        };
        let sessions = state.sessions.lock().unwrap();
        let stored = sessions.find(store, &id).ok_or_else(|| format!("The session {id} is gone."))?;
        let session = crate::overrides::applied(&state.user_folder, stored, overrides_on);
        let launch = crate::launch::launch(&sessions, &profiles, &values, &serial_devices, store, &session)?;
        if launch.login.is_none() {
            return Err("File transfer is for SSH sessions.".into());
        }
        let local = crate::variables::substitute(session.get("LocalPath").trim(), &values);
        let remote = crate::variables::substitute(session.get("RemotePath").trim(), &values);
        (launch, session.get("SessionName").to_string(), local, remote)
    };
    let login = launch.login.clone().unwrap_or_default();

    // the terminal's ssh, without its terminal, running the sftp subsystem
    let base: Vec<String> = launch.command.get_argv().iter().skip(1).map(|w| w.to_string_lossy().to_string()).filter(|w| w != "-tt").collect();
    let base = with_keepalive(without_forwarding(base));
    let mut args = base.clone();
    args.splice(0..0, ["-s".to_string(), "-T".to_string()]);
    args.push("sftp".into());

    let label = window.label().to_string();
    let (app_for_ask, label_for_ask, title_for_ask) = (app.clone(), label.clone(), title.clone());
    let token = crate::askpass::register_asking(
        launch.password.as_deref(),
        Arc::new(move |prompt: &str, secret: bool| ask_user(&app_for_ask, &label_for_ask, &title_for_ask, prompt, secret)),
    )?;
    let program = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = tokio::process::Command::new(crate::launch::openssh("ssh"))
        .args(&args)
        .env("SSH_ASKPASS", &program)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env(crate::askpass::TOKEN_VAR, &token)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("Couldn't start ssh: {e}"))?;

    // what ssh says on its way (refused, unreachable), for the error if it fails
    let said = Arc::new(Mutex::new(String::new()));
    if let Some(stderr) = child.stderr.take() {
        let said = said.clone();
        tokio::spawn(async move {
            use tokio::io::AsyncBufReadExt;
            let mut lines = tokio::io::BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let mut said = said.lock().unwrap();
                said.push_str(&line);
                said.push('\n');
            }
        });
    }
    let (stdin, stdout) = (child.stdin.take().unwrap(), child.stdout.take().unwrap());
    let raw = Arc::new(RawSftpSession::new(tokio::io::join(stdout, stdin)));
    // logging in can wait on the user (a host key, a password)
    raw.set_timeout(600);
    let local = if local.is_empty() { dirs::home_dir().unwrap_or_default().to_string_lossy().to_string() } else { local };
    let address = if login.username.is_empty() { login.host.clone() } else { format!("{}@{}", login.username, login.host) };
    if let Err(e) = raw.init().await {
        // no SFTP: a device with SCP and a `dir` command (Cisco IOS and the like) is used that way
        let _ = child.start_kill();
        let probe = scp_list(&base, &token, "").await;
        if let Err(why) = &probe {
            crate::applog::info(format!("{address} has no SFTP, and isn't a device that lists files with dir: {why}"));
        }
        if let Ok(listing) = probe {
            crate::applog::info(format!("File transfer opened to {address} over SCP (no SFTP there)"));
            let home = listing.path.clone();
            let remote = if remote.is_empty() { home.clone() } else { remote };
            let id = NEXT.fetch_add(1, Ordering::Relaxed);
            sftps.conns.lock().unwrap().insert(id, Arc::new(Conn { raw: None, child: None, scp: Some(base), token, window: label }));
            return Ok(Opened { id, home, remote, local, address, scp: true });
        }
        crate::askpass::forget(&token);
        // ssh's own words, once it has ended
        let _ = tokio::time::timeout(Duration::from_secs(3), child.wait()).await;
        tokio::time::sleep(Duration::from_millis(200)).await;
        let said = said.lock().unwrap().trim().to_string();
        // a Cisco device that refused dir: logged in, but not at privilege 15
        let refused = probe.as_ref().err().filter(|why| why.contains("invalid autocommand")).cloned();
        let reason = if let Some(why) = refused {
            why
        } else if !said.is_empty() && !said.to_lowercase().starts_with("connection closed") {
            said
        } else if matches!(e, SftpError::UnexpectedBehavior(_) | SftpError::IO(_)) || said.to_lowercase().starts_with("connection closed") {
            // logged in, then dropped: no SFTP there (network devices often have only SCP), or the login was refused
            format!(
                "{} closed the connection. It may not offer SFTP (many switches and routers only offer SCP), or it refused the login.",
                login.host
            )
        } else {
            plain(e)
        };
        return Err(format!("Couldn't open the file transfer: {reason}"));
    }
    raw.set_timeout(60);
    let home = raw.realpath(".").await.map_err(plain)?.files.into_iter().next().map(|f| f.filename).unwrap_or_else(|| "/".into());
    let remote = if remote.is_empty() { home.clone() } else { remote };
    crate::applog::info(format!("File transfer opened to {address}"));

    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    sftps.conns.lock().unwrap().insert(id, Arc::new(Conn { raw: Some(raw), child: Some(tokio::sync::Mutex::new(child)), scp: None, token, window: label }));
    Ok(Opened { id, home, remote, local, address, scp: false })
}

/// The tab closed: disconnect
#[tauri::command]
pub async fn sftp_close(sftps: State<'_, Sftps>, id: u32) -> Result<(), String> {
    if let Some(demo) = DEMO.lock().unwrap().as_mut() {
        demo.remove(&id);
    }
    let conn = sftps.conns.lock().unwrap().remove(&id);
    if let Some(conn) = conn {
        crate::askpass::forget(&conn.token);
        if let Some(raw) = &conn.raw {
            let _ = raw.close_session();
        }
        if let Some(child) = &conn.child {
            let _ = child.lock().await.kill().await;
        }
    }
    Ok(())
}

/// A window closed: its file transfers disconnect
pub fn close_window(sftps: &Sftps, label: &str) {
    sftps.conns.lock().unwrap().retain(|_, c| {
        if c.window == label {
            crate::askpass::forget(&c.token);
            if let Some(raw) = &c.raw {
                let _ = raw.close_session();
            }
            if let Some(Ok(mut child)) = c.child.as_ref().map(|c| c.try_lock()) {
                let _ = child.start_kill();
            }
            false
        } else {
            true
        }
    });
}

/// `drwxr-xr-x` from a mode
fn mode_text(mode: u32) -> String {
    let kind = match mode & 0o170000 {
        0o040000 => 'd',
        0o120000 => 'l',
        _ => '-',
    };
    let mut text = String::from(kind);
    for shift in [6, 3, 0] {
        let bits = (mode >> shift) & 7;
        text.push(if bits & 4 != 0 { 'r' } else { '-' });
        text.push(if bits & 2 != 0 { 'w' } else { '-' });
        text.push(if bits & 1 != 0 { 'x' } else { '-' });
    }
    text
}

/// A remote path joined to a name
fn join(dir: &str, name: &str) -> String {
    if dir.ends_with('/') { format!("{dir}{name}") } else { format!("{dir}/{name}") }
}

/// Everything in a remote folder
async fn read_dir(raw: &RawSftpSession, path: &str) -> Result<Vec<russh_sftp::protocol::File>, String> {
    let handle = raw.opendir(path).await.map_err(plain)?.handle;
    let mut files = Vec::new();
    loop {
        match raw.readdir(handle.as_str()).await {
            Ok(name) => files.extend(name.files),
            Err(SftpError::Status(s)) if s.status_code == StatusCode::Eof => break,
            Err(e) => {
                let _ = raw.close(handle).await;
                return Err(plain(e));
            }
        }
    }
    let _ = raw.close(handle).await;
    files.retain(|f| f.filename != "." && f.filename != "..");
    Ok(files)
}

/// A remote folder's files, its path made absolute (`..` and `~` resolved)
#[tauri::command]
pub async fn sftp_list(sftps: State<'_, Sftps>, id: u32, path: String) -> Result<Listing, String> {
    if let Some(device) = demo_device(id) {
        return Ok(demo_list(&device, path.trim()));
    }
    let conn = conn(&sftps, id)?;
    if let Some(base) = &conn.scp {
        return scp_list(base, &conn.token, path.trim()).await;
    }
    let raw = conn.raw()?;
    let path = raw.realpath(if path.trim().is_empty() { "." } else { path.trim() }).await.map_err(plain)?.files.into_iter().next().map(|f| f.filename).unwrap_or(path);
    let mut entries = Vec::new();
    for file in read_dir(raw, &path).await? {
        let attrs = &file.attrs;
        let link = attrs.is_symlink();
        // a link shows as what it points to
        let target = if link { raw.stat(join(&path, &file.filename)).await.ok().map(|a| a.attrs) } else { None };
        let shown = target.as_ref().unwrap_or(attrs);
        // OpenSSH's long name (as ls -l) has the owner and group names
        let words: Vec<&str> = file.longname.split_whitespace().collect();
        let (owner, group) = if words.len() >= 4 {
            (words[2].to_string(), words[3].to_string())
        } else {
            (attrs.uid.map(|u| u.to_string()).unwrap_or_default(), attrs.gid.map(|g| g.to_string()).unwrap_or_default())
        };
        entries.push(Entry {
            name: file.filename.clone(),
            dir: shown.is_dir(),
            link,
            size: shown.size.unwrap_or(0),
            modified: shown.mtime.unwrap_or(0) as u64,
            date: String::new(),
            permissions: attrs.permissions.map(mode_text).unwrap_or_default(),
            owner,
            group,
        });
    }
    Ok(Listing { path, entries })
}

/// A new remote folder
#[tauri::command]
pub async fn sftp_mkdir(sftps: State<'_, Sftps>, id: u32, path: String) -> Result<(), String> {
    if demo_device(id).is_some() {
        return Err(DEMO_UNCHANGED.into());
    }
    conn(&sftps, id)?.raw()?.mkdir(path, FileAttributes::empty()).await.map(|_| ()).map_err(plain)
}

/// Rename or move a remote file or folder
#[tauri::command]
pub async fn sftp_rename(sftps: State<'_, Sftps>, id: u32, from: String, to: String) -> Result<(), String> {
    if demo_device(id).is_some() {
        return Err(DEMO_UNCHANGED.into());
    }
    conn(&sftps, id)?.raw()?.rename(from, to).await.map(|_| ()).map_err(plain)
}

/// Delete a remote file, or a folder and everything in it
#[tauri::command]
pub async fn sftp_delete(sftps: State<'_, Sftps>, id: u32, path: String) -> Result<(), String> {
    if demo_device(id).is_some() {
        return Err(DEMO_UNCHANGED.into());
    }
    let conn = conn(&sftps, id)?;
    match &conn.scp {
        Some(base) => scp_delete(base, &conn.token, &path).await,
        None => delete(conn.raw()?, &path).await,
    }
}

/// Delete a file, or a folder and everything in it
async fn delete(raw: &RawSftpSession, path: &str) -> Result<(), String> {
    let attrs = raw.lstat(path).await.map_err(plain)?.attrs;
    if attrs.is_dir() {
        for file in read_dir(raw, path).await? {
            Box::pin(delete(raw, &join(path, &file.filename))).await?;
        }
        raw.rmdir(path).await.map(|_| ()).map_err(plain)
    } else {
        raw.remove(path).await.map(|_| ()).map_err(plain)
    }
}

/// Stop a transfer
#[tauri::command]
pub fn sftp_cancel(sftps: State<'_, Sftps>, transfer: u32) {
    if let Some(flag) = sftps.cancels.lock().unwrap().get(&transfer) {
        flag.store(true, Ordering::Relaxed);
    }
}

/// One file to copy: where from, where to, its size and time
struct Item {
    from: String,
    to: String,
    size: u64,
    modified: u64,
}

/// Copy files and folders into a folder: up (this Mac's paths to the remote folder) or down. Progress goes to
/// `on_progress`; folders are copied with everything in them.
#[tauri::command]
pub async fn sftp_transfer(
    sftps: State<'_, Sftps>,
    id: u32,
    transfer: u32,
    upload: bool,
    sources: Vec<String>,
    // a download's sizes, from the remote listing (SCP can't tell them)
    sizes: Option<Vec<u64>>,
    target: String,
    on_progress: Channel<Progress>,
) -> Result<(), String> {
    if demo_device(id).is_some() {
        let cancel = Arc::new(AtomicBool::new(false));
        sftps.cancels.lock().unwrap().insert(transfer, cancel.clone());
        let result = demo_transfer(upload, &sources, &sizes.unwrap_or_default(), &target, &cancel, &on_progress).await;
        sftps.cancels.lock().unwrap().remove(&transfer);
        return result;
    }
    let conn = conn(&sftps, id)?;
    let cancel = Arc::new(AtomicBool::new(false));
    sftps.cancels.lock().unwrap().insert(transfer, cancel.clone());
    let result = match &conn.scp {
        Some(base) => scp_transfer(base, &conn.token, upload, &sources, &sizes.unwrap_or_default(), &target, &cancel, &on_progress).await,
        None => run_transfer(conn.raw()?, upload, &sources, &target, &cancel, &on_progress).await,
    };
    sftps.cancels.lock().unwrap().remove(&transfer);
    result
}

/// Copy files and folders over SFTP: count them all first, for the progress bar, then copy each, stopping if cancelled
async fn run_transfer(
    raw: &RawSftpSession,
    upload: bool,
    sources: &[String],
    target: &str,
    cancel: &AtomicBool,
    on_progress: &Channel<Progress>,
) -> Result<(), String> {
    // the files, folders made on the way
    let mut items = Vec::new();
    for source in sources {
        let name = source.trim_end_matches('/').rsplit(['/', std::path::MAIN_SEPARATOR]).next().unwrap_or(source).to_string();
        if upload {
            gather_local(raw, std::path::Path::new(source), &join(target, &name), &mut items).await?;
        } else {
            gather_remote(raw, source, &std::path::Path::new(target).join(&name), &mut items).await?;
        }
    }
    let total: u64 = items.iter().map(|i| i.size).sum();
    let _ = on_progress.send(Progress::Total { bytes: total, files: items.len() as u64 });
    let mut done = 0u64;
    for item in &items {
        if cancel.load(Ordering::Relaxed) {
            return Err("Cancelled".into());
        }
        let file = item.from.rsplit(['/', std::path::MAIN_SEPARATOR]).next().unwrap_or(&item.from).to_string();
        let report = |bytes: u64| {
            let _ = on_progress.send(Progress::Done { bytes: done + bytes, file: file.clone() });
        };
        if upload {
            upload_file(raw, item, cancel, &report).await?;
        } else {
            download_file(raw, item, cancel, &report).await?;
        }
        done += item.size;
        let _ = on_progress.send(Progress::Done { bytes: done, file: file.clone() });
    }
    Ok(())
}

/// This Mac's file or folder (made on the remote side as it's found), for an upload
async fn gather_local(raw: &RawSftpSession, from: &std::path::Path, to: &str, items: &mut Vec<Item>) -> Result<(), String> {
    let meta = std::fs::metadata(from).map_err(|e| format!("{}: {e}", from.display()))?;
    if meta.is_dir() {
        // already there is fine
        let _ = raw.mkdir(to, FileAttributes::empty()).await;
        let mut children: Vec<_> = std::fs::read_dir(from).map_err(|e| e.to_string())?.flatten().map(|e| e.path()).collect();
        children.sort();
        for child in children {
            let name = child.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            Box::pin(gather_local(raw, &child, &join(to, &name), items)).await?;
        }
    } else {
        let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        items.push(Item { from: from.to_string_lossy().to_string(), to: to.to_string(), size: meta.len(), modified });
    }
    Ok(())
}

/// A remote file or folder (made on this Mac as it's found), for a download
async fn gather_remote(raw: &RawSftpSession, from: &str, to: &std::path::Path, items: &mut Vec<Item>) -> Result<(), String> {
    let attrs = raw.stat(from).await.map_err(|e| format!("{from}: {}", plain(e)))?.attrs;
    if attrs.is_dir() {
        std::fs::create_dir_all(to).map_err(|e| format!("{}: {e}", to.display()))?;
        let mut children = read_dir(raw, from).await?;
        children.sort_by(|a, b| a.filename.cmp(&b.filename));
        for child in children {
            Box::pin(gather_remote(raw, &join(from, &child.filename), &to.join(&child.filename), items)).await?;
        }
    } else {
        items.push(Item { from: from.to_string(), to: to.to_string_lossy().to_string(), size: attrs.size.unwrap_or(0), modified: attrs.mtime.unwrap_or(0) as u64 });
    }
    Ok(())
}

/// Write at an offset, on any system
fn write_at(file: &std::fs::File, data: &[u8], offset: u64) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        std::os::unix::fs::FileExt::write_all_at(file, data, offset)
    }
    #[cfg(windows)]
    {
        let mut written = 0;
        while written < data.len() {
            written += std::os::windows::fs::FileExt::seek_write(file, &data[written..], offset + written as u64)?;
        }
        Ok(())
    }
}

/// Read at an offset (fewer bytes only at the end), on any system
fn read_at(file: &std::fs::File, buffer: &mut [u8], offset: u64) -> std::io::Result<usize> {
    #[cfg(unix)]
    {
        let mut read = 0;
        while read < buffer.len() {
            let n = std::os::unix::fs::FileExt::read_at(file, &mut buffer[read..], offset + read as u64)?;
            if n == 0 {
                break;
            }
            read += n;
        }
        Ok(read)
    }
    #[cfg(windows)]
    {
        let mut read = 0;
        while read < buffer.len() {
            let n = std::os::windows::fs::FileExt::seek_read(file, &mut buffer[read..], offset + read as u64)?;
            if n == 0 {
                break;
            }
            read += n;
        }
        Ok(read)
    }
}

/// Download one file, several reads in flight, into place
async fn download_file(raw: &RawSftpSession, item: &Item, cancel: &AtomicBool, report: &impl Fn(u64)) -> Result<(), String> {
    let local = std::fs::File::create(&item.to).map_err(|e| format!("{}: {e}", item.to))?;
    let handle = raw.open(item.from.as_str(), OpenFlags::READ, FileAttributes::empty()).await.map_err(|e| format!("{}: {}", item.from, plain(e)))?.handle;
    let offsets: Vec<u64> = (0..item.size.div_ceil(CHUNK as u64)).map(|i| i * CHUNK as u64).collect();
    let mut reads = stream::iter(offsets).map(|offset| {
        let handle = handle.clone();
        async move { (offset, raw.read(handle, offset, CHUNK).await) }
    }).buffered(IN_FLIGHT);
    let mut done = 0u64;
    let mut result = Ok(());
    while let Some((offset, read)) = reads.next().await {
        if cancel.load(Ordering::Relaxed) {
            result = Err("Cancelled".to_string());
            break;
        }
        let mut data = match read {
            Ok(data) => data.data,
            Err(SftpError::Status(s)) if s.status_code == StatusCode::Eof => Vec::new(),
            Err(e) => {
                result = Err(plain(e));
                break;
            }
        };
        // a short read before the end: the rest of the chunk, in turn
        while data.len() < CHUNK as usize && offset + (data.len() as u64) < item.size {
            match raw.read(handle.as_str(), offset + data.len() as u64, CHUNK - data.len() as u32).await {
                Ok(more) if !more.data.is_empty() => data.extend(more.data),
                _ => break,
            }
        }
        if let Err(e) = write_at(&local, &data, offset) {
            result = Err(e.to_string());
            break;
        }
        done += data.len() as u64;
        report(done);
    }
    drop(reads);
    let _ = raw.close(handle).await;
    result?;
    if item.modified != 0 {
        let _ = local.set_modified(UNIX_EPOCH + Duration::from_secs(item.modified));
    }
    Ok(())
}

/// Upload one file, several writes in flight
async fn upload_file(raw: &RawSftpSession, item: &Item, cancel: &AtomicBool, report: &impl Fn(u64)) -> Result<(), String> {
    let local = std::fs::File::open(&item.from).map_err(|e| format!("{}: {e}", item.from))?;
    let flags = OpenFlags::WRITE | OpenFlags::CREATE | OpenFlags::TRUNCATE;
    let handle = raw.open(item.to.as_str(), flags, FileAttributes::empty()).await.map_err(|e| format!("{}: {}", item.to, plain(e)))?.handle;
    let offsets: Vec<u64> = (0..item.size.div_ceil(CHUNK as u64)).map(|i| i * CHUNK as u64).collect();
    let local = &local;
    let mut writes = stream::iter(offsets).map(|offset| {
        let handle = handle.clone();
        async move {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".to_string());
            }
            let mut buffer = vec![0u8; CHUNK as usize];
            let n = read_at(local, &mut buffer, offset).map_err(|e| e.to_string())?;
            buffer.truncate(n);
            raw.write(handle, offset, buffer).await.map_err(plain)?;
            Ok(n as u64)
        }
    }).buffer_unordered(IN_FLIGHT);
    let mut done = 0u64;
    let mut result = Ok(());
    while let Some(written) = writes.next().await {
        match written {
            Ok(n) => {
                done += n;
                report(done);
            }
            Err(e) => {
                result = Err(e);
                break;
            }
        }
    }
    drop(writes);
    let _ = raw.close(handle.as_str()).await;
    result?;
    if item.modified != 0 {
        let attrs = FileAttributes { atime: Some(item.modified as u32), mtime: Some(item.modified as u32), ..FileAttributes::empty() };
        let _ = raw.setstat(item.to.as_str(), attrs).await;
    }
    Ok(())
}

// ---- SCP, for devices with no SFTP ----

/// The tab's ssh arguments without its forwarding, for a file transfer's own connection: the tab already holds the
/// profile's tunnels (a second -L on the same port fails with "address already in use"), and an X11 display or the
/// agent has nothing to do with copying files. ClearAllForwardings drops the tunnels whichever way they were given
/// (the profile's Tunnels, its SSH options, ~/.ssh/config); -A and -X are taken out, as ssh can't be told to undo them
fn without_forwarding(base: Vec<String>) -> Vec<String> {
    let mut args: Vec<String> = base.into_iter().filter(|w| w != "-A" && w != "-X").collect();
    args.splice(0..0, ["-o".to_string(), "ClearAllForwardings=yes".to_string()]);
    args
}

/// Keepalives for every ssh and scp a file transfer runs, so a long copy (or an idle tab) isn't dropped by a
/// firewall: the profile's Keepalive when it sets one (already in the arguments), else every 30 seconds; ssh gives up
/// after 4 unanswered (two minutes), as a slow device writing its flash can pause that long
fn with_keepalive(mut base: Vec<String>) -> Vec<String> {
    if !base.iter().any(|w| w.starts_with("ServerAliveInterval=")) {
        base.splice(0..0, ["-o".to_string(), "ServerAliveInterval=30".to_string()]);
    }
    if !base.iter().any(|w| w.starts_with("ServerAliveCountMax=")) {
        base.splice(0..0, ["-o".to_string(), "ServerAliveCountMax=4".to_string()]);
    }
    base
}

/// ssh or scp with the connection's login (askpass.rs): the password, and the user asked anything else
fn with_login(program: &str, token: &str) -> Result<tokio::process::Command, String> {
    let helper = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut command = tokio::process::Command::new(crate::launch::openssh(program));
    command
        .env("SSH_ASKPASS", helper)
        .env("SSH_ASKPASS_REQUIRE", "force")
        .env(crate::askpass::TOKEN_VAR, token)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true);
    Ok(command)
}

/// A folder on the device, from its `dir` (Cisco IOS: "Directory of flash:/" and a line a file). With no path, its
/// flash; `show` is tried when `dir` isn't allowed (a login below privilege 15)
async fn scp_list(base: &[String], token: &str, path: &str) -> Result<Listing, String> {
    // IOS takes "flash:" as a command's argument but not the "flash:/" its dir shows
    let path = if path.is_empty() { "flash:" } else { path.strip_suffix('/').filter(|p| p.ends_with(':')).unwrap_or(path) };
    let mut said = String::new();
    for verb in ["dir", "show"] {
        let mut command = with_login("ssh", token)?;
        command.arg("-T").args(base).arg(format!("{verb} {path}"));
        let output = tokio::time::timeout(Duration::from_secs(600), command.output())
            .await
            .map_err(|_| "The device didn't answer.".to_string())?
            .map_err(|e| format!("Couldn't start ssh: {e}"))?;
        let text = String::from_utf8_lossy(&output.stdout);
        if let Some(listing) = parse_dir(&text) {
            return Ok(listing);
        }
        said = format!("{}{}", String::from_utf8_lossy(&output.stderr).trim(), text.trim());
    }
    Err(if said.is_empty() { format!("{path}: no listing") } else { privilege_hint(&said) })
}

/// A file on the device deleted with its own command, over the session's ssh: `delete /force` asks nothing (IOS's
/// plain delete asks to confirm, which a command run this way can't answer). SCP itself can't delete.
async fn scp_delete(base: &[String], token: &str, path: &str) -> Result<(), String> {
    if path.chars().any(|c| c.is_whitespace() || c == '"') {
        return Err(format!("{path}: a name with spaces can't be deleted this way."));
    }
    let mut command = with_login("ssh", token)?;
    command.arg("-T").args(base).arg(format!("delete /force {path}"));
    let output = tokio::time::timeout(Duration::from_secs(120), command.output())
        .await
        .map_err(|_| "The device didn't answer.".to_string())?
        .map_err(|e| format!("Couldn't start ssh: {e}"))?;
    let said = format!("{}\n{}", String::from_utf8_lossy(&output.stdout).trim(), String::from_utf8_lossy(&output.stderr).trim());
    let said = said.trim();
    // IOS reports a failure as "%Error deleting …" or "% Invalid input …", and the command still ends cleanly
    let failed = said.lines().any(|l| l.trim_start().starts_with('%') || l.contains("invalid autocommand"));
    if failed || (!output.status.success() && !said.to_lowercase().contains("closed by remote host")) {
        let reason = said.lines().filter(|l| !l.to_lowercase().contains("closed by remote host")).collect::<Vec<_>>().join(" ");
        return Err(format!("{path}: {}", privilege_hint(if reason.trim().is_empty() { "the delete failed" } else { reason.trim() })));
    }
    Ok(())
}

/// A device's refusal, with what usually fixes it: SCP and commands like delete need a privilege 15 login on IOS
fn privilege_hint(said: &str) -> String {
    let lower = said.to_lowercase();
    let likely = ["invalid autocommand", "administratively", "authorization failed", "privilege", "permission denied", "connection closed", "lost connection"];
    if likely.iter().any(|w| lower.contains(w)) {
        format!(
            "{said}. On Cisco IOS this usually means the login isn't privilege 15: give the user privilege 15 (username <name> privilege 15) or allow it through AAA (aaa authorization exec), and check ip scp server enable."
        )
    } else {
        said.to_string()
    }
}

/// A Cisco-style `dir`: the folder from "Directory of …", then "index perms size date… name" lines
fn parse_dir(text: &str) -> Option<Listing> {
    let path = text.lines().find_map(|l| l.trim().strip_prefix("Directory of ").map(|p| p.trim().to_string()))?;
    let mut entries = Vec::new();
    for line in text.lines() {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.len() < 5 || words[0].parse::<u64>().is_err() {
            continue;
        }
        let perms = words[1];
        if !perms.starts_with(['-', 'd']) || !perms[1..].chars().all(|c| "rwx-".contains(c)) {
            continue;
        }
        let Ok(size) = words[2].parse::<u64>() else { continue };
        let name = words[words.len() - 1].to_string();
        entries.push(Entry {
            name,
            dir: perms.starts_with('d'),
            link: false,
            size,
            modified: 0,
            date: words[3..words.len() - 1].join(" "),
            permissions: perms.to_string(),
            owner: String::new(),
            group: String::new(),
        });
    }
    Some(Listing { path, entries })
}

/// The session's ssh arguments for scp: -l (a user to ssh, a bandwidth limit to scp) and -p (a port) as scp takes
/// them; the host is the last word
fn scp_args(base: &[String]) -> (Vec<String>, String) {
    let mut args = Vec::new();
    let mut words = base.iter();
    while let Some(word) = words.next() {
        match word.as_str() {
            "-l" => {
                if let Some(user) = words.next() {
                    args.extend(["-o".to_string(), format!("User={user}")]);
                }
            }
            "-p" => {
                if let Some(port) = words.next() {
                    args.extend(["-P".to_string(), port.clone()]);
                }
            }
            _ => args.push(word.clone()),
        }
    }
    let host = args.pop().unwrap_or_default();
    (args, host)
}

/// Copy files with scp (-O: the SCP protocol, as network devices speak it), one at a time. scp prints its progress
/// meter only to a terminal, so it runs in a pseudo terminal and the meter's percentage is read as it goes.
/// `sizes`: a download's file sizes, from the remote listing (an upload's are read here)
#[allow(clippy::too_many_arguments)]
async fn scp_transfer(
    base: &[String],
    token: &str,
    upload: bool,
    sources: &[String],
    sizes: &[u64],
    target: &str,
    cancel: &Arc<AtomicBool>,
    on_progress: &Channel<Progress>,
) -> Result<(), String> {
    let (args, host) = scp_args(base);
    let mut file_sizes = Vec::new();
    for (i, source) in sources.iter().enumerate() {
        if upload {
            let meta = std::fs::metadata(source).map_err(|e| format!("{source}: {e}"))?;
            if meta.is_dir() {
                return Err(format!("{source} is a folder: over SCP, copy the files in it instead."));
            }
            file_sizes.push(meta.len());
        } else {
            file_sizes.push(sizes.get(i).copied().unwrap_or(0));
        }
    }
    let total: u64 = file_sizes.iter().sum();
    let _ = on_progress.send(Progress::Total { bytes: total, files: sources.len() as u64 });
    // local paths as scp reads them: a \\?\ one would be taken for a host, and scp copy between two "remote" places
    let sources: Vec<String> =
        if upload { sources.iter().map(|s| plain_path(std::path::Path::new(s)).to_string_lossy().to_string()).collect() } else { sources.to_vec() };
    let target = if upload { target.to_string() } else { plain_path(std::path::Path::new(target)).to_string_lossy().to_string() };
    let target = target.as_str();
    let helper = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().to_string();
    let mut done = 0u64;
    for (source, size) in sources.iter().zip(file_sizes) {
        let name = source.trim_end_matches('/').rsplit(['/', std::path::MAIN_SEPARATOR]).next().unwrap_or(source).to_string();
        let (from, to) = if upload {
            (source.clone(), format!("{host}:{}", join(target, &name)))
        } else {
            (format!("{host}:{source}"), std::path::Path::new(target).join(&name).to_string_lossy().to_string())
        };
        let _ = on_progress.send(Progress::Done { bytes: done, file: name.clone() });
        let mut words = vec!["-O".to_string()];
        words.extend(args.iter().cloned());
        words.extend([from, to.clone()]);
        // how far the file has got, only ever forward: scp's meter (every percentage on the Mac; Windows' pseudo console
        // sends only the characters that change, so few whole percentages get through) and, for a download, the
        // file's size as scp writes it, once it's this run's file (not one already there from before)
        let best = Arc::new(AtomicU64::new(0));
        let watching = Arc::new(AtomicBool::new(true));
        if !upload && size > 0 {
            let (best, watching, progress, file, path, started, before) =
                (best.clone(), watching.clone(), on_progress.clone(), name.clone(), to.clone(), std::time::SystemTime::now(), done);
            tokio::spawn(async move {
                while watching.load(Ordering::Relaxed) {
                    if let Ok(meta) = tokio::fs::metadata(&path).await {
                        let fresh = meta.modified().map(|m| m >= started).unwrap_or(false);
                        let got = meta.len().min(size);
                        if fresh && got > best.fetch_max(got, Ordering::Relaxed) {
                            let _ = progress.send(Progress::Done { bytes: before + got, file: file.clone() });
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(250)).await;
                }
            });
        }
        let (token, helper, cancel, progress, file, before, meter_best) = (token.to_string(), helper.clone(), cancel.clone(), on_progress.clone(), name.clone(), done, best.clone());
        let result = tokio::task::spawn_blocking(move || {
            run_scp(&words, &token, &helper, &cancel, |percent| {
                let got = size * percent / 100;
                if size > 0 && got > meter_best.fetch_max(got, Ordering::Relaxed) {
                    let _ = progress.send(Progress::Done { bytes: before + got, file: file.clone() });
                }
            })
        })
        .await;
        watching.store(false, Ordering::Relaxed);
        let result = result.map_err(|e| e.to_string())?;
        if let Err(said) = result {
            return Err(if said == "Cancelled" { said } else { format!("{name}: {}", privilege_hint(&said)) });
        }
        // scp can end well having copied nothing (a path it read as another host, a device that ignores a request),
        // so a download counts only when the file is here, at the size the listing gave
        if !upload {
            match std::fs::metadata(&to) {
                Ok(meta) if size == 0 || meta.len() == size => {}
                Ok(meta) => return Err(format!("{name}: scp ended, but {to} has {} of {} bytes", meta.len(), size)),
                Err(_) => return Err(format!("{name}: scp ended, but {to} wasn't written")),
            }
        }
        done += if upload || size > 0 { size } else { std::fs::metadata(&to).map(|m| m.len()).unwrap_or(0) };
        let _ = on_progress.send(Progress::Done { bytes: done, file: name });
    }
    Ok(())
}

/// One scp in a pseudo terminal: its meter's percentages to `report`; on failure, what it said
fn run_scp(words: &[String], token: &str, helper: &str, cancel: &AtomicBool, report: impl Fn(u64)) -> Result<(), String> {
    use portable_pty::{native_pty_system, CommandBuilder, PtySize};
    use std::io::Read;
    let pair = native_pty_system()
        .openpty(PtySize { rows: 24, cols: 200, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;
    let mut command = CommandBuilder::new(crate::launch::openssh("scp"));
    command.args(words);
    command.env("SSH_ASKPASS", helper);
    command.env("SSH_ASKPASS_REQUIRE", "force");
    command.env(crate::askpass::TOKEN_VAR, token);
    let mut child = pair.slave.spawn_command(command).map_err(|e| format!("Couldn't start scp: {e}"))?;
    drop(pair.slave);
    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let mut writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    // the meter, read on its own thread: "name   45% 8192KB   1.2MB/s   00:10 ETA", redrawn after each \r
    let (tx, rx) = std::sync::mpsc::channel::<Vec<u8>>();
    std::thread::spawn(move || {
        let mut buffer = [0u8; 4096];
        while let Ok(n) = reader.read(&mut buffer) {
            // Windows' pseudo console asks where the cursor is (ESC [6n) as it starts, and waits for the answer
            // before the program runs; a terminal tab's page answers, here nothing would, and scp never started
            if buffer[..n].windows(4).any(|w| w == b"\x1b[6n") {
                use std::io::Write;
                let _ = writer.write_all(b"\x1b[1;1R").and_then(|_| writer.flush());
            }
            if n == 0 || tx.send(buffer[..n].to_vec()).is_err() {
                break;
            }
        }
    });
    let mut said = String::new();
    let mut last = None;
    let status = loop {
        if cancel.load(Ordering::Relaxed) {
            let _ = child.kill();
            return Err("Cancelled".into());
        }
        while let Ok(bytes) = rx.try_recv() {
            said.push_str(&String::from_utf8_lossy(&bytes));
        }
        if let Some(percent) = meter_percent(&said) {
            if last != Some(percent) {
                last = Some(percent);
                report(percent);
            }
        }
        // keep only the end: the meter redraws without end
        if said.len() > 8192 {
            // at a character's start: a file name's accents are several bytes
            let cut = (said.len() - 4096..said.len()).find(|&i| said.is_char_boundary(i)).unwrap_or(said.len());
            said = said[cut..].to_string();
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(e) => return Err(e.to_string()),
        }
    };
    std::thread::sleep(Duration::from_millis(100));
    while let Ok(bytes) = rx.try_recv() {
        said.push_str(&String::from_utf8_lossy(&bytes));
    }
    if status.success() {
        return Ok(());
    }
    // what it said, without its meter
    let text: Vec<&str> = said.split(['\r', '\n']).map(str::trim).filter(|l| !l.is_empty() && meter_percent(l).is_none()).collect();
    Err(if text.is_empty() { "scp failed".into() } else { text.join(" ") })
}

/// The last percentage scp's meter shows in some output
fn meter_percent(text: &str) -> Option<u64> {
    let last = text.split(['\r', '\n']).rev().find(|l| l.contains('%'))?;
    last.split_whitespace().find_map(|w| w.strip_suffix('%').and_then(|n| n.parse::<u64>().ok())).filter(|p| *p <= 100)
}

// ---- this Mac's side ----

/// A user or group id's name, as ls shows it
#[cfg(unix)]
fn id_name(id: u32, group: bool) -> String {
    /// The names found so far, so each id is asked about once
    static NAMES: Mutex<Option<HashMap<(u32, bool), String>>> = Mutex::new(None);
    let mut names = NAMES.lock().unwrap();
    let names = names.get_or_insert_with(HashMap::new);
    names
        .entry((id, group))
        .or_insert_with(|| {
            // SAFETY: the C library's lookups, read under the lock above
            unsafe {
                let name = if group {
                    let g = libc::getgrgid(id);
                    if g.is_null() { std::ptr::null() } else { (*g).gr_name }
                } else {
                    let p = libc::getpwuid(id);
                    if p.is_null() { std::ptr::null() } else { (*p).pw_name }
                };
                if name.is_null() { id.to_string() } else { std::ffi::CStr::from_ptr(name).to_string_lossy().to_string() }
            }
        })
        .clone()
}

/// A local path as people write it: Windows' canonical paths start `\\?\` (`\\?\C:\Users`, `\\?\UNC\server\share`),
/// which scp reads as a host (`\\?\C`) and then copies between two remote places; the prefix is dropped. Unchanged
/// elsewhere.
pub fn plain_path(path: &std::path::Path) -> std::path::PathBuf {
    let text = path.to_string_lossy();
    if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
        return std::path::PathBuf::from(format!(r"\\{rest}"));
    }
    match text.strip_prefix(r"\\?\") {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => std::path::PathBuf::from(rest),
        _ => path.to_path_buf(),
    }
}

// ---------------------------------------------------------------------------------------------------------------------
// Demo mode (demo.rs): a session's files are a made-up switch's flash, over SCP as a real Cisco's are. The listing is
// the stand-in device's own `dir` (demo_device.rs), so the tab's terminal and its file transfer agree; copies take
// a while and show their progress, then leave an empty file of the right size, so nothing real is read or sent.

/// The demo's open file transfers: each tab's id, and the session's name and host
static DEMO: Mutex<Option<HashMap<u32, (String, String)>>> = Mutex::new(None);

/// What a demo device says to a change it doesn't make
const DEMO_UNCHANGED: &str = "Demo mode: nothing on the device is changed.";

/// How fast a demo copy goes, in bytes a second: an IOS image takes about a minute, long enough to be seen
const DEMO_SPEED: u64 = 20_000_000;

/// A demo tab's session name and host, when the tab is a demo's
fn demo_device(id: u32) -> Option<(String, String)> {
    DEMO.lock().unwrap().as_ref().and_then(|d| d.get(&id).cloned())
}

/// Open a demo session's files: SCP, at flash:, beside the demo's own Downloads folder
fn demo_open(session: &crate::sessions::StoredSession) -> Result<Opened, String> {
    let name = session.get("SessionName").to_string();
    let host = session.get("Host").to_string();
    let user = session.get("Username");
    let address = if user.is_empty() { host.clone() } else { format!("{user}@{host}") };
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    DEMO.lock().unwrap().get_or_insert_with(HashMap::new).insert(id, (name, host));
    let local = crate::demo::downloads().to_string_lossy().to_string();
    Ok(Opened { id, home: "flash:/".into(), remote: "flash:/".into(), local, address, scp: true })
}

/// A demo device's folder, from its `dir`: flash:/ has files, any other folder is empty
fn demo_list(device: &(String, String), path: &str) -> Listing {
    let path = if path.is_empty() { "flash:/" } else { path };
    let empty = || Listing { path: path.to_string(), entries: Vec::new() };
    if !path.trim_end_matches('/').eq_ignore_ascii_case("flash:") {
        return empty();
    }
    crate::demo_device::canned_answer("ios", &device.0, &device.1, "dir flash:/").and_then(|text| parse_dir(&text)).unwrap_or_else(empty)
}

/// A demo copy: its progress at [`DEMO_SPEED`], then, for a download, an empty file of the right size in `target`
async fn demo_transfer(
    upload: bool,
    sources: &[String],
    sizes: &[u64],
    target: &str,
    cancel: &AtomicBool,
    on_progress: &Channel<Progress>,
) -> Result<(), String> {
    let size_of = |i: usize, source: &str| -> u64 {
        if upload {
            std::fs::metadata(source).map(|m| m.len()).unwrap_or(0)
        } else {
            sizes.get(i).copied().unwrap_or(0)
        }
    };
    let total: u64 = sources.iter().enumerate().map(|(i, s)| size_of(i, s)).sum();
    let _ = on_progress.send(Progress::Total { bytes: total, files: sources.len() as u64 });
    let mut done = 0;
    for (i, source) in sources.iter().enumerate() {
        let name = source.rsplit(['/', ':', '\\']).next().unwrap_or(source).to_string();
        let size = size_of(i, source);
        let mut sent = 0;
        while sent < size {
            if cancel.load(Ordering::Relaxed) {
                return Err("Cancelled".into());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
            sent = (sent + DEMO_SPEED / 4).min(size);
            let _ = on_progress.send(Progress::Done { bytes: done + sent, file: name.clone() });
        }
        done += size;
        if !upload {
            let file = std::fs::File::create(std::path::Path::new(target).join(&name)).map_err(|e| e.to_string())?;
            file.set_len(size).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// A folder on this computer (`~` is home, and `~/…`, or on Windows `~\…` and `%USERPROFILE%\…`, are in it, as
/// launch.rs's expand_home reads them), its path made absolute
#[tauri::command]
pub fn local_list(path: String) -> Result<Listing, String> {
    let path = path.trim();
    let path = if path.is_empty() || path == "~" || path.eq_ignore_ascii_case("%USERPROFILE%") {
        dirs::home_dir().unwrap_or_default()
    } else {
        std::path::PathBuf::from(crate::launch::expand_home(path))
    };
    let path = plain_path(&std::fs::canonicalize(&path).map_err(|e| format!("{}: {e}", path.display()))?);
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(&path).map_err(|e| format!("{}: {e}", path.display()))?.flatten() {
        let Ok(lmeta) = entry.metadata() else { continue };
        let link = lmeta.file_type().is_symlink();
        let meta = if link { std::fs::metadata(entry.path()).unwrap_or(lmeta.clone()) } else { lmeta.clone() };
        let modified = meta.modified().ok().and_then(|t| t.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
        #[cfg(unix)]
        let (permissions, owner, group) = {
            use std::os::unix::fs::MetadataExt;
            // demo mode: never the user's own name, in a screenshot
            if crate::demo::on() {
                (mode_text(lmeta.mode()), "demo".to_string(), "staff".to_string())
            } else {
                (mode_text(lmeta.mode()), id_name(lmeta.uid(), false), id_name(lmeta.gid(), true))
            }
        };
        #[cfg(not(unix))]
        let (permissions, owner, group) = (String::new(), String::new(), String::new());
        entries.push(Entry {
            name: entry.file_name().to_string_lossy().to_string(),
            dir: meta.is_dir(),
            link,
            size: if meta.is_dir() { 0 } else { meta.len() },
            modified,
            date: String::new(),
            permissions,
            owner,
            group,
        });
    }
    Ok(Listing { path: path.to_string_lossy().to_string(), entries })
}

/// A new folder on this Mac
#[tauri::command]
pub fn local_mkdir(path: String) -> Result<(), String> {
    std::fs::create_dir(&path).map_err(|e| e.to_string())
}

/// Rename a file or folder on this Mac
#[tauri::command]
pub fn local_rename(from: String, to: String) -> Result<(), String> {
    if std::path::Path::new(&to).exists() {
        return Err(format!("{to} already exists."));
    }
    std::fs::rename(&from, &to).map_err(|e| e.to_string())
}

/// Show a file or folder in Finder, or Explorer on Windows
#[tauri::command]
pub fn local_reveal(path: String) -> Result<(), String> {
    crate::reveal(std::path::Path::new(&path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_transfer_leaves_the_tabs_tunnels_agent_and_x11_to_the_tab() {
        let tab: Vec<String> = ["-p", "2222", "-A", "-X", "-C", "-L", "8080:localhost:80", "10.0.0.5"].map(String::from).to_vec();
        let args = without_forwarding(tab);
        assert_eq!(args, ["-o", "ClearAllForwardings=yes", "-p", "2222", "-C", "-L", "8080:localhost:80", "10.0.0.5"]);
    }

    #[test]
    fn reads_a_cisco_dir() {
        let text = "\nDirectory of flash:/\n\n    2  -rwx        3096   Mar 1 1993 00:15:59 +00:00  multiple-fs\n    4  drwx         512   Mar 1 1993 00:04:09 +00:00  c2960-lanbasek9-mz.150-2.SE11\n  611  -rwx        5684   Mar 1 1993 02:20:14 +00:00  config.text\n\n57931776 bytes total (39998976 bytes free)\n";
        let listing = parse_dir(text).unwrap();
        assert_eq!(listing.path, "flash:/");
        assert_eq!(listing.entries.len(), 3);
        assert_eq!(listing.entries[1].name, "c2960-lanbasek9-mz.150-2.SE11");
        assert!(listing.entries[1].dir);
        assert_eq!((listing.entries[2].size, listing.entries[2].date.as_str()), (5684, "Mar 1 1993 02:20:14 +00:00"));
        assert!(parse_dir("bash: dir: command not found").is_none());
        let (args, host) = scp_args(&["-p".into(), "2222".into(), "-l".into(), "admin".into(), "-o".into(), "X=1".into(), "10.0.0.1".into()]);
        assert_eq!(args, ["-P", "2222", "-o", "User=admin", "-o", "X=1"]);
        assert_eq!(host, "10.0.0.1");
    }

    #[test]
    fn explains_privilege_refusals() {
        assert!(privilege_hint("Line has invalid autocommand \"delete /force flash:x\"").contains("privilege 15"));
        assert_eq!(privilege_hint("%Error deleting flash:/x (No such file or directory)"), "%Error deleting flash:/x (No such file or directory)");
    }

    #[test]
    fn reads_scps_meter() {
        assert_eq!(meter_percent("c3560.bin                45%   8192KB   1.2MB/s   00:10 ETA"), Some(45));
        assert_eq!(meter_percent("a 10% x\rc3560.bin   100%   18MB   1.1MB/s   00:16    \r\n"), Some(100));
        assert_eq!(meter_percent("Password:"), None);
    }

    #[test]
    fn keeps_connections_alive() {
        let added = with_keepalive(vec!["10.0.0.1".into()]);
        assert!(added.contains(&"ServerAliveInterval=30".to_string()) && added.ends_with(&["10.0.0.1".to_string()]));
        let profile = with_keepalive(vec!["-o".into(), "ServerAliveInterval=15".into(), "h".into()]);
        assert_eq!(profile.iter().filter(|w| w.starts_with("ServerAliveInterval")).count(), 1);
    }

    #[test]
    fn writes_modes_as_ls_does() {
        assert_eq!(mode_text(0o40755), "drwxr-xr-x");
        assert_eq!(mode_text(0o100644), "-rw-r--r--");
        assert_eq!(mode_text(0o120777), "lrwxrwxrwx");
        assert_eq!(join("/", "etc"), "/etc");
        assert_eq!(join("/home/me", "a b"), "/home/me/a b");
    }

    /// Demo mode's file transfer lists a switch's flash from the stand-in device's own `dir`, as a real Cisco's is
    /// read over SCP: the files with their sizes, and an empty folder anywhere else
    #[test]
    fn a_demo_switch_lists_its_flash_as_its_dir_says() {
        let device = ("core-sw-01".to_string(), "core-sw-01.example.net".to_string());
        let flash = demo_list(&device, "flash:");
        assert_eq!(flash.path, "flash:/");
        let image = flash.entries.iter().find(|e| e.name == "cat9k_iosxe.17.12.03.SPA.bin").expect("the image is listed");
        assert_eq!((image.size, image.dir), (1_301_418_244, false));
        assert!(flash.entries.iter().any(|e| e.name == "tracelogs" && e.dir));
        assert!(demo_list(&device, "flash:/tracelogs").entries.is_empty());
    }
}
