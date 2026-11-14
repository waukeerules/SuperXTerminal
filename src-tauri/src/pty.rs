//! Pseudo terminals for terminal tabs.
//!
//! Each tab's page (SuperXPuTTY's `web/terminal`, through `src/lib/terminal/PageHost.ts`) asks for a pseudo terminal
//! with `pty_open`, types into it with `pty_write` and resizes it with `pty_resize`. Output comes back on the tab's
//! own `Channel` as base64, the form the page's "data" message already takes. What runs in it is a [`Target`]: the
//! user's login shell, or a stored session started as `launch.rs` builds it.
//!
//! A tab can move to another window (Move to New Window): its output goes through an [`Outlet`], which keeps the
//! recent output, and `pty_attach` gives the session to the new window's page, replaying that output first.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use portable_pty::{native_pty_system, Child, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

use crate::charset::Charset;
use crate::launch::{self, Launch, Look};
use crate::sessions::Store;
use crate::AppState;

/// What a tab runs
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    /// The user's login shell
    LocalShell,
    /// A stored session, by its store and id
    Session { store: Store, id: String },
    /// Quick connect's one-off session
    QuickConnect(launch::QuickConnect),
    /// A profile's key fetch (launch::KEY_FETCH_NEEDED), run in the session's tab before it connects
    KeyFetch { profile: String },
}

/// What a pseudo terminal sends its tab
// Started is far larger than the rest (it carries the profile's whole look), but a session sends it once and the
// others thousands of times, each made and sent at once and never kept in number: a Box would buy nothing
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PtyEvent {
    /// Output, base64-encoded; UTF-8 may be split across chunks, which xterm.js handles
    Data { data: String },
    /// The program ended; `code` is its exit code when it has one
    Exit { code: Option<u32> },
    /// Something to tell the user in the terminal before the program's own output
    Notice { text: String, kind: String },
    /// How the page should treat the tab: `local` for a local shell, the profile's cursor and bell, and what the
    /// connection is ("pty", or a stream's "telnet", "raw" or "serial"), for the Special Commands it can send
    Started { local: bool, look: Look, link: String },
}

/// How much recent output a session keeps, to replay when its tab moves to another window
const KEPT_OUTPUT: usize = 2 * 1024 * 1024;

/// Where a session's output goes: its tab's channel, which changes when the tab moves to another window, and what
/// the new tab needs to carry on: how the session started, the recent output, and how it ended, if it has
#[derive(Clone)]
struct Outlet(Arc<Mutex<OutletState>>);

/// What an [`Outlet`] keeps: the tab's channel, the "started" event, the recent output and the end, if it's come, and
/// the device's character set its output is turned to UTF-8 from
struct OutletState {
    channel: Channel<PtyEvent>,
    started: Option<PtyEvent>,
    recent: VecDeque<u8>,
    exit: Option<Option<u32>>,
    charset: Charset,
    /// The tab's raw output, as it came, and its resizes, written to a file when SUPERXTERMINAL_CAPTURE names a
    /// folder ([`capture_file`]); None otherwise, which is always but while chasing a display problem
    capture: Option<(std::fs::File, std::time::Instant)>,
}

/// A file for a tab's raw output, in the folder SUPERXTERMINAL_CAPTURE names, for diagnosing what a terminal draws
/// (ConPTY's redraws, a device's escape codes): None when it isn't set, or the file can't be made. Only for a
/// developer's own sessions: what's typed back (a password echoed by a device) can be in it
fn capture_file() -> Option<(std::fs::File, std::time::Instant)> {
    let folder = std::env::var_os("SUPERXTERMINAL_CAPTURE")?;
    static COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let n = COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let path = std::path::Path::new(&folder).join(format!("tab-{stamp}-{n}.txt"));
    std::fs::create_dir_all(&folder).ok()?;
    std::fs::File::create(path).ok().map(|f| (f, std::time::Instant::now()))
}

impl OutletState {
    /// A line in the tab's capture, with the milliseconds since the tab opened
    fn captured(&mut self, line: &str) {
        if let Some((file, opened)) = self.capture.as_mut() {
            let _ = writeln!(file, "[{}ms] {line}", opened.elapsed().as_millis());
        }
    }
}

impl Outlet {
    /// An outlet sending to a tab's channel, with nothing passed on yet; the output is in `charset`
    fn new(channel: Channel<PtyEvent>, charset: Charset) -> Outlet {
        Outlet(Arc::new(Mutex::new(OutletState { channel, started: None, recent: VecDeque::new(), exit: None, charset, capture: capture_file() })))
    }

    /// Pass an event on to the tab (a tab that's gone, or moving, misses it; the output is kept all the same)
    fn send(&self, event: PtyEvent) {
        let mut state = self.0.lock().unwrap();
        match &event {
            PtyEvent::Started { .. } => state.started = Some(event.clone()),
            PtyEvent::Exit { code } => state.exit = Some(*code),
            _ => {}
        }
        let _ = state.channel.send(event);
    }

