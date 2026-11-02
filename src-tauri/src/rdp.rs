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
use ironrdp_pdu::input::fast_path::{FastPathInputEvent, KeyboardFlags};
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

/// One open Remote Desktop: where its input goes, and the window whose tab it is
struct Session {
    input: mpsc::UnboundedSender<RdpInputEvent>,
    window: String,
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
}

/// The session's address and login, as SuperXPuTTY gives them to its RDP control: the user's local overrides
/// applied, placeholders filled, the folders' username and port where the session has none, "DOMAIN\\user" split,
/// and the password from -pw or the profile
fn target(state: &AppState, store: Store, id: &str) -> Result<Target, String> {
    let sessions = state.sessions.lock().unwrap();
    let stored = sessions.find(store, id).ok_or_else(|| format!("The session {id} is gone."))?;
    let session = crate::overrides::applied(&state.user_folder, stored);
    let values = crate::variables::values(&state.user_folder);
    let fill = |text: &str| crate::variables::substitute(text, &values);
    let along = sessions.along(store, session.folder());
    let host = fill(session.get("Host").trim());
    if host.is_empty() {
        return Err("This session has no host.".into());
    }
    let mut username = fill(session.get("Username").trim());
    if username.is_empty() {
        username = along.iter().map(|f| f.username.clone()).find(|u| !u.is_empty()).map(|u| fill(&u)).unwrap_or_default();
    }
    let mut port = session.port();
    if port == 0 {
        port = along.iter().map(|f| f.port).find(|p| *p != 0).unwrap_or(0);
    }
    let (domain, username) = split_user(&username);
    let mut password = None;
    crate::editing::replace_password(session.get("ExtraArgs"), |p| {
        password = Some(p.to_string());
        p.to_string()
    });
    let profile = session.get("PuttySession");
    let password = password
        .filter(|p| !p.is_empty())
        .or_else(|| crate::profiles::password(if profile.is_empty() { "Default Settings" } else { profile }))
        .or_else(|| crate::profiles::password("Default Settings"));
    Ok(Target { host, port: if port == 0 { 3389 } else { port }, domain, username, password })
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

/// Connect a tab to a session's Remote Desktop at the tab's size (in device pixels); returns the id for the other
/// calls. `username` and `password` are ones typed in the tab for this connection (never stored).
#[tauri::command]
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
) -> Result<u32, String> {
    let mut target = target(&state, store, &id)?;
    if let Some(typed) = username.filter(|u| !u.trim().is_empty()) {
        (target.domain, target.username) = split_user(typed.trim());
    }
    if target.username.is_empty() {
        return Err(LOGIN_NEEDED.into());
    }
    let Some(password) = password.filter(|p| !p.is_empty()).or(target.password) else {
        return Err(PASSWORD_NEEDED.into());
    };
    let address = format!("{}:{}", target.host, target.port);
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
        .with_desktop_width(width.max(200))
        .with_desktop_height(height.max(200))
        .with_desktop_scale_factor(scale.clamp(100, 500))
        .with_color_depth(32)
        .with_credssp(true)
        .with_sound(true);
    if let Some(domain) = &target.domain {
        builder = builder.with_domain(domain.clone());
    }
    let config = builder.build().map_err(|e| e.to_string())?;
    let (output, mut events) = mpsc::channel(32);
    let client = RdpClient::new(config, output);
    let input = client.input_sender();
    let rdp_id = rdps.next.fetch_add(1, Ordering::Relaxed) + 1;
    rdps.sessions.lock().unwrap().insert(rdp_id, Session { input, window: window.label().to_string() });
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
                    crate::applog::warn(format!("Remote Desktop to {address} failed: {error}"));
                    send_json(&on_event, &RdpEvent::Ended { reason: plain_failure(&error.to_string()), clean: false });
                    break;
                }
                RdpOutputEvent::Terminated(result) => {
                    let (reason, clean) = match result {
                        Ok(reason) => (format!("{reason}"), true),
                        Err(error) => (error.to_string(), false),
                    };
                    crate::applog::info(format!("Remote Desktop to {address} ended: {reason}"));
                    send_json(&on_event, &RdpEvent::Ended { reason, clean });
                    break;
                }
            }
        }
    });
    Ok(rdp_id)
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

fn send_json(channel: &Channel<InvokeResponseBody>, event: &RdpEvent) {
    if let Ok(text) = serde_json::to_string(event) {
        let _ = channel.send(InvokeResponseBody::Json(text));
    }
}

/// The smallest rectangle (x, y, width, height) holding every pixel that differs; None when nothing did. Rows are
/// compared whole first (a memory compare), and only changed rows are searched for their changed columns.
fn changed_rect(old: &[u32], new: &[u32], w: u16, h: u16) -> Option<(u16, u16, u16, u16)> {
    let width = w as usize;
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

fn send(rdps: &Rdps, id: u32, event: RdpInputEvent) -> Result<(), String> {
    let sessions = rdps.sessions.lock().unwrap();
    let session = sessions.get(&id).ok_or("The Remote Desktop session has ended.")?;
    session.input.send(event).map_err(|_| "The Remote Desktop session has ended.".to_string())
}

/// A key pressed or let go, as its PC scancode (`extended` for the E0 keys: arrows, right Ctrl and Alt, ...)
#[tauri::command]
pub fn rdp_key(rdps: State<'_, Rdps>, id: u32, scancode: u8, extended: bool, down: bool) -> Result<(), String> {
    let mut flags = KeyboardFlags::empty();
    if extended {
        flags |= KeyboardFlags::EXTENDED;
    }
    if !down {
        flags |= KeyboardFlags::RELEASE;
    }
    send(&rdps, id, RdpInputEvent::FastPath(smallvec![FastPathInputEvent::KeyboardEvent(flags, scancode)]))
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

/// The tab changed size (device pixels): the remote desktop follows (Fit the tab)
#[tauri::command]
pub fn rdp_resize(rdps: State<'_, Rdps>, id: u32, width: u16, height: u16, scale: u32) -> Result<(), String> {
    send(&rdps, id, RdpInputEvent::Resize { width: width.max(200), height: height.max(200), scale_factor: scale.clamp(100, 500), physical_size: None })
}

/// The tab closed: disconnect
#[tauri::command]
pub fn rdp_close(rdps: State<'_, Rdps>, id: u32) {
    if let Some(session) = rdps.sessions.lock().unwrap().remove(&id) {
        let _ = session.input.send(RdpInputEvent::Close);
    }
}

/// A window closed: its Remote Desktops disconnect
pub fn close_window(rdps: &Rdps, label: &str) {
    rdps.sessions.lock().unwrap().retain(|_, s| {
        if s.window == label {
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
}
