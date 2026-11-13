//! Remote Desktop in a tab, as SuperXPuTTY shows RDP sessions with Microsoft's control (ctlApplicationPanel /
//! RdpOptions). The Mac has no such control, so the connection is IronRDP's client engine (Devolutions, MIT/Apache):
//! it connects (TLS, and CredSSP/NLA for the login), decodes the screen, and takes keyboard and mouse input.
//!
//! The engine hands over the whole screen after every update; it's compared with the last one here and only the
//! rectangle that changed goes to the tab, as raw bytes on its `Channel` (`Frame`). Events (started, the pointer,
//! the end) go as JSON on the same channel.

use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use ironrdp_client::config::{ConfigBuilder, Destination};
use ironrdp_client::rdp::{RdpClient, RdpInputEvent, RdpOutputEvent};
use ironrdp_pdu::input::fast_path::{FastPathInputEvent, KeyboardFlags, SynchronizeFlags};
use ironrdp_pdu::input::mouse::PointerFlags;
use ironrdp_pdu::input::MousePdu;
use serde::Serialize;
use smallvec::smallvec;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::State;
use tokio::sync::mpsc;

use crate::sessions::Store;
use crate::AppState;

/// A packet of screen: "SXF1", x, y, width, height of the rectangle and of the whole screen (u16 LE each), then the
/// rectangle's pixels as RGBA
const FRAME: &[u8; 4] = b"SXF1";
/// A pointer shape: "SXP1", width, height, hotspot x and y (u16 LE), then RGBA
const POINTER: &[u8; 4] = b"SXP1";

