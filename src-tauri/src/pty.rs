//! Pseudo terminals for terminal tabs.
//!
//! Each tab's page (SuperXPuTTY's `web/terminal`, through `src/lib/terminal/PageHost.ts`) asks for a pseudo terminal
//! with `pty_open`, types into it with `pty_write` and resizes it with `pty_resize`. Output comes back on the tab's
//! own `Channel` as base64, the form the page's "data" message already takes. What runs in it is a [`Target`]: the
//! user's login shell, or a stored session started as `launch.rs` builds it.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use base64::Engine;
use portable_pty::{native_pty_system, Child, MasterPty, PtySize};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::State;

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
}

/// What a pseudo terminal sends its tab
#[derive(Clone, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PtyEvent {
    /// Output, base64-encoded; UTF-8 may be split across chunks, which xterm.js handles
    Data { data: String },
    /// The program ended; `code` is its exit code when it has one
    Exit { code: Option<u32> },
    /// Something to tell the user in the terminal before the program's own output
    Notice { text: String, kind: String },
    /// How the page should treat the tab: `local` for a local shell, and the profile's cursor and bell
    Started { local: bool, look: Look },
}

/// One open pseudo terminal: the master side (for resizing), its writer (for typing) and the program in it
struct Session {
    master: Box<dyn MasterPty + Send>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    child: Box<dyn Child + Send + Sync>,
}

/// Every open pseudo terminal, by the id given to its tab
#[derive(Default)]
pub struct Ptys {
    next: AtomicU32,
    sessions: Mutex<HashMap<u32, Session>>,
}

/// Start a tab's program (`target`) in a new pseudo terminal of the given size, sending its output on `on_event`
///
/// Returns the id the tab uses for the other calls; Err when the session can't start on this Mac.
#[tauri::command]
pub fn pty_open(
    ptys: State<'_, Ptys>,
    state: State<'_, AppState>,
    target: Target,
    cols: u16,
    rows: u16,
    on_event: Channel<PtyEvent>,
) -> Result<u32, String> {
    let profiles = state.profiles.lock().unwrap().clone();
    let launch = match target {
        Target::LocalShell => launch::local_shell(&profiles),
        Target::Session { store, id } => {
            let sessions = state.sessions.lock().unwrap();
            let session = sessions.find(store, &id).ok_or_else(|| format!("The session {id} is gone."))?;
            launch::launch(&sessions, &profiles, store, session)?
        }
    };
    let Launch { mut command, password, local, notices, look } = launch;
    let _ = on_event.send(PtyEvent::Started { local, look });
    for text in notices {
        let _ = on_event.send(PtyEvent::Notice { text, kind: "info".into() });
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
    if command.get_cwd().is_none() {
        if let Some(home) = dirs::home_dir() {
            command.cwd(home);
        }
    }
    let child = pair.slave.spawn_command(command).map_err(|e| e.to_string())?;
    // the slave side belongs to the child now; keeping it open here would stop the reader seeing the end
    drop(pair.slave);

    let mut reader = pair.master.try_clone_reader().map_err(|e| e.to_string())?;
    let writer = Arc::new(Mutex::new(pair.master.take_writer().map_err(|e| e.to_string())?));
    let id = ptys.next.fetch_add(1, Ordering::Relaxed) + 1;

    let typist = writer.clone();
    std::thread::Builder::new()
        .name(format!("pty-{id}"))
        .spawn(move || {
            let mut prompt = password.map(PasswordPrompt::new);
            let mut buffer = [0u8; 64 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let data = base64::engine::general_purpose::STANDARD.encode(&buffer[..n]);
                        if on_event.send(PtyEvent::Data { data }).is_err() {
                            break;
                        }
                        if let Some(answer) = prompt.as_mut().and_then(|p| p.answer(&buffer[..n])) {
                            let mut writer = typist.lock().unwrap();
                            let _ = writer.write_all(answer.as_bytes()).and_then(|_| writer.flush());
                            prompt = None;
                        }
                    }
                }
            }
            let _ = on_event.send(PtyEvent::Exit { code: None });
        })
        .map_err(|e| e.to_string())?;

    ptys.sessions.lock().unwrap().insert(id, Session { master: pair.master, writer, child });
    Ok(id)
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
        let end = self.tail.trim_end().to_ascii_lowercase();
        (end.ends_with("password:") || end.ends_with("passcode:")).then(|| format!("{}\r", self.password))
    }
}

/// Type into a pseudo terminal: `data` is text as the page sends it ("input" messages), including control characters
#[tauri::command]
pub fn pty_write(ptys: State<'_, Ptys>, id: u32, data: String) -> Result<(), String> {
    let mut sessions = ptys.sessions.lock().unwrap();
    let session = sessions.get_mut(&id).ok_or("no such terminal")?;
    let mut writer = session.writer.lock().unwrap();
    writer.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())
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

#[cfg(test)]
mod tests {
    use super::PasswordPrompt;

    #[test]
    fn answers_the_first_prompt_even_when_split() {
        let mut prompt = PasswordPrompt::new("pw".into());
        assert_eq!(prompt.answer(b"(admin@192.168.0.50) Pass"), None);
        assert_eq!(prompt.answer(b"word: "), Some("pw\r".into()));
    }

    #[test]
    fn ignores_other_output() {
        let mut prompt = PasswordPrompt::new("pw".into());
        assert_eq!(prompt.answer(b"Are you sure you want to continue connecting (yes/no/[fingerprint])? "), None);
    }
}
