//! Pseudo terminals for terminal tabs.
//!
//! Each tab's page (SuperXPuTTY's `web/terminal`, through `src/lib/terminal/PageHost.ts`) asks for a pseudo terminal
//! with `pty_open`, types into it with `pty_write` and resizes it with `pty_resize`. Output comes back on the tab's
//! own `Channel` as base64, the form the page's "data" message already takes. M0 runs the user's login shell; SSH
//! (`ssh -tt`) comes in M1 through the same code.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};

use base64::Engine;
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

/// What a pseudo terminal sends its tab
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PtyEvent {
    /// Output, base64-encoded; UTF-8 may be split across chunks, which xterm.js handles
    Data { data: String },
    /// The program ended; `code` is its exit code when it has one
    Exit { code: Option<u32> },
}

/// One open pseudo terminal: the master side (for resizing), its writer (for typing) and the program in it
struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Box<dyn Child + Send + Sync>,
}

/// Every open pseudo terminal, by the id given to its tab
#[derive(Default)]
pub struct Ptys {
    next: AtomicU32,
    sessions: Mutex<HashMap<u32, Session>>,
}

/// Start the user's login shell in a new pseudo terminal of the given size, sending its output on `on_event`
///
/// Returns the id the tab uses for the other calls.
#[tauri::command]
pub fn pty_open(ptys: State<'_, Ptys>, cols: u16, rows: u16, on_event: Channel<PtyEvent>) -> Result<u32, String> {
    let pair = native_pty_system()
        .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())?;

    // new_default_prog is the user's login shell, started as a login shell, in their home folder
    let mut command = CommandBuilder::new_default_prog();
    command.env("TERM", "xterm-256color");
    command.env("COLORTERM", "truecolor");
    command.env("TERM_PROGRAM", "SuperXTerminal");
    // an app started from Finder has no LANG, and shells then treat UTF-8 input as bytes
    if std::env::var_os("LANG").is_none() {
        command.env("LANG", "en_US.UTF-8");
    }
    let child = pair.slave.spawn_command(command).map_err(|e| e.to_string())?;
    // the slave side belongs to the child now; keeping it open here would stop the reader seeing the end
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = pair.master.take_writer().map_err(|e| e.to_string())?;
    let id = ptys.next.fetch_add(1, Ordering::Relaxed) + 1;

    std::thread::Builder::new()
        .name(format!("pty-{id}"))
        .spawn(move || {
            let mut buffer = [0u8; 64 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let data = base64::engine::general_purpose::STANDARD.encode(&buffer[..n]);
                        if on_event.send(PtyEvent::Data { data }).is_err() {
                            break;
                        }
                    }
                }
            }
            // the exit code is read by pty_close's wait, or here once the program has gone
            let _ = on_event.send(PtyEvent::Exit { code: None });
        })
        .map_err(|e| e.to_string())?;

    ptys.sessions.lock().unwrap().insert(id, Session { master: pair.master, writer, child });
    Ok(id)
}

/// Type into a pseudo terminal: `data` is text as the page sends it ("input" messages), including control characters
#[tauri::command]
pub fn pty_write(ptys: State<'_, Ptys>, id: u32, data: String) -> Result<(), String> {
    let mut sessions = ptys.sessions.lock().unwrap();
    let session = sessions.get_mut(&id).ok_or("no such terminal")?;
    session.writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    session.writer.flush().map_err(|e| e.to_string())
}

/// Resize a pseudo terminal to the page's columns and rows, so the program redraws for them
#[tauri::command]
pub fn pty_resize(ptys: State<'_, Ptys>, id: u32, cols: u16, rows: u16) -> Result<(), String> {
    let sessions = ptys.sessions.lock().unwrap();
    let session = sessions.get(&id).ok_or("no such terminal")?;
    session
        .master
        .resize(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
        .map_err(|e| e.to_string())
}

/// Close a tab's pseudo terminal: end its program if it's still running and forget it
#[tauri::command]
pub fn pty_close(ptys: State<'_, Ptys>, id: u32) -> Result<(), String> {
    if let Some(mut session) = ptys.sessions.lock().unwrap().remove(&id) {
        let _ = session.child.kill();
        let _ = session.child.wait();
    }
    Ok(())
}