/// What else the tab hears (as JSON)
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum RdpEvent {
    /// Connecting to this host
    Connecting { host: String },
    /// The pointer: "default", "hidden", or at a position on the remote screen
    Pointer { kind: &'static str, x: u16, y: u16 },
    /// The connection couldn't be made, or the session ended; `clean` for a logoff or disconnect from Windows
    Ended { reason: String, clean: bool },
}

/// One open Remote Desktop: where its input goes, the window whose tab it is, its clipboard, and how its desktop
/// follows the tab (not at all when its size is fixed; in Retina pixels when that's on)
struct Session {
    input: mpsc::UnboundedSender<RdpInputEvent>,
    window: String,
    clipboard: Option<crate::rdp_clipboard::Shared>,
    fixed: bool,
    retina: bool,
}

/// What the tab is told once `rdp_open` has started: its id, the desktop's size, and how to show it
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Opened {
    id: u32,
    width: u16,
    height: u16,
    /// The size stays (Display=Fixed); else the desktop follows the tab
    fixed: bool,
    /// Scale the desktop to fit the tab (SmartSizing); else shown at its size, scrolled
    smart_sizing: bool,
    /// The desktop's scale (100, or the screen's with Retina): its pixels per point, times 100
    scale: u32,
    /// Windows key combinations (⌘ and a key as Windows and it): 0 on this computer (never), 1 on the remote
    /// computer (always), 2 only in full screen
    keyboard: u32,
}

/// Every open Remote Desktop, by the id given to its tab
#[derive(Default)]
pub struct Rdps {
    next: AtomicU32,
    sessions: Mutex<HashMap<u32, Session>>,
}

/// What `rdp_open` needs from the session: where, who, and the password
struct Target {
    host: String,
    port: u16,
    domain: Option<String>,
    username: String,
    password: Option<String>,
    /// The options it connects with: the defaults and the session's own
    options: crate::rdp_options::RdpOptions,
}

/// The session's address and login, as SuperXPuTTY gives them to its RDP control: the user's local overrides
/// applied, placeholders filled, the username from [`login`], the folders' port where the session has none,
/// "DOMAIN\\user" split, and the password from [`stored_password`]
fn target(state: &AppState, store: Store, id: &str) -> Result<Target, String> {
    let overrides_on = state.settings.lock().unwrap().use_local_overrides;
    let sessions = state.sessions.lock().unwrap();
    let stored = sessions.find(store, id).ok_or_else(|| format!("The session {id} is gone."))?;
    let session = crate::overrides::applied(&state.user_folder, stored, overrides_on);
    let values = crate::variables::values(&state.user_folder);
    let fill = |text: &str| crate::variables::substitute(text, &values);
    let along = sessions.along(store, session.folder());
    let host = fill(session.get("Host").trim());
    if host.is_empty() {
        return Err("This session has no host.".into());
    }
    let profiles = crate::profiles::Profiles::load(&state.user_folder);
    let (username, profile) = login(&sessions, &profiles, &values, store, &session);
    let mut port = session.port();
    if port == 0 {
        port = along.iter().map(|f| f.port).find(|p| *p != 0).unwrap_or(0);
    }
    let (domain, username) = split_user(&username);
    let password = stored_password(&profiles, &session, &profile)
        // a 1Password reference (op://...) in its place: read from 1Password now (onepassword.rs)
        .map(|p| crate::onepassword::resolved(&p))
        .filter(|p| !p.is_empty());
    let defaults = state.settings.lock().unwrap().rdp_defaults.clone();
    let options = crate::rdp_options::RdpOptions::effective(&defaults, session.get("RdpOptions"));
    Ok(Target { host, port: if port == 0 { 3389 } else { port }, domain, username, password, options })
}

/// The password an RDP session logs in with, in the order SSH sessions use (launch.rs), before a 1Password reference
/// is read: the session's own (Edit Session's Password, this computer's only), else its -pw, else its profile's (or
/// the first along its Based on), else Default Settings'. A jump server profile (Log in here) gives none, as its
/// password is the jump server's. One typed in the tab comes before all of these (`rdp_open`).
fn stored_password(
    profiles: &crate::profiles::Profiles,
    session: &crate::sessions::StoredSession,
    profile: &str,
) -> Option<String> {
    let mut from_args = None;
    crate::editing::replace_password(session.get("ExtraArgs"), |p| {
        from_args = Some(p.to_string());
        p.to_string()
    });
    let jump_server = profiles.get(profile).is_some_and(|p| p.log_in_here);
    Some(session.id())
        .filter(|id| !id.is_empty())
        .and_then(crate::session_passwords::get)
        .or(from_args)
        .filter(|p| !p.is_empty())
        .or_else(|| if jump_server { None } else { profiles.password_along(profile) })
        .or_else(|| crate::profiles::password(crate::profiles::DEFAULT_PROFILE))
        .filter(|p| !p.is_empty())
}

/// Who an RDP session logs in as, and the profile its password comes from, in the order SSH sessions use
/// (launch.rs): the session's username, else its folders' (nearest first), else its profile's (with what it's
/// based on), else Default Settings'. The profile is the session's, else its folders', else Default Settings. A jump
/// server profile (Log in here) gives no username, as its login is the jump server's, not the desktop's. Its
/// placeholders are filled from `values`, and a username that's a 1Password reference (op://...), as typed or as an
/// Environment Variable's value, is read from 1Password: sent as it was, the server refused it as a wrong login.
fn login(
    sessions: &crate::sessions::Sessions,
    profiles: &crate::profiles::Profiles,
    values: &std::collections::HashMap<String, String>,
    store: Store,
    session: &crate::sessions::StoredSession,
) -> (String, String) {
    let along = sessions.along(store, session.folder());
    let mut profile_name = session.get("PuttySession").to_string();
    if crate::profiles::is_default(&profile_name) {
        profile_name = along.iter().map(|f| f.putty_profile.clone()).find(|p| !p.is_empty()).unwrap_or(profile_name);
    }
    let default = profiles.default_profile();
    let resolved = if crate::profiles::is_default(&profile_name) { None } else { profiles.resolved(&profile_name) };
    let profile = resolved.as_ref().unwrap_or(default);
    let from_profile = if profile.log_in_here { String::new() } else { profile.username.clone() };
    let username = [session.get("Username").to_string()]
        .into_iter()
        .chain(along.iter().map(|f| f.username.clone()))
        .chain([from_profile, default.username.clone()])
        .find(|u| !u.trim().is_empty())
        .unwrap_or_default();
    let username = crate::onepassword::resolved(&crate::variables::substitute(username.trim(), values));
    (username.trim().to_string(), profile.name.clone())
}

/// ".\\user" is a local account (no domain); "CORP\\user" a domain one; "user@domain" as it is
fn split_user(username: &str) -> (Option<String>, String) {
    match username.split_once('\\') {
        Some((".", user)) => (None, user.to_string()),
        Some((domain, user)) => (Some(domain.to_string()), user.to_string()),
        None => (None, username.to_string()),
    }
}

/// What `rdp_open` says when there's no password to log in with: the tab asks for one and tries again
pub const PASSWORD_NEEDED: &str = "RDP_PASSWORD_NEEDED";
/// ... or no username either (the session and its folders have none): the tab asks for both
pub const LOGIN_NEEDED: &str = "RDP_LOGIN_NEEDED";

/// Connect a tab to a session's Remote Desktop; returns its id, the desktop's size and how to show it. `width` and
/// `height` are the tab's in points and `scale` the screen's (200 on Retina): the desktop is that size in points, in
/// Retina pixels with the Retina option, or the options' fixed size. `username` and `password` are ones typed in the
/// tab for this connection (never stored). async: a 1Password reference for its password waits for the user's
/// approval, which mustn't hold up the window
#[tauri::command(async)]
#[allow(clippy::too_many_arguments)]
pub fn rdp_open(
    window: tauri::WebviewWindow,
    state: State<'_, AppState>,
    rdps: State<'_, Rdps>,
    store: Store,
    id: String,
    width: u16,
    height: u16,
    scale: u32,
    username: Option<String>,
    password: Option<String>,
    on_event: Channel<InvokeResponseBody>,
) -> Result<Opened, String> {
    let mut target = target(&state, store, &id)?;
    if let Some(typed) = username.filter(|u| !u.trim().is_empty()) {
        (target.domain, target.username) = split_user(typed.trim());
    }
    if target.username.is_empty() {
        return Err(LOGIN_NEEDED.into());
    }
    let Some(password) = password.filter(|p| !p.is_empty()).or(target.password.clone()) else {
        return Err(PASSWORD_NEEDED.into());
    };
    let password_for_gateway = password.clone();
    let address = format!("{}:{}", target.host, target.port);
    let options = &target.options;
    let fixed = options.display.as_deref() == Some("Fixed");
    let retina = options.retina == Some(true) && !fixed;
    let (width, height, scale) = desktop_size(options, width, height, scale);
    let mut builder = ConfigBuilder::new()
        .with_destination(Destination::new(address.clone()).map_err(|e| e.to_string())?)
        .with_username(target.username.clone())
        .with_password(password)
        .with_client_name("SuperXTerminal")
        // who's connecting, as the RDP client info says it: Microsoft's clients send their ActiveX control's path,
        // which servers only log; the platform is macOS
        .with_client_build(0)
        .with_client_dir("C:\\Windows\\System32\\mstscax.dll")
        .with_platform(ironrdp_pdu::rdp::capability_sets::MajorPlatformType::OSX)
        .with_desktop_width(width)
        .with_desktop_height(height)
        .with_desktop_scale_factor(scale)
        // IronRDP draws 16 or 32 bits; SuperXPuTTY's 15 and 24 are the nearest
        .with_color_depth(if options.color_depth.unwrap_or(32) <= 16 { 16 } else { 32 })
        .with_credssp(true)
        // Audio=0 plays here; 1 (on the remote computer) and 2 (don't play) send none here
        .with_sound(options.audio.unwrap_or(0) == 0)
        // no bulk compression: IronRDP drops its decompressor when Windows re-activates the session (after the
        // desktop is resized, e.g. going full screen) while Windows keeps compressing, and every update after is garbage
        .with_compression(false);
    if let Some(domain) = &target.domain {
        builder = builder.with_domain(domain.clone());
    }
    // this computer's clipboard, as this channel's own backend in place of the engine's (rdp_clipboard.rs says why)
    let clipboard = (options.clipboard != Some(false)).then(crate::rdp_clipboard::Shared::default);
    builder = builder.with_clipboard(ironrdp_client::config::ClipboardType::Disable);
    if let Some(shared) = clipboard.clone() {
        builder = builder.with_static_channel(move |_| {
            use ironrdp_cliprdr::backend::CliprdrBackendFactory;
            Some(ironrdp_cliprdr::CliprdrClient::new(crate::rdp_clipboard::Factory(shared.clone()).build_cliprdr_backend()))
        });
    }
    // this computer's drives in the remote desktop's File Explorer (rdp_drives.rs), with the Drives option
    if options.drives == Some(true) {
        let drives = crate::rdp_drives::local_drives();
        let names: Vec<&str> = drives.iter().map(|drive| drive.name.as_str()).collect();
        crate::applog::info(format!("Remote Desktop to {address} is offered this computer's drives: {}", names.join(", ")));
        builder = builder.with_static_channel(move |_| Some(crate::rdp_drives::channel(&drives)));
        // Windows says nothing on the device channel unless the sound channel is there as well (MS-RDPEFS, Appendix
        // A <1>); when sound isn't played here, one that takes no sound stands in for it
        if options.audio.unwrap_or(0) != 0 {
            builder = builder.with_static_channel(|_| {
                Some(ironrdp_rdpsnd::client::Rdpsnd::new(Box::new(ironrdp_rdpsnd::client::NoopRdpsndBackend)))
            });
        }
    }
    // RD Gateway, as SuperXPuTTY's: the session's own login is the gateway's too
    if let Some(gateway) = options.gateway.as_deref().map(str::trim).filter(|g| !g.is_empty()) {
        let endpoint = if gateway.contains(':') { gateway.to_string() } else { format!("{gateway}:443") };
        let gateway_user = match &target.domain {
            Some(domain) => format!("{domain}\\{}", target.username),
            None => target.username.clone(),
        };
        builder = builder
            .with_transport(ironrdp_client::config::TransportKind::Gateway { endpoint })
            .with_gateway_username(gateway_user)
            .with_gateway_password(password_for_gateway.clone());
    }
    let config = builder.build().map_err(|e| e.to_string())?;
    let (output, mut events) = mpsc::channel(32);
    let client = RdpClient::new(config, output);
    let input = client.input_sender();
    if let Some(shared) = &clipboard {
        *shared.sender.lock().unwrap() = Some(input.clone());
    }
    let rdp_id = rdps.next.fetch_add(1, Ordering::Relaxed) + 1;
    rdps.sessions.lock().unwrap().insert(rdp_id, Session { input, window: window.label().to_string(), clipboard, fixed, retina });
    crate::applog::info(format!("Remote Desktop to {address} as {} (session {id}) in {}", target.username, window.label()));
    send_json(&on_event, &RdpEvent::Connecting { host: address.clone() });

    // the engine's loop isn't Send (its clipboard backend), so it gets a thread and a runtime of its own
    let name = format!("rdp-{rdp_id}");
    std::thread::Builder::new()
        .name(name)
        .spawn(move || match tokio::runtime::Builder::new_current_thread().enable_all().build() {
            Ok(runtime) => runtime.block_on(client.run()),
            Err(e) => crate::applog::error(format!("Remote Desktop couldn't start: {e}")),
        })
        .map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn(async move {
        let mut last: Vec<u32> = Vec::new();
        let (mut last_w, mut last_h) = (0u16, 0u16);
        // events that came in while skipping to the newest screen, handled next
        let mut queued: std::collections::VecDeque<RdpOutputEvent> = std::collections::VecDeque::new();
        loop {
            let event = match queued.pop_front() {
                Some(event) => event,
                None => match events.recv().await {
                    Some(event) => event,
                    None => break,
                },
            };
            match event {
                RdpOutputEvent::Image { mut buffer, mut width, mut height } => {
                    // each screen holds the whole desktop, so only the newest waiting one is drawn: when updates come
                    // faster than the tab draws them, it skips ahead instead of falling further behind
                    while let Ok(next) = events.try_recv() {
                        match next {
                            RdpOutputEvent::Image { buffer: b, width: w, height: h } => (buffer, width, height) = (b, w, h),
                            other => queued.push_back(other),
                        }
                    }
                    let (w, h) = (width.get(), height.get());
                    let changed = if w != last_w || h != last_h || last.len() != buffer.len() {
                        Some((0, 0, w, h))
                    } else {
                        changed_rect(&last, &buffer, w, h)
                    };
                    if let Some(rect) = changed {
                        let _ = on_event.send(InvokeResponseBody::Raw(frame(&buffer, w, h, rect)));
                    }
                    last = buffer;
                    (last_w, last_h) = (w, h);
                }
                RdpOutputEvent::PointerDefault => send_json(&on_event, &RdpEvent::Pointer { kind: "default", x: 0, y: 0 }),
                RdpOutputEvent::PointerHidden => send_json(&on_event, &RdpEvent::Pointer { kind: "hidden", x: 0, y: 0 }),
                RdpOutputEvent::PointerPosition { x, y } => send_json(&on_event, &RdpEvent::Pointer { kind: "position", x, y }),
                RdpOutputEvent::PointerBitmap(pointer) => {
                    let mut packet = Vec::with_capacity(12 + pointer.bitmap_data.len());
                    packet.extend_from_slice(POINTER);
                    for v in [pointer.width, pointer.height, pointer.hotspot_x, pointer.hotspot_y] {
                        packet.extend_from_slice(&v.to_le_bytes());
                    }
                    packet.extend_from_slice(&pointer.bitmap_data);
                    let _ = on_event.send(InvokeResponseBody::Raw(packet));
                }
                RdpOutputEvent::ConnectionFailure(error) => {
                    crate::applog::warn(format!("Remote Desktop to {address} failed: {}", with_causes(&error)));
                    send_json(&on_event, &RdpEvent::Ended { reason: plain_failure(&with_causes(&error)), clean: false });
                    break;
                }
                RdpOutputEvent::Terminated(result) => {
                    let (reason, clean) = match result {
                        Ok(reason) => (format!("{reason}"), true),
                        Err(error) => (with_causes(&error), false),
                    };
                    crate::applog::info(format!("Remote Desktop to {address} ended: {reason}"));
                    send_json(&on_event, &RdpEvent::Ended { reason, clean });
                    break;
                }
            }
        }
    });
    Ok(Opened { id: rdp_id, width, height, fixed, smart_sizing: options.smart_sizing != Some(false), scale, keyboard: options.keyboard.unwrap_or(1) })
}

/// The desktop's size and scale: the options' fixed size, else the tab's in points at 100%, or in Retina pixels at
/// the screen's scale with the Retina option
fn desktop_size(options: &crate::rdp_options::RdpOptions, width: u16, height: u16, scale: u32) -> (u16, u16, u32) {
    let clamp = |v: u32| v.clamp(200, 8192) as u16;
    if options.display.as_deref() == Some("Fixed") {
        return (clamp(options.width.unwrap_or(1920)), clamp(options.height.unwrap_or(1080)), 100);
    }
    if options.retina == Some(true) {
        let factor = scale.clamp(100, 300);
        return (clamp(width as u32 * factor / 100), clamp(height as u32 * factor / 100), factor);
    }
    (clamp(width as u32), clamp(height as u32), 100)
}

/// An error with what caused it (IronRDP's own message is often only "custom error")
fn with_causes(error: &dyn std::error::Error) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(&format!(": {cause}"));
        source = cause.source();
    }
    text
}