    /// Pass output on as UTF-8, keeping the last [`KEPT_OUTPUT`] bytes of it (already turned, for a moved tab's replay)
    fn data(&self, bytes: &[u8]) {
        let mut state = self.0.lock().unwrap();
        state.captured(&format!("DATA {:?}", String::from_utf8_lossy(bytes)));
        let turned = state.charset.decode(bytes);
        let bytes = turned.as_slice();
        state.recent.extend(bytes);
        let over = state.recent.len().saturating_sub(KEPT_OUTPUT);
        state.recent.drain(..over);
        let data = base64::engine::general_purpose::STANDARD.encode(bytes);
        let _ = state.channel.send(PtyEvent::Data { data });
    }

    /// Note a resize in the tab's capture, if it's being captured
    fn resized(&self, cols: u16, rows: u16) {
        self.0.lock().unwrap().captured(&format!("RESIZE {cols}x{rows}"));
    }

    /// Send the output to another tab from now on, after how it started, what it has shown and how it ended
    fn attach(&self, channel: Channel<PtyEvent>) {
        let mut state = self.0.lock().unwrap();
        state.channel = channel;
        if let Some(started) = state.started.clone() {
            let _ = state.channel.send(started);
        }
        let recent: Vec<u8> = state.recent.iter().copied().collect();
        let recent = without_queries(&recent);
        for chunk in recent.chunks(64 * 1024) {
            let _ = state.channel.send(PtyEvent::Data { data: base64::engine::general_purpose::STANDARD.encode(chunk) });
        }
        if let Some(code) = state.exit {
            let _ = state.channel.send(PtyEvent::Exit { code });
        }
    }
}

/// Output to replay in a moved tab, without the questions a program asked the terminal: where the cursor is (ESC[6n,
/// which ConPTY asks as every session starts on Windows), its status (ESC[5n), what it is (ESC[c, ESC[>c, ESC[>q), a
/// mode's setting (ESC[?...$p), the window's size (ESC[14t...), a colour or the clipboard (OSC 4, 10-19, 52 ending "?") or a setting
/// (DCS $q). They were answered when first asked; the new tab's terminal would answer them again, into the session,
/// where a Cisco switch showed ";1R" at its prompt. Everything drawn stays, and a sequence cut short at either end is
/// left as it is.
fn without_queries(bytes: &[u8]) -> Vec<u8> {
    const ESC: u8 = 0x1b;
    let mut kept = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let end = match (bytes[at], bytes.get(at + 1)) {
            (ESC, Some(b'[')) => csi_query(bytes, at + 2),
            // a colour (4, 10-19) or the clipboard (52) asked for; not a title that ends in a question mark
            (ESC, Some(b']')) => string_query(bytes, at + 2, |body| {
                let number = body.split(|&b| b == b';').next().and_then(|n| std::str::from_utf8(n).ok()?.parse::<u32>().ok());
                matches!(number, Some(4 | 10..=19 | 52)) && body.ends_with(b"?")
            }),
            (ESC, Some(b'P')) => string_query(bytes, at + 2, |body| body.starts_with(b"$q")),
            _ => None,
        };
        match end {
            Some(end) => at = end,
            None => {
                kept.push(bytes[at]);
                at += 1;
            }
        }
    }
    kept
}

/// The end of a control sequence starting at `from` (after ESC[) when it asks the terminal something; None when it
/// doesn't or is cut short
fn csi_query(bytes: &[u8], from: usize) -> Option<usize> {
    // parameters (0-9 ; < = > ?), intermediates (space to /), then the final byte
    let mut at = from;
    while at < bytes.len() && (0x30..=0x3f).contains(&bytes[at]) {
        at += 1;
    }
    let params = &bytes[from..at];
    let mid = at;
    while at < bytes.len() && (0x20..=0x2f).contains(&bytes[at]) {
        at += 1;
    }
    let intermediates = &bytes[mid..at];
    let last = *bytes.get(at)?;
    let first: Option<u32> = std::str::from_utf8(params).ok()?.split(';').next()?.parse().ok();
    let asks = match last {
        // device status: ESC[5n, ESC[6n, ESC[?6n
        b'n' => true,
        // device attributes: ESC[c, ESC[0c, ESC[>c, ESC[=c
        b'c' => intermediates.is_empty(),
        // a mode's setting: ESC[...$p
        b'p' => intermediates == b"$",
        // the terminal's name and version: ESC[>q
        b'q' => params.starts_with(b">"),
        // the window's state, place and size, and the title (11, 13-21); not 22 and 23, which keep and restore the title
        b't' => matches!(first, Some(11 | 13..=21)),
        _ => false,
    };
    asks.then_some(at + 1)
}

