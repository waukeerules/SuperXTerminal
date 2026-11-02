//! Connect external's login: the Terminal window's ssh gets the session's password without it ever being on the
//! command line, in the environment or on screen. The command carries a one-time token instead, and ssh is told to
//! ask SuperXTerminal itself for passwords (`SSH_ASKPASS`, this app's own program, with `SSH_ASKPASS_REQUIRE=force`).
//! Run that way, the program swaps the token for the password over a socket only this user can reach, and the token
//! is then spent. Anything else ssh asks (trusting a new host key, a key's passphrase, the password again after a
//! wrong one) is asked in the Terminal window, as ssh would itself.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Set on the ssh command: the token, and that this program is being run as ssh's askpass
pub const TOKEN_VAR: &str = "SXT_ASKPASS";

/// How long a token waits for ssh to use it
const LIFETIME: Duration = Duration::from_secs(120);

/// Tokens not yet used, with their password and when they were made
static TOKENS: Mutex<Option<HashMap<String, (String, Instant)>>> = Mutex::new(None);

/// The socket, in this user's own temporary folder
fn socket_path() -> std::path::PathBuf {
    std::env::temp_dir().join("SuperXTerminal").join("askpass.sock")
}

/// A token for a password, good once, for two minutes; the listener starts with the first one
#[cfg(unix)]
pub fn register(password: &str) -> Result<String, String> {
    listen()?;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).map_err(|e| e.to_string())?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let mut tokens = TOKENS.lock().unwrap();
    let tokens = tokens.get_or_insert_with(HashMap::new);
    tokens.retain(|_, (_, made)| made.elapsed() < LIFETIME);
    tokens.insert(token.clone(), (password.to_string(), Instant::now()));
    Ok(token)
}

/// Answer tokens on the socket (once per run of the app): a line with a token gets its password, once
#[cfg(unix)]
fn listen() -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;
    use std::os::unix::net::UnixListener;
    static STARTED: std::sync::OnceLock<Result<(), String>> = std::sync::OnceLock::new();
    STARTED
        .get_or_init(|| {
            let path = socket_path();
            let folder = path.parent().unwrap();
            std::fs::create_dir_all(folder).map_err(|e| e.to_string())?;
            let _ = std::fs::set_permissions(folder, std::fs::Permissions::from_mode(0o700));
            let _ = std::fs::remove_file(&path);
            let listener = UnixListener::bind(&path).map_err(|e| format!("Couldn't start the login helper: {e}"))?;
            let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
            std::thread::Builder::new()
                .name("askpass".into())
                .spawn(move || {
                    for stream in listener.incoming().flatten() {
                        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                        let mut line = String::new();
                        let mut reader = BufReader::new(&stream);
                        if reader.read_line(&mut line).is_err() {
                            continue;
                        }
                        let password = TOKENS
                            .lock()
                            .unwrap()
                            .as_mut()
                            .and_then(|tokens| tokens.remove(line.trim()))
                            .filter(|(_, made)| made.elapsed() < LIFETIME)
                            .map(|(password, _)| password)
                            .unwrap_or_default();
                        let mut stream = &stream;
                        let _ = stream.write_all(password.as_bytes());
                    }
                })
                .map_err(|e| e.to_string())?;
            Ok(())
        })
        .clone()
}

#[cfg(not(unix))]
pub fn register(_password: &str) -> Result<String, String> {
    Err("Not on this system".into())
}

/// Run as ssh's askpass (the token is set): print the answer to ssh's question and say so. The password question
/// is answered with the token's password the first time; anything else is asked in the terminal.
#[cfg(unix)]
pub fn helper() -> bool {
    let Ok(token) = std::env::var(TOKEN_VAR) else { return false };
    let prompt = std::env::args().nth(1).unwrap_or_default();
    let lower = prompt.to_lowercase();
    let mut answer = None;
    if lower.contains("password") && !lower.contains("passphrase") {
        answer = fetch(&token).filter(|p| !p.is_empty());
    }
    let answer = answer.or_else(|| ask_in_terminal(&prompt, !lower.contains("yes/no"))).unwrap_or_default();
    println!("{answer}");
    true
}

#[cfg(not(unix))]
pub fn helper() -> bool {
    false
}

/// The token's password from the running app, if it's still waiting
#[cfg(unix)]
fn fetch(token: &str) -> Option<String> {
    let mut stream = std::os::unix::net::UnixStream::connect(socket_path()).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(5))).ok()?;
    stream.write_all(format!("{token}\n").as_bytes()).ok()?;
    stream.shutdown(std::net::Shutdown::Write).ok()?;
    let mut password = String::new();
    stream.read_to_string(&mut password).ok()?;
    Some(password)
}

/// Ask in the Terminal window, hiding what's typed for a secret
#[cfg(unix)]
fn ask_in_terminal(prompt: &str, secret: bool) -> Option<String> {
    use std::os::fd::AsRawFd;
    let mut tty = std::fs::OpenOptions::new().read(true).write(true).open("/dev/tty").ok()?;
    let _ = tty.write_all(prompt.as_bytes());
    let _ = tty.flush();
    let fd = tty.as_raw_fd();
    // SAFETY: termios calls on the terminal opened above
    let saved = unsafe {
        let mut t: libc::termios = std::mem::zeroed();
        if !secret || libc::tcgetattr(fd, &mut t) != 0 {
            None
        } else {
            let saved = t;
            t.c_lflag &= !libc::ECHO;
            libc::tcsetattr(fd, libc::TCSANOW, &t);
            Some(saved)
        }
    };
    let mut line = String::new();
    let read = BufReader::new(&tty).read_line(&mut line);
    if let Some(saved) = saved {
        // SAFETY: as above; the terminal's settings put back
        unsafe { libc::tcsetattr(fd, libc::TCSANOW, &saved) };
        let _ = tty.write_all(b"\n");
    }
    read.ok()?;
    Some(line.trim_end_matches(['\r', '\n']).to_string())
}