/// A failure as the user can act on it
fn plain_failure(error: &str) -> String {
    let lower = error.to_lowercase();
    if lower.contains("logon") || lower.contains("credssp") || lower.contains("authentication") || lower.contains("0xc000006d") {
        format!("The login was refused: check the username and password. ({error})")
    } else if lower.contains("connection refused") || lower.contains("timed out") || lower.contains("no route") {
        format!("The computer couldn't be reached on the Remote Desktop port. ({error})")
    } else {
        error.to_string()
    }
}

/// Send an event to the tab as JSON; a tab that's gone misses it
fn send_json(channel: &Channel<InvokeResponseBody>, event: &RdpEvent) {
    if let Ok(text) = serde_json::to_string(event) {
        let _ = channel.send(InvokeResponseBody::Json(text));
    }
}

/// The smallest rectangle (x, y, width, height) holding every pixel that differs; None when nothing did. Rows are
/// compared whole first (a memory compare), and only changed rows are searched for their changed columns.
fn changed_rect(old: &[u32], new: &[u32], w: u16, h: u16) -> Option<(u16, u16, u16, u16)> {
    let width = w as usize;
    /// Row `y` of a frame `width` pixels wide
    fn row(pixels: &[u32], width: usize, y: usize) -> &[u32] {
        &pixels[y * width..(y + 1) * width]
    }
    let row = |pixels, y| row(pixels, width, y);
    let differs = |y: usize| row(old, y) != row(new, y);
    let top = (0..h as usize).find(|&y| differs(y))?;
    let bottom = (top..h as usize).rev().find(|&y| differs(y)).unwrap_or(top);
    let (mut left, mut right) = (width, 0);
    for y in top..=bottom {
        let (a, b) = (row(old, y), row(new, y));
        if a == b {
            continue;
        }
        // only the part outside what's already known to change needs looking at
        if let Some(first) = a[..left.min(width)].iter().zip(&b[..left.min(width)]).position(|(p, q)| p != q) {
            left = first;
        }
        let from = (right + 1).min(width);
        if let Some(last) = a[from..].iter().zip(&b[from..]).rposition(|(p, q)| p != q) {
            right = from + last;
        }
        right = right.max(left);
    }
    Some((left as u16, top as u16, (right - left + 1) as u16, (bottom - top + 1) as u16))
}