/// The end of an OSC or DCS string starting at `from` (after ESC] or ESCP), ended by BEL or ESC\, when `asks` its
/// body; None when it doesn't or is cut short
fn string_query(bytes: &[u8], from: usize, asks: impl Fn(&[u8]) -> bool) -> Option<usize> {
    let mut at = from;
    while at < bytes.len() {
        match bytes[at] {
            0x07 => return asks(&bytes[from..at]).then_some(at + 1),
            0x1b if bytes.get(at + 1) == Some(&b'\\') => return asks(&bytes[from..at]).then_some(at + 2),
            _ => at += 1,
        }
    }
    None
}

/// One open pseudo terminal: the master side (for resizing), its writer (for typing) and the program in it
struct Session {
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// The device's character set, what's typed is turned into
    charset: Charset,
    kind: SessionKind,
    /// The window whose tab it is; closing that window ends it
    window: String,
    outlet: Outlet,
    /// How the server load readings reach its server (server_load.rs); None when there are none
    server_load: Option<crate::server_load::LoadVia>,
}

/// A program in a pseudo terminal (ssh, a shell), or a Telnet, Raw or Serial stream in the core
enum SessionKind {
    Pty {
        master: Box<dyn MasterPty + Send>,
        /// Shared with the reader thread, which waits for its exit code once the output ends
        child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    },
    Stream(crate::streams::Stream),
}

/// Every open pseudo terminal, by the id given to its tab
#[derive(Default)]
pub struct Ptys {
    next: AtomicU32,
    sessions: Mutex<HashMap<u32, Session>>,
}

impl Ptys {
    /// How the server load readings reach the server of the tab with pseudo terminal `id`; None when there are none
    /// (its profile hasn't Show server load on, it isn't SSH) or it has closed
    pub fn server_load(&self, id: u32) -> Option<crate::server_load::LoadVia> {
        self.sessions.lock().unwrap().get(&id).and_then(|s| s.server_load.clone())
    }
}