/// A screen packet for a rectangle of the screen (0x00RRGGBB pixels to RGBA)
fn frame(buffer: &[u32], w: u16, h: u16, (x, y, rw, rh): (u16, u16, u16, u16)) -> Vec<u8> {
    let mut packet = Vec::with_capacity(16 + rw as usize * rh as usize * 4);
    packet.extend_from_slice(FRAME);
    for v in [x, y, rw, rh, w, h] {
        packet.extend_from_slice(&v.to_le_bytes());
    }
    for row in y as usize..(y + rh) as usize {
        let pixels = &buffer[row * w as usize + x as usize..row * w as usize + (x + rw) as usize];
        packet.extend(pixels.iter().flat_map(|pixel| {
            let [_, r, g, b] = pixel.to_be_bytes();
            [r, g, b, 255]
        }));
    }
    packet
}

/// Pass the tab's input to the session's engine, or say the session has ended
fn send(rdps: &Rdps, id: u32, event: RdpInputEvent) -> Result<(), String> {
    let sessions = rdps.sessions.lock().unwrap();
    let session = sessions.get(&id).ok_or("The Remote Desktop session has ended.")?;
    session.input.send(event).map_err(|_| "The Remote Desktop session has ended.".to_string())
}

/// A key pressed or let go, as its PC scancode (`extended` for the E0 keys: arrows, right Ctrl and Alt, ...)
#[tauri::command]
pub fn rdp_key(rdps: State<'_, Rdps>, id: u32, scancode: u8, extended: bool, down: bool) -> Result<(), String> {
    send(&rdps, id, RdpInputEvent::FastPath(smallvec![key_event(scancode, extended, down)]))
}

/// A key pressed or let go as the session's engine takes it: its PC scancode, with the flags for an extended (E0)
/// key and for a key let go
pub(crate) fn key_event(scancode: u8, extended: bool, down: bool) -> FastPathInputEvent {
    let mut flags = KeyboardFlags::empty();
    if extended {
        flags |= KeyboardFlags::EXTENDED;
    }
    if !down {
        flags |= KeyboardFlags::RELEASE;
    }
    FastPathInputEvent::KeyboardEvent(flags, scancode)
}

/// The tab got the keyboard (`on`) or lost it, on Windows: while it has it and its Windows key combinations go to
/// the remote computer, the Windows key and what's pressed with it are taken from this computer and sent there
/// (rdp_win_keys.rs). Nothing to do on the Mac and Linux.
#[tauri::command]
pub fn rdp_grab_keys(rdps: State<'_, Rdps>, id: u32, on: bool) {
    let input = rdps.sessions.lock().unwrap().get(&id).map(|s| s.input.clone());
    match input {
        Some(input) if on => crate::rdp_win_keys::grab(id, input),
        _ => crate::rdp_win_keys::let_go(id),
    }
}

/// Several keys in one go, each (scancode, extended, down), so a combination arrives whole: Ctrl+Alt+Del from the
/// tab's menu, ⌘C as Ctrl+C
#[tauri::command]
pub fn rdp_keys(rdps: State<'_, Rdps>, id: u32, keys: Vec<(u8, bool, bool)>) -> Result<(), String> {
    let events = keys.into_iter().map(|(scancode, extended, down)| key_event(scancode, extended, down)).collect();
    send(&rdps, id, RdpInputEvent::FastPath(events))
}

/// The US keyboard's key for a character: its scancode and whether Shift is pressed with it
fn us_key(c: char) -> Option<(u8, bool)> {
    /// The US keyboard's rows: their characters, shifted characters and the scancode of the first key
    const ROWS: [(&str, &str, u8); 4] = [
        ("`1234567890-=", "~!@#$%^&*()_+", 0x29),
        ("qwertyuiop[]\\", "QWERTYUIOP{}|", 0x10),
        ("asdfghjkl;'", "ASDFGHJKL:\"", 0x1e),
        ("zxcvbnm,./", "ZXCVBNM<>?", 0x2c),
    ];
    if c == ' ' {
        return Some((0x39, false));
    }
    for (plain, shifted, first) in ROWS {
        for (shift, keys) in [(false, plain), (true, shifted)] {
            if let Some(at) = keys.chars().position(|k| k == c) {
                // the number row starts at ` (0x29) then 1 (0x02); \\ is 0x2b, after ]
                let code = match (first, at) {
                    (0x29, 0) => 0x29,
                    (0x29, n) => 0x01 + n as u8,
                    (0x10, 12) => 0x2b,
                    (f, n) => f + n as u8,
                };
                return Some((code, shift));
            }
        }
    }
    None
}

/// Text typed (Send Commands, the button bar, scripts), as SuperXPuTTY's RdpControl.SendKeystrokes: a character on
/// the US keyboard as its key (with Shift), as the remote is expected to use the same layout; any other as Unicode;
/// Enter, Tab, Backspace and Escape as their keys.
#[tauri::command]
pub async fn rdp_text(rdps: State<'_, Rdps>, id: u32, text: String) -> Result<(), String> {
    let key = |scancode: u8, down: bool| FastPathInputEvent::KeyboardEvent(if down { KeyboardFlags::empty() } else { KeyboardFlags::RELEASE }, scancode);
    let mut presses: Vec<Vec<FastPathInputEvent>> = Vec::new();
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        let special = match c {
            '\r' | '\n' => {
                // \r\n is one Enter
                if c == '\r' && chars.peek() == Some(&'\n') {
                    chars.next();
                }
                Some(0x1c)
            }
            '\t' => Some(0x0f),
            '\u{8}' | '\u{7f}' => Some(0x0e),
            '\u{1b}' => Some(0x01),
            _ => None,
        };
        if let Some(scancode) = special.map(|s| (s, false)).or_else(|| us_key(c)) {
            let (scancode, shift) = scancode;
            let mut press = Vec::new();
            if shift {
                press.push(key(0x2a, true));
            }
            press.push(key(scancode, true));
            press.push(key(scancode, false));
            if shift {
                press.push(key(0x2a, false));
            }
            presses.push(press);
            continue;
        }
        let mut units = [0u16; 2];
        for unit in c.encode_utf16(&mut units) {
            presses.push(vec![
                FastPathInputEvent::UnicodeKeyboardEvent(KeyboardFlags::empty(), *unit),
                FastPathInputEvent::UnicodeKeyboardEvent(KeyboardFlags::RELEASE, *unit),
            ]);
        }
    }
    // each key down and up on its own, a little apart, as a keyboard sends them: Windows drops keys that come in a
    // burst, or pressed and let go in one message
    let input = rdps.sessions.lock().unwrap().get(&id).map(|s| s.input.clone()).ok_or("The Remote Desktop session has ended.")?;
    for event in presses.into_iter().flatten() {
        input
            .send(RdpInputEvent::FastPath(smallvec![event]))
            .map_err(|_| "The Remote Desktop session has ended.".to_string())?;
        tokio::time::sleep(std::time::Duration::from_millis(6)).await;
    }
    Ok(())
}

/// Caps Lock and Num Lock as the Mac has them, so letters come out in the same case
#[tauri::command]
pub fn rdp_sync(rdps: State<'_, Rdps>, id: u32, caps: bool, num: bool) -> Result<(), String> {
    let mut flags = SynchronizeFlags::empty();
    if caps {
        flags |= SynchronizeFlags::CAPS_LOCK;
    }
    if num {
        flags |= SynchronizeFlags::NUM_LOCK;
    }
    send(&rdps, id, RdpInputEvent::FastPath(smallvec![FastPathInputEvent::SyncEvent(flags)]))
}