/// Start a tab's program (`target`) in a new pseudo terminal of the given size, sending its output on `on_event`
///
/// Returns the id the tab uses for the other calls; Err when the session can't start on this Mac. `login_as` is the
/// username typed at the tab's "login as:", when the launch asked for one (launch::USERNAME_NEEDED).
// each of the page's arguments is a parameter of its own, as Tauri passes them. async: off the window's own thread,
// as a 1Password reference in its password or values waits for the user to approve it (onepassword.rs)
#[allow(clippy::too_many_arguments)]
#[tauri::command(async)]
pub fn pty_open(
    window: tauri::WebviewWindow,
    ptys: State<'_, Ptys>,
    state: State<'_, AppState>,
    target: Target,
    cols: u16,
    rows: u16,
    on_event: Channel<PtyEvent>,
    login_as: Option<String>,
) -> Result<u32, String> {
    let profiles = state.profiles.lock().unwrap().clone();
    // the username typed at the tab's "login as:" (launch::USERNAME_NEEDED), for this connection and its reconnects
    let login_as = login_as.map(|u| u.trim().to_string()).filter(|u| !u.is_empty());
    let with_login = |mut session: crate::sessions::StoredSession| {
        if let Some(user) = &login_as {
            session.set("Username", user);
        }
        session
    };
    let values = crate::variables::values(&state.user_folder);
    // what a 1Password reference read for another use left unsaid isn't this tab's to say
    crate::onepassword::take_problems();
    let (serial_devices, overrides_on) = {
        let settings = state.settings.lock().unwrap();
        (settings.serial_devices.clone(), settings.use_local_overrides)
    };
    // what's opening, for the app's log (never a password: Quick connect's host and user only)
    let what = match &target {
        Target::LocalShell => "a Local Shell".to_string(),
        Target::Session { id, .. } => format!("session {id}"),
        Target::QuickConnect(quick) => format!("Quick connect {} {}", quick.protocol, quick.host),
        Target::KeyFetch { profile } => format!("the key fetch of profile {profile}"),
    };
    let launch = match target {
        Target::LocalShell => Ok(launch::local_shell(&profiles)),
        Target::Session { store, id } => {
            let sessions = state.sessions.lock().unwrap();
            match sessions.find(store, &id) {
                None => Err(format!("The session {id} is gone.")),
                // the user's local overrides beat the session's values and its folders'
                Some(session) => {
                    let session = with_login(crate::overrides::applied(&state.user_folder, session, overrides_on));
                    launch::launch(&sessions, &profiles, &values, &serial_devices, store, &session)
                }
            }
        }
        Target::QuickConnect(quick) => {
            let sessions = state.sessions.lock().unwrap();
            quick.session().and_then(|s| launch::launch(&sessions, &profiles, &values, &serial_devices, Store::Saved, &with_login(s)))
        }
        Target::KeyFetch { profile } => launch::key_fetch(&profiles, &values, &profile),
    };
    let launch = launch.inspect_err(|e| crate::applog::warn(format!("Couldn't open {what}: {e}")));
    let Launch { mut command, password, local, mut notices, look, stream, server_load, .. } = launch?;
    // a 1Password reference that couldn't be read (op missing, not approved...): the tab says why, as it opens
    let unread = crate::onepassword::take_problems();
    crate::applog::info(format!("Opening {what} in {}", window.label()));
    // an SSH session: its command goes in the app's log, and with Verbose SSH (Settings > General) ssh tells the tab
    // how connecting and logging in went; -v right after the program, as the host and any remote command come last
    let ssh = stream.is_none()
        && !local
        && command.get_argv().first().and_then(|p| std::path::Path::new(p).file_stem()).is_some_and(|s| s.eq_ignore_ascii_case("ssh"));
    if ssh {
        if state.settings.lock().unwrap().ssh_verbose {
            command.get_argv_mut().insert(1, "-v".into());
            notices.push("Verbose SSH is on (Settings > General > Diagnostics): ssh's own messages follow.".into());
        }
        crate::applog::info(format!("ssh command: {}", shown_command(command.get_argv())));
    }
    let link = match &stream {
        Some(crate::streams::Target::Telnet { .. }) => "telnet",
        Some(crate::streams::Target::Raw { .. }) => "raw",
        Some(crate::streams::Target::Serial { .. }) => "serial",
        None => "pty",
    };
    // the profile's Remote character set: not for a local shell (this computer's own, UTF-8), nor for SSH on Windows,
    // whose pseudo console has already turned the device's bytes into UTF-8 (charset.rs)
    let chosen = Charset::named(&look.charset);
    let charset = if local || (cfg!(windows) && stream.is_none()) { Charset::Utf8 } else { chosen };
    if chosen != charset && !local {
        notices.push(format!("The profile's character set ({}) isn't used for SSH on Windows: Windows' console turns the device's characters into UTF-8 first.", chosen.label()));
    }
    let outlet = Outlet::new(on_event, charset);
    outlet.0.lock().unwrap().captured(&format!("OPEN {cols}x{rows}"));
    outlet.send(PtyEvent::Started { local, look, link: link.into() });
    for text in unread {
        outlet.send(PtyEvent::Notice { text, kind: "warn".into() });
    }
    for text in notices {
        outlet.send(PtyEvent::Notice { text, kind: "info".into() });
    }
    if let Some(target) = stream {
        return open_stream(&ptys, &target, cols, rows, outlet, charset, window.label());
    }

    let pair = native_pty_system()
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;

    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "SuperXTerminal");
    // an app started from Finder has no LANG, and shells then treat UTF-8 input as bytes
    if std::env::var_os("LANG").is_none() {
        command.env("LANG", "en_US.UTF-8");
    }
    if command.get_cwd().is_none() && let Some(home) = dirs::home_dir() {
            command.cwd(home);
        }
    // the system's environment, not an AppImage's: its libraries broke the shell's programs (appimage.rs)
    crate::appimage::tidy(&mut command);
    let child = Arc::new(Mutex::new(pair.slave.spawn_command(command).map_err(|e| e.to_string())?));
    // the slave side belongs to the child now; keeping it open here would stop the reader seeing the end
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = Arc::new(Mutex::new(pair.master.take_writer().map_err(|e| e.to_string())?));
    let id = ptys.next.fetch_add(1, Ordering::Relaxed) + 1;

    let typist = writer.clone();
    let ending = child.clone();
    let output = outlet.clone();
    // the tab hears of the end once: from the reader when the output ends, or from the watcher when the program does
    let finished = Arc::new(AtomicBool::new(false));
    let told = finished.clone();
    let watched = outlet.clone();
    watch_exit(child.clone(), finished.clone(), move |code| watched.send(PtyEvent::Exit { code }));
    std::thread::Builder::new()
        .name(format!("pty-{id}"))
        .spawn(move || {
            let mut prompt = password.map(PasswordPrompt::new);
            let mut buffer = [0u8; 64 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        // on even while no tab takes it (one moving to another window); closing the tab ends the program
                        output.data(&buffer[..n]);
                        if let Some(answer) = prompt.as_mut().and_then(|p| p.answer(&buffer[..n])) {
                            let mut writer = typist.lock().unwrap();
                            let _ = writer.write_all(answer.as_bytes()).and_then(|_| writer.flush());
                            prompt = None;
                        }
                    }
                }
            }
            // the exit code says whether the session ended cleanly (a logout closes its tab, as SuperXPuTTY's does);
            // the output can end a moment before the program does, so wait a little for it
            let mut code = None;
            for _ in 0..40 {
                match ending.lock().unwrap().try_wait() {
                    Ok(Some(status)) => {
                        code = Some(status.exit_code());
                        break;
                    }
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(50)),
                    Err(_) => break,
                }
            }
            if !told.swap(true, Ordering::SeqCst) {
                output.send(PtyEvent::Exit { code });
            }
        })
        .map_err(|e| e.to_string())?;

    ptys.sessions.lock().unwrap().insert(id, Session { writer, charset, kind: SessionKind::Pty { master: pair.master, child }, window: window.label().to_string(), outlet, server_load });
    Ok(id)
}

/// How often the watcher looks whether the program has ended, and how long it then leaves the reader to pass on its
/// last output and see the end itself
const EXIT_POLL: std::time::Duration = std::time::Duration::from_millis(250);
/// How long the reader is left after the end is seen (see [`EXIT_POLL`])
const EXIT_GRACE: std::time::Duration = std::time::Duration::from_millis(500);

/// Tell of the program's end even when its output doesn't end with it: on Windows a ConPTY's output stays open after
/// ssh exits (until the pseudo console is closed), and anywhere a job it left running in the background holds it
/// open. The tab then never heard the session had ended (a pulled cable's "Connection reset" left a dead tab).
fn watch_exit(child: Arc<Mutex<Box<dyn Child + Send + Sync>>>, finished: Arc<AtomicBool>, on_exit: impl FnOnce(Option<u32>) + Send + 'static) {
    let _ = std::thread::Builder::new().name("pty-exit".into()).spawn(move || loop {
        std::thread::sleep(EXIT_POLL);
        if finished.load(Ordering::SeqCst) {
            return;
        }
        let status = child.lock().unwrap().try_wait();
        match status {
            Ok(Some(status)) => {
                std::thread::sleep(EXIT_GRACE);
                if !finished.swap(true, Ordering::SeqCst) {
                    on_exit(Some(status.exit_code()));
                }
                return;
            }
            Ok(None) => {}
            Err(_) => return,
        }
    });
}

/// Start a Telnet, Raw or Serial stream and pass its output on, as a pseudo terminal's. The other end closing is a
/// clean end (the tab closes, as plink's would); a broken connection isn't (the tab stays and can reconnect).
fn open_stream(ptys: &Ptys, target: &crate::streams::Target, cols: u16, rows: u16, outlet: Outlet, charset: Charset, window: &str) -> Result<u32, String> {
    let (stream, mut reader) = crate::streams::open(target, cols, rows)?;
    let id = ptys.next.fetch_add(1, Ordering::Relaxed) + 1;
    let output = outlet.clone();
    std::thread::Builder::new()
        .name(format!("stream-{id}"))
        .spawn(move || {
            let mut buffer = [0u8; 64 * 1024];
            let code = loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break 0,
                    Ok(n) => output.data(&buffer[..n]),
                    Err(e) => {
                        output.send(PtyEvent::Notice { text: format!("Connection lost: {e}"), kind: "error".into() });
                        break 1;
                    }
                }
            };
            output.send(PtyEvent::Exit { code: Some(code) });
        })
        .map_err(|e| e.to_string())?;
    let writer = stream.writer.clone();
    ptys.sessions.lock().unwrap().insert(id, Session { writer, charset, kind: SessionKind::Stream(stream), window: window.to_string(), outlet, server_load: None });
    Ok(id)
}

/// A Special Command for a tab (Telnet's Break, Are You There...; a serial Break)
#[tauri::command]
pub fn pty_special(ptys: State<'_, Ptys>, id: u32, command: String) -> Result<(), String> {
    let sessions = ptys.sessions.lock().unwrap();
    match &sessions.get(&id).ok_or("no such terminal")?.kind {
        SessionKind::Stream(stream) => stream.special(&command),
        SessionKind::Pty { .. } => Err("ssh can't send special commands".into()),
    }
}

/// A serial tab's line settings, as `-sercfg` ("9600,8,n,1,X")
#[tauri::command]
pub fn pty_serial_get(ptys: State<'_, Ptys>, id: u32) -> Result<String, String> {
    let sessions = ptys.sessions.lock().unwrap();
    match &sessions.get(&id).ok_or("no such terminal")?.kind {
        SessionKind::Stream(stream) => stream.serial_line().map(|l| l.to_sercfg()).ok_or_else(|| "Only a serial tab has line settings".into()),
        SessionKind::Pty { .. } => Err("Only a serial tab has line settings".into()),
    }
}

/// Change a serial tab's line settings while it's open; `sercfg` as PuTTY's -sercfg
#[tauri::command]
pub fn pty_serial_set(ptys: State<'_, Ptys>, id: u32, sercfg: String) -> Result<(), String> {
    let sessions = ptys.sessions.lock().unwrap();
    match &sessions.get(&id).ok_or("no such terminal")?.kind {
        SessionKind::Stream(stream) => {
            let mut line = stream.serial_line().ok_or("Only a serial tab has line settings")?;
            line.apply(&sercfg)?;
            stream.set_serial_line(line)
        }
        SessionKind::Pty { .. } => Err("Only a serial tab has line settings".into()),
    }
}

/// Watches a session's first output for a password prompt, to type the session's password there once
///
/// Only the first prompt is answered, so a later "Password:" (e.g. Cisco's `enable`) is never sent the login
/// password, and a wrong password leaves the second prompt to the user.
struct PasswordPrompt {
    password: String,
    /// The end of the output so far, enough to hold a prompt split across reads
    tail: String,
}