/// The mouse at (x, y) on the remote screen: moved, a button ("left", "right", "middle") pressed or let go, or the
/// wheel turned (`wheel` in notches, + away from the user)
#[tauri::command]
pub fn rdp_mouse(rdps: State<'_, Rdps>, id: u32, x: u16, y: u16, button: Option<String>, down: bool, wheel: i16, horizontal: bool) -> Result<(), String> {
    let mut flags = PointerFlags::empty();
    let mut units = 0i16;
    if wheel != 0 {
        flags |= if horizontal { PointerFlags::HORIZONTAL_WHEEL } else { PointerFlags::VERTICAL_WHEEL };
        // 120 a notch, as Windows counts them; the sign goes in the flags
        units = (wheel.unsigned_abs().min(2) as i16) * 120;
        if wheel < 0 {
            flags |= PointerFlags::WHEEL_NEGATIVE;
            units = -units;
        }
    } else if let Some(button) = button.as_deref() {
        flags |= match button {
            "right" => PointerFlags::RIGHT_BUTTON,
            "middle" => PointerFlags::MIDDLE_BUTTON_OR_WHEEL,
            _ => PointerFlags::LEFT_BUTTON,
        };
        if down {
            flags |= PointerFlags::DOWN;
        }
    } else {
        flags |= PointerFlags::MOVE;
    }
    let pdu = MousePdu { flags, number_of_wheel_rotation_units: units, x_position: x, y_position: y };
    send(&rdps, id, RdpInputEvent::FastPath(smallvec![FastPathInputEvent::MouseEvent(pdu)]))
}

/// The tab changed size (points, at the screen's `scale`): the remote desktop follows (Fit the tab), unless its size
/// is fixed
#[tauri::command]
pub fn rdp_resize(rdps: State<'_, Rdps>, id: u32, width: u16, height: u16, scale: u32) -> Result<(), String> {
    let (fixed, retina) = rdps.sessions.lock().unwrap().get(&id).map(|s| (s.fixed, s.retina)).unwrap_or((true, false));
    if fixed {
        return Ok(());
    }
    let options = crate::rdp_options::RdpOptions { retina: Some(retina), ..Default::default() };
    let (width, height, scale_factor) = desktop_size(&options, width, height, scale);
    send(&rdps, id, RdpInputEvent::Resize { width, height, scale_factor, physical_size: None })
}

/// The tab got the keyboard: text copied on this computer meanwhile is offered to the remote desktop's clipboard
#[tauri::command]
pub fn rdp_clipboard_announce(rdps: State<'_, Rdps>, id: u32) -> bool {
    let clipboard = rdps.sessions.lock().unwrap().get(&id).and_then(|s| s.clipboard.clone());
    clipboard.is_some_and(|c| c.announce())
}

/// The tab closed: disconnect
#[tauri::command]
pub fn rdp_close(rdps: State<'_, Rdps>, id: u32) {
    crate::rdp_win_keys::let_go(id);
    if let Some(session) = rdps.sessions.lock().unwrap().remove(&id) {
        let _ = session.input.send(RdpInputEvent::Close);
    }
}