impl PasswordPrompt {
    /// Watches output for the password prompt, to type `password` at it once
    fn new(password: String) -> PasswordPrompt {
        PasswordPrompt { password, tail: String::new() }
    }

    /// The text to type if this output ends with a password prompt
    fn answer(&mut self, bytes: &[u8]) -> Option<String> {
        self.tail.push_str(&String::from_utf8_lossy(bytes));
        if self.tail.len() > 256 {
            let cut = self.tail.len() - 256;
            let cut = (cut..self.tail.len()).find(|i| self.tail.is_char_boundary(*i)).unwrap_or(cut);
            self.tail.drain(..cut);
        }
        let end = without_escapes(&self.tail);
        let end = end.trim_end().to_ascii_lowercase();
        (end.ends_with("password:") || end.ends_with("passcode:")).then(|| format!("{}\r", self.password))
    }
}

/// Output with the terminal's escape sequences taken out (CSI, OSC and the two-character ones), and an unfinished one
/// at the end too: Windows' ConPTY draws the space after "Password:" as a cursor move (ESC[1C) once the screen has
/// scrolled (ssh -v's debug lines), and the prompt then didn't end the output
fn without_escapes(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\x1b' {
            out.push(c);
            continue;
        }
        match chars.next() {
            // CSI: parameters and intermediates, up to a final byte from @ to ~
            Some('[') => for c in chars.by_ref() {
                if ('@'..='~').contains(&c) {
                    break;
                }
            },
            // OSC (a window title): up to BEL or ST (ESC \)
            Some(']') => while let Some(c) = chars.next() {
                if c == '\x07' || (c == '\x1b' && chars.next_if_eq(&'\\').is_some()) {
                    break;
                }
            },
            // a character set or keypad mode: one more character
            Some('(' | ')') => {
                chars.next();
            }
            _ => {}
        }
    }
    out
}

/// Type into a pseudo terminal: `data` is text as the page sends it ("input" messages), including control characters
#[tauri::command]
pub fn pty_write(ptys: State<'_, Ptys>, id: u32, data: String) -> Result<(), String> {
    let mut sessions = ptys.sessions.lock().unwrap();
    let session = sessions.get_mut(&id).ok_or("no such terminal")?;
    // the page's answer to where the cursor is (ESC[row;colR), for a capture; nothing typed is noted
    if data.starts_with("[") && data.ends_with('R') && data[2..data.len() - 1].chars().all(|c| c.is_ascii_digit() || c == ';') {
        session.outlet.0.lock().unwrap().captured(&format!("ANSWER {:?}", data));
    }
    let mut writer = session.writer.lock().unwrap();
    writer.write_all(&session.charset.encode(&data)).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())
}

/// Resize a pseudo terminal to the page's columns and rows, so the program redraws for them
#[tauri::command]
pub fn pty_resize(ptys: State<'_, Ptys>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let sessions = ptys.sessions.lock().unwrap();
    match &sessions.get(&id).ok_or("no such terminal")?.kind {
        SessionKind::Pty { master, .. } => {
            sessions[&id].outlet.resized(cols, rows);
            master.resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 }).map_err(|e| e.to_string())
        }
        SessionKind::Stream(stream) => {
            stream.resize(cols, rows);
            Ok(())
        }
    }
}

/// A command line for the app's log, each word quoted when it holds a space, and secrets masked: the word after -pw
/// or -pass, and the value of any option whose name says password (not ssh's PasswordAuthentication=yes/no) (the launch never puts the password itself in
/// ssh's arguments, as it types it at the prompt, but a profile's ssh options are free text)
fn shown_command(argv: &[std::ffi::OsString]) -> String {
    let mut words = Vec::new();
    let mut hide_next = false;
    for arg in argv {
        let arg = arg.to_string_lossy();
        let word = if hide_next {
            "********".to_string()
        } else if let Some((name, _)) = arg.split_once('=').filter(|(name, _)| {
            let name = name.to_ascii_lowercase();
            name.contains("pass") && !name.ends_with("authentication")
        }) {
            format!("{name}=********")
        } else {
            arg.to_string()
        };
        hide_next = matches!(arg.as_ref(), "-pw" | "-pass" | "--password");
        words.push(if word.contains(' ') { format!("\"{word}\"") } else { word });
    }
    words.join(" ")
}

/// Close a tab's pseudo terminal: end its program if it's still running and forget it
#[tauri::command]
pub fn pty_close(ptys: State<'_, Ptys>, id: u32) -> Result<(), String> {
    let session = ptys.sessions.lock().unwrap().remove(&id);
    end(session);
    Ok(())
}

/// Move to New Window: the session `id` goes to a tab in this window, which gets how it started, the recent output
/// and, if it has ended, how; its output comes here from now on, and closing this window ends it
#[tauri::command]
pub fn pty_attach(window: tauri::WebviewWindow, ptys: State<'_, Ptys>, id: u32, on_event: Channel<PtyEvent>) -> Result<(), String> {
    let mut sessions = ptys.sessions.lock().unwrap();
    let session = sessions.get_mut(&id).ok_or("The session ended before its tab moved.")?;
    session.window = window.label().to_string();
    session.outlet.attach(on_event);
    Ok(())
}

/// A window closed: end every session its tabs had, as closing each tab would
pub fn close_window(ptys: &Ptys, label: &str) {
    let gone: Vec<Session> = {
        let mut sessions = ptys.sessions.lock().unwrap();
        let ids: Vec<u32> = sessions.iter().filter(|(_, s)| s.window == label).map(|(id, _)| *id).collect();
        ids.iter().filter_map(|id| sessions.remove(id)).collect()
    };
    for session in gone {
        end(Some(session));
    }
}

/// End a session's program or connection
fn end(session: Option<Session>) {
    match session.map(|s| s.kind) {
        Some(SessionKind::Pty { child, .. }) => {
            let mut child = child.lock().unwrap();
            let _ = child.kill();
            let _ = child.wait();
        }
        Some(SessionKind::Stream(stream)) => stream.close(),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{shown_command, watch_exit, PasswordPrompt};
    use portable_pty::{native_pty_system, PtySize};
    use std::io::{Read, Write};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    /// A shell running `script` (its words for "a job in the background, then exit 3" differ): cmd on Windows, where
    /// MSYS's sh (Git's) keeps its own process alive until its background jobs end, sh elsewhere
    fn shell(unix: &str, windows: &str) -> portable_pty::CommandBuilder {
        if cfg!(windows) {
            let mut command = portable_pty::CommandBuilder::new("cmd.exe");
            command.args(["/c", windows]);
            command
        } else {
            let mut command = portable_pty::CommandBuilder::new("sh");
            command.args(["-c", unix]);
            command
        }
    }

    /// Read the output and answer the cursor position query, as the tab's page does: Windows' ConPTY asks it first
    /// (ESC[6n) and holds the program up until it's answered
    fn answer_the_terminal(master: &dyn portable_pty::MasterPty) {
        let mut output = master.try_clone_reader().unwrap();
        let mut input = master.take_writer().unwrap();
        std::thread::spawn(move || {
            let mut buf = [0u8; 4096];
            while let Ok(n) = output.read(&mut buf) {
                if n == 0 {
                    break;
                }
                if buf[..n].windows(4).any(|w| w == b"\x1b[6n") {
                    let _ = input.write_all(b"\x1b[1;1R");
                }
            }
        });
    }

    #[test]
    fn the_end_of_a_program_is_told_though_its_output_stays_open() {
        // a job left in the background holds the output open, as Windows' ConPTY does after ssh exits
        let pair = native_pty_system().openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }).unwrap();
        let command = shell("sleep 5 & exit 3", "start /b ping -n 6 127.0.0.1 >nul & exit 3");
        let child = Arc::new(Mutex::new(pair.slave.spawn_command(command).unwrap()));
        drop(pair.slave);
        answer_the_terminal(&*pair.master);
        let (sent, got) = std::sync::mpsc::channel();
        let finished = Arc::new(AtomicBool::new(false));
        watch_exit(child, finished.clone(), move |code| sent.send(code).unwrap());
        assert_eq!(got.recv_timeout(std::time::Duration::from_secs(3)).unwrap(), Some(3));
        assert!(finished.load(Ordering::SeqCst));
        drop(pair.master);
    }

    #[test]
    fn the_watcher_says_nothing_once_the_reader_has_told_of_the_end() {
        let pair = native_pty_system().openpty(PtySize { rows: 24, cols: 80, pixel_width: 0, pixel_height: 0 }).unwrap();
        let command = shell("exit 0", "exit 0");
        let child = Arc::new(Mutex::new(pair.slave.spawn_command(command).unwrap()));
        drop(pair.slave);
        answer_the_terminal(&*pair.master);
        let (sent, got) = std::sync::mpsc::channel();
        let finished = Arc::new(AtomicBool::new(true));
        watch_exit(child, finished, move |code| sent.send(code).unwrap());
        assert!(got.recv_timeout(std::time::Duration::from_secs(2)).is_err());
    }

    #[test]
    fn the_logged_command_hides_passwords() {
        let argv: Vec<std::ffi::OsString> =
            ["ssh", "-v", "-pw", "cisco", "-o", "PasswordAuthentication=no", "-o", "Password=x", "-l", "a b", "192.168.0.50"]
                .iter()
                .map(Into::into)
                .collect();
        assert_eq!(
            shown_command(&argv),
            "ssh -v -pw ******** -o PasswordAuthentication=no -o Password=******** -l \"a b\" 192.168.0.50"
        );
    }

    #[test]
    fn answers_the_first_prompt_even_when_split() {
        let mut prompt = PasswordPrompt::new("pw".into());
        assert_eq!(prompt.answer(b"(admin@192.168.0.50) Pass"), None);
        assert_eq!(prompt.answer(b"word: "), Some("pw\r".into()));
    }

    #[test]
    fn answers_a_prompt_conpty_ends_with_a_cursor_move() {
        // ssh -v's debug lines scroll the screen, and ConPTY then draws the prompt's trailing space as ESC[1C
        let mut prompt = PasswordPrompt::new("pw".into());
        assert_eq!(prompt.answer(b"debug1: Next authentication method: keyboard-interactive\r\n"), None);
        assert_eq!(prompt.answer(b"(admin@192.168.0.50) Password:\x1b[1C"), Some("pw\r".into()));
        // with the cursor shown, or a window title, after it; and a sequence split across reads
        assert_eq!(PasswordPrompt::new("pw".into()).answer(b"Password:\x1b[?25h\x1b]0;ssh\x07"), Some("pw\r".into()));
        let mut split = PasswordPrompt::new("pw".into());
        assert_eq!(split.answer(b"Password:\x1b["), Some("pw\r".into()));
    }

    #[test]
    fn ignores_other_output() {
        let mut prompt = PasswordPrompt::new("pw".into());
        assert_eq!(prompt.answer(b"Are you sure you want to continue connecting (yes/no/[fingerprint])? "), None);
    }

    #[test]
    fn a_tab_moved_to_another_window_is_shown_what_was_drawn_but_not_asked_the_old_questions() {
        // On Windows ConPTY opens every session asking where the cursor is (ESC[6n). Replayed in the new window's tab,
        // its terminal answered it again, into the session: a Cisco switch showed ";1R" at its prompt
        use super::Outlet;
        use base64::Engine;
        use tauri::ipc::{Channel, InvokeResponseBody};
        let outlet = Outlet::new(Channel::new(|_| Ok(())), crate::charset::Charset::Utf8);
        // ConPTY's start, a password prompt, and a program asking the terminal what it is and its colours
        outlet.data(b"\x1b[?9001h\x1b[?1004h\x1b[6n\x1b[H\x1b[2J(admin@192.168.0.50) Password: \r\n");
        outlet.data(b"\x1b[c\x1b[>0c\x1b[5n\x1b]11;?\x07\x1b[?2004$p\x1b]0;Ready?\x07\x1b[32mlab_cisco_switch#\x1b[0m");
        let shown = Arc::new(Mutex::new(Vec::<u8>::new()));
        let into = Arc::clone(&shown);
        outlet.attach(Channel::new(move |body| {
            if let InvokeResponseBody::Json(json) = body
                && let Ok(PtyEventSeen::Data { data }) = serde_json::from_str::<PtyEventSeen>(&json)
            {
                into.lock().unwrap().extend(base64::engine::general_purpose::STANDARD.decode(data).unwrap());
            }
            Ok(())
        }));
        let shown = String::from_utf8(shown.lock().unwrap().clone()).unwrap();
        assert_eq!(
            shown,
            "\x1b[?9001h\x1b[?1004h\x1b[H\x1b[2J(admin@192.168.0.50) Password: \r\n\x1b]0;Ready?\x07\x1b[32mlab_cisco_switch#\x1b[0m",
            "the new tab draws what the session showed, with nothing left for its terminal to answer"
        );
        // what the page receives, as PtyEvent sends it
        #[derive(serde::Deserialize)]
        #[serde(tag = "type", rename_all = "camelCase")]
        enum PtyEventSeen {
            Data { data: String },
            #[serde(other)]
            Other,
        }
    }

    #[test]
    fn a_latin1_devices_output_reaches_the_tab_as_utf8_and_so_does_a_moved_tabs_replay() {
        use super::Outlet;
        use base64::Engine;
        use tauri::ipc::{Channel, InvokeResponseBody};
        /// The text a channel is sent as output
        fn watched(into: Arc<Mutex<Vec<u8>>>) -> Channel<super::PtyEvent> {
            Channel::new(move |body| {
                if let InvokeResponseBody::Json(json) = body
                    && let Ok(value) = serde_json::from_str::<serde_json::Value>(&json)
                    && value["type"] == "data"
                {
                    into.lock().unwrap().extend(base64::engine::general_purpose::STANDARD.decode(value["data"].as_str().unwrap()).unwrap());
                }
                Ok(())
            })
        }
        let first = Arc::new(Mutex::new(Vec::new()));
        let outlet = Outlet::new(watched(first.clone()), crate::charset::Charset::Latin1);
        outlet.data(b"Interface d\xe9sactiv\xe9e\r\n");
        assert_eq!(String::from_utf8(first.lock().unwrap().clone()).unwrap(), "Interface désactivée\r\n");
        let moved = Arc::new(Mutex::new(Vec::new()));
        outlet.attach(watched(moved.clone()));
        assert_eq!(String::from_utf8(moved.lock().unwrap().clone()).unwrap(), "Interface désactivée\r\n");
    }
}