/// A window closed: its Remote Desktops disconnect
pub fn close_window(rdps: &Rdps, label: &str) {
    rdps.sessions.lock().unwrap().retain(|id, s| {
        if s.window == label {
            crate::rdp_win_keys::let_go(*id);
            let _ = s.input.send(RdpInputEvent::Close);
            false
        } else {
            true
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A Remote Desktop session in folder "Lab" with the given attributes
    fn desktop(attributes: &[(&str, &str)]) -> crate::sessions::StoredSession {
        let mut all = vec![("SessionId".to_string(), "Lab/ts1".to_string()), ("Host".to_string(), "10.0.0.9".to_string())];
        all.extend(attributes.iter().map(|(n, v)| (n.to_string(), v.to_string())));
        crate::sessions::StoredSession { attributes: all }
    }

    /// Default Settings with `default_user`, "Windows" with its own username, "Windows Old" based on it, and a jump
    /// server profile (Log in here) with one
    fn windows_profiles(default_user: &str) -> crate::profiles::Profiles {
        use crate::profiles::Profile;
        crate::profiles::Profiles {
            profiles: vec![
                Profile { name: crate::profiles::DEFAULT_PROFILE.into(), username: default_user.into(), ..Profile::default() },
                Profile { name: "Windows".into(), username: "CORP\\admin".into(), ..Profile::default() },
                Profile { name: "Windows Old".into(), based_on: "Windows".into(), ..Profile::default() },
                Profile { name: "Bastion".into(), username: "jumper".into(), log_in_here: true, ..Profile::default() },
            ],
        }
    }

    #[test]
    fn a_desktop_with_no_username_of_its_own_logs_in_as_its_profiles() {
        use crate::sessions::{FolderSetting, Sessions};
        let profiles = windows_profiles("");
        let none = Sessions::default();
        let login = |sessions: &Sessions, profiles: &crate::profiles::Profiles, attributes: &[(&str, &str)]| {
            login(sessions, profiles, &std::collections::HashMap::new(), Store::Saved, &desktop(attributes))
        };
        // the profile's username, and its password's name
        assert_eq!(login(&none, &profiles, &[("PuttySession", "Windows")]), ("CORP\\admin".into(), "Windows".into()));
        // ... or the one it's based on
        assert_eq!(login(&none, &profiles, &[("PuttySession", "Windows Old")]).0, "CORP\\admin");
        // the session's own wins
        assert_eq!(login(&none, &profiles, &[("PuttySession", "Windows"), ("Username", "me")]).0, "me");
        // then its folder's
        let mut lab = Sessions::default();
        let folder = FolderSetting { path: "Lab".into(), username: "labuser".into(), ..FolderSetting::default() };
        lab.folders.entry(Store::Saved).or_default().insert("lab".into(), folder);
        assert_eq!(login(&lab, &profiles, &[("PuttySession", "Windows")]).0, "labuser");
        // a folder's profile, when the session names none
        let mut profiled = Sessions::default();
        let folder = FolderSetting { path: "Lab".into(), putty_profile: "Windows".into(), ..FolderSetting::default() };
        profiled.folders.entry(Store::Saved).or_default().insert("lab".into(), folder);
        assert_eq!(login(&profiled, &profiles, &[]), ("CORP\\admin".into(), "Windows".into()));
        // Default Settings' when the profile has none, or there's no profile
        let defaulted = windows_profiles("fallback");
        assert_eq!(login(&none, &defaulted, &[]), ("fallback".into(), crate::profiles::DEFAULT_PROFILE.into()));
        // a jump server's login isn't the desktop's
        assert_eq!(login(&none, &profiles, &[("PuttySession", "Bastion")]).0, "");
        // nothing anywhere: the tab asks
        assert_eq!(login(&none, &profiles, &[]).0, "");
    }

    #[test]
    fn a_desktops_username_that_is_a_1password_reference_logs_in_as_what_1password_holds() {
        let profiles = windows_profiles("");
        let none = crate::sessions::Sessions::default();
        crate::onepassword::keep("op://Private/rdp-test/username", "me@outlook.com");
        let values = std::collections::HashMap::from([("rdp_user".to_string(), "\"op://Private/rdp-test/username\"".to_string())]);
        let login = |attributes: &[(&str, &str)]| login(&none, &profiles, &values, Store::Saved, &desktop(attributes)).0;
        // typed as 1Password copies it, in quotes for its spaces
        assert_eq!(login(&[("Username", "\"op://Private/rdp-test/username\"")]), "me@outlook.com");
        // as an Environment Variable's value
        assert_eq!(login(&[("Username", "{rdp_user}")]), "me@outlook.com");
        // one that can't be read: none, so the tab asks for the login
        assert_eq!(login(&[("Username", "op://Private/rdp-test-missing/username")]), "");
        crate::onepassword::take_problems();
    }

    #[test]
    fn a_desktops_password_is_its_own_then_its_pw_then_its_profiles_along_based_on() {
        use crate::profiles::{Profile, Profiles};
        let profiles = Profiles {
            profiles: vec![
                Profile { name: crate::profiles::DEFAULT_PROFILE.into(), ..Profile::default() },
                Profile { name: "RDP Base".into(), ..Profile::default() },
                Profile { name: "RDP Child".into(), based_on: "RDP Base".into(), ..Profile::default() },
                Profile { name: "RDP Bastion".into(), log_in_here: true, ..Profile::default() },
            ],
        };
        crate::profiles::set_password("RDP Base", "from-base").unwrap();
        crate::profiles::set_password("RDP Bastion", "jump-servers").unwrap();
        let mut own = desktop(&[("ExtraArgs", "-pw from-args")]);
        own.set("SessionId", "Lab/rdp-own-password");
        let folder = std::env::temp_dir().join(format!("sxt-rdp-own-password-{}", std::process::id()));
        // the session's own comes first
        crate::session_passwords::set(&folder, "Lab/rdp-own-password", "its-own").unwrap();
        assert_eq!(stored_password(&profiles, &own, "RDP Child").as_deref(), Some("its-own"));
        // then its -pw
        crate::session_passwords::set(&folder, "Lab/rdp-own-password", "").unwrap();
        assert_eq!(stored_password(&profiles, &own, "RDP Child").as_deref(), Some("from-args"));
        // then the profile's, found along Based on
        let mut bare = desktop(&[]);
        bare.set("SessionId", "Lab/rdp-bare");
        assert_eq!(stored_password(&profiles, &bare, "RDP Child").as_deref(), Some("from-base"));
        // a jump server's isn't the desktop's
        assert_eq!(stored_password(&profiles, &bare, "RDP Bastion"), None);
        let _ = std::fs::remove_dir_all(&folder);
    }

    #[test]
    fn types_us_keys() {
        assert_eq!(us_key('a'), Some((0x1e, false)));
        assert_eq!(us_key('A'), Some((0x1e, true)));
        assert_eq!(us_key('1'), Some((0x02, false)));
        assert_eq!(us_key('!'), Some((0x02, true)));
        assert_eq!(us_key('0'), Some((0x0b, false)));
        assert_eq!(us_key('`'), Some((0x29, false)));
        assert_eq!(us_key('\\'), Some((0x2b, false)));
        assert_eq!(us_key('|'), Some((0x2b, true)));
        assert_eq!(us_key('"'), Some((0x28, true)));
        assert_eq!(us_key('?'), Some((0x35, true)));
        assert_eq!(us_key('é'), None);
    }

    #[test]
    fn finds_the_changed_rectangle() {
        let old = vec![0u32; 6 * 4];
        let mut new = old.clone();
        new[6 + 2] = 1;
        new[2 * 6 + 4] = 1;
        assert_eq!(changed_rect(&old, &new, 6, 4), Some((2, 1, 3, 2)));
        assert_eq!(changed_rect(&old, &old, 6, 4), None);
        let packet = frame(&new, 6, 4, (2, 1, 3, 2));
        assert_eq!(&packet[..4], FRAME);
        assert_eq!(packet.len(), 16 + 3 * 2 * 4);
    }

    /// The connection sequence a terminal server's PDUs are fed to, as IronRDP's client engine drives it, standing at
    /// the phase `state` with the server's MCS channels: I/O 1003, the user's 1007, the message channel 1004.
    mod connecting {
        use std::sync::Arc;

        use ironrdp_connector::connection_activation::{ConnectionActivationSequence, ConnectionActivationState};
        use ironrdp_connector::{
            ClientConnector, ClientConnectorState, ConnectorResult, LicenseCache, LicenseExchangeSequence,
            LicenseExchangeState, Sequence, State as _,
        };
        use ironrdp_core::{decode, encode_vec, WriteBuf};
        use ironrdp_pdu::mcs::{SendDataIndication, SendDataRequest};
        use ironrdp_pdu::rdp::autodetect::{AutoDetectResponse, AutoDetectRspPdu};
        use ironrdp_pdu::rdp::server_license::{LicenseEncryptionData, LicenseInformation};
        use ironrdp_pdu::x224::X224;

        use super::*;

        const IO: u16 = 1003;
        const USER: u16 = 1007;
        const MESSAGE: u16 = 1004;

        /// No licences kept, as the app keeps none
        #[derive(Debug)]
        struct NoLicences;
        impl LicenseCache for NoLicences {
            fn get_license(&self, _: LicenseInformation) -> ConnectorResult<Option<Vec<u8>>> {
                Ok(None)
            }
            fn store_license(&self, _: LicenseInformation) -> ConnectorResult<()> {
                Ok(())
            }
        }

        /// The connector config the app's own builder makes, for a made-up terminal server
        fn config() -> ironrdp_connector::Config {
            ConfigBuilder::new()
                .with_destination(Destination::new("ts.example:3389").unwrap())
                .with_username("me".to_string())
                .with_password("secret".to_string())
                .with_client_name("SuperXTerminal")
                .with_client_build(0)
                .with_client_dir(r"C:\Windows\System32\mstscax.dll")
                .with_platform(ironrdp_pdu::rdp::capability_sets::MajorPlatformType::OSX)
                .build()
                .unwrap()
                .connector()
                .clone()
        }

        /// A connector at `state`, the message channel negotiated
        fn at(state: ClientConnectorState) -> ClientConnector {
            let mut connector = ClientConnector::new(config(), "127.0.0.1:50000".parse().unwrap());
            connector.message_channel_id = Some(MESSAGE);
            connector.state = state;
            connector
        }

        /// Licensing, the client's New License Request sent and the server's Platform Challenge awaited
        fn awaiting_the_platform_challenge() -> ClientConnectorState {
            let mut license_exchange = LicenseExchangeSequence::new(IO, "me".into(), None, [0; 4], Arc::new(NoLicences));
            license_exchange.state = LicenseExchangeState::PlatformChallenge {
                encryption_data: LicenseEncryptionData { premaster_secret: vec![0; 48], mac_salt_key: vec![0; 16], license_key: vec![0; 16] },
            };
            ClientConnectorState::LicensingExchange { io_channel_id: IO, user_channel_id: USER, license_exchange }
        }

        /// An Auto-Detect Request on the message channel: the security header (SEC_AUTODETECT_REQ), then `request`
        fn on_the_message_channel(request: &[u8]) -> Vec<u8> {
            let mut user_data = vec![0x00, 0x10, 0x00, 0x00];
            user_data.extend_from_slice(request);
            encode_vec(&X224(SendDataIndication { initiator_id: 1002, channel_id: MESSAGE, user_data: user_data.into() }))
                .unwrap()
        }

        /// The probe Windows Server 2022 with RDS licensing sends in the middle of licensing (IronRDP #1629's capture,
        /// byte for byte): continuous network detection's Bandwidth Measure Start (request type 0x0014), number 0,
        /// which wants no answer
        fn bandwidth_probe() -> Vec<u8> {
            on_the_message_channel(&[0x06, 0x00, 0x00, 0x00, 0x14, 0x00])
        }

        /// A round-trip probe: RTT Measure Request (connect-time, request type 0x1001) number 5, which wants one back
        fn round_trip_probe() -> Vec<u8> {
            on_the_message_channel(&[0x06, 0x00, 0x05, 0x00, 0x01, 0x10])
        }

        /// The Auto-Detect Response the client sent back, on the message channel
        fn answer(output: &WriteBuf) -> AutoDetectResponse {
            let request = decode::<X224<SendDataRequest<'_>>>(output.filled()).unwrap().0;
            assert_eq!((request.initiator_id, request.channel_id), (USER, MESSAGE));
            decode::<AutoDetectRspPdu>(&request.user_data).unwrap().response
        }

        /// The connector, still waiting for the server's Platform Challenge
        fn still_awaiting_the_platform_challenge(connector: &ClientConnector) {
            let ClientConnectorState::LicensingExchange { license_exchange, .. } = &connector.state else {
                panic!("left licensing: {}", connector.state.name());
            };
            assert!(matches!(license_exchange.state, LicenseExchangeState::PlatformChallenge { .. }));
        }

        #[test]
        fn a_terminal_servers_probe_in_the_middle_of_licensing_doesnt_end_the_connection() {
            let mut connector = at(awaiting_the_platform_challenge());
            let mut output = WriteBuf::new();
            connector.step(&bandwidth_probe(), &mut output).expect("the probe doesn't end the connection");
            assert!(output.filled().is_empty(), "a Bandwidth Measure Start isn't answered");
            still_awaiting_the_platform_challenge(&connector);
            connector.step(&round_trip_probe(), &mut output).expect("nor does a round-trip probe");
            assert_eq!(answer(&output), AutoDetectResponse::RttResponse { sequence_number: 5 });
            still_awaiting_the_platform_challenge(&connector);
        }

        #[test]
        fn a_terminal_servers_probe_before_its_capabilities_doesnt_end_the_connection() {
            let mut connector = at(ClientConnectorState::CapabilitiesExchange {
                connection_activation: ConnectionActivationSequence::new(config(), IO, USER),
            });
            let mut output = WriteBuf::new();
            connector.step(&bandwidth_probe(), &mut output).expect("the probe doesn't end the connection");
            connector.step(&round_trip_probe(), &mut output).expect("nor does a round-trip probe");
            assert_eq!(answer(&output), AutoDetectResponse::RttResponse { sequence_number: 5 });
            let ClientConnectorState::CapabilitiesExchange { connection_activation } = &connector.state else {
                panic!("left the capabilities exchange: {}", connector.state.name());
            };
            assert!(matches!(connection_activation.connection_activation_state(), ConnectionActivationState::CapabilitiesExchange));
        }
    }
}
