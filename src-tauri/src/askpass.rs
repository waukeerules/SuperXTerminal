//! Logins for ssh run by the app itself rather than in a tab: Connect external's Terminal window and the SFTP file
//! transfer's connection. Their ssh gets the session's password without it ever being on the command line, in the
//! environment or on screen. The command carries a token instead, and ssh is told to ask SuperXTerminal itself
//! (`SSH_ASKPASS`, this app's own program, with `SSH_ASKPASS_REQUIRE=force`). Run that way, the program asks the
//! running app over a socket only this user can reach: the password, once; anything else (trusting a new host key,
//! a key's passphrase, the password again after a wrong one) is asked in the terminal when there is one (Connect
//! external), else by the app in a dialog (file transfer).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Set on the ssh command: the token, and that this program is being run as ssh's askpass
pub const TOKEN_VAR: &str = "SXT_ASKPASS";

/// How long a token lasts: a password waits two minutes for ssh; one that can ask the user, as long as it's kept
const LIFETIME: Duration = Duration::from_secs(120);

/// Asks the user ssh's question (the prompt, and whether the answer is secret); None when cancelled
pub type Asker = Arc<dyn Fn(&str, bool) -> Option<String> + Send + Sync>;

struct Entry {
    /// Given at the first password question of each ssh (one run, or each run for a token with an asker)
    password: Option<String>,
    /// The ssh processes that have had it: asked again, the password was wrong, so the user is asked
    given: std::collections::HashSet<u32>,
    /// For questions the password doesn't answer; a token with one lasts until it's dropped
    asker: Option<Asker>,
    made: Instant,
}

static TOKENS: Mutex<Option<HashMap<String, Entry>>> = Mutex::new(None);

/// The socket, in this user's own temporary folder
fn socket_path() -> std::path::PathBuf {
    std::env::temp_dir().join("SuperXTerminal").join("askpass.sock")
}

/// A token for a password, good once, for two minutes (Connect external)
pub fn register(password: &str) -> Result<String, String> {
    add(Entry { password: Some(password.to_string()), given: Default::default(), asker: None, made: Instant::now() })
}

/// A token for a connection that may run ssh several times (file transfer: SFTP, or SCP's runs): each ssh gets its
/// password (if any) once, and anything else is asked of the user; drop it with `forget`
pub fn register_asking(password: Option<&str>, asker: Asker) -> Result<String, String> {
    add(Entry { password: password.map(str::to_string), given: Default::default(), asker: Some(asker), made: Instant::now() })
}

/// The token's connection is done
pub fn forget(token: &str) {
    if let Some(tokens) = TOKENS.lock().unwrap().as_mut() {
        tokens.remove(token);
    }
}

#[cfg(unix)]
fn add(entry: Entry) -> Result<String, String> {
    listen()?;
    let mut bytes = [0u8; 16];
    std::fs::File::open("/dev/urandom").and_then(|mut f| f.read_exact(&mut bytes)).map_err(|e| e.to_string())?;
    let token: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let mut tokens = TOKENS.lock().unwrap();
    let tokens = tokens.get_or_insert_with(HashMap::new);
    tokens.retain(|_, e| e.asker.is_some() || e.made.elapsed() < LIFETIME);
    tokens.insert(token.clone(), entry);
    Ok(token)
}

#[cfg(not(unix))]
fn add(_entry: Entry) -> Result<String, String> {
    Err("Not on this system".into())
}

/// The answer to one request from the ssh `ssh_pid`: "password" gives the token's password (once to each ssh, and
/// only once at all for a token without an asker); "ask" asks the user
fn answer(token: &str, kind: &str, ssh_pid: u32, prompt: &str) -> String {
    let asker = {
        let mut tokens = TOKENS.lock().unwrap();
        let Some(entry) = tokens.as_mut().and_then(|t| t.get_mut(token)) else { return String::new() };
        if entry.asker.is_none() && entry.made.elapsed() >= LIFETIME {
            return String::new();
        }
        if kind == "password" {
            if entry.asker.is_none() {
                return entry.password.take().unwrap_or_default();
            }
            return match &entry.password {
                Some(password) if entry.given.insert(ssh_pid) => password.clone(),
                _ => String::new(),
            };
        }
        entry.asker.clone()
    };
    // asked outside the lock: the user may take a while
    let secret = !prompt.to_lowercase().contains("yes/no");
    asker.and_then(|ask| ask(prompt, secret)).unwrap_or_default()
}

/// Answer requests on the socket (once per run of the app): "token\nkind\nssh-pid\nprompt" (newlines in the prompt
/// as \x01)
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
                        // each on its own thread: a question can wait on the user
                        std::thread::spawn(move || {
                            let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
                            let mut request = String::new();
                            if BufReader::new(&stream).read_to_string(&mut request).is_err() {
                                return;
                            }
                            let mut parts = request.splitn(4, '\n');
                            let (token, kind) = (parts.next().unwrap_or(""), parts.next().unwrap_or("password"));
                            let pid = parts.next().and_then(|p| p.trim().parse().ok()).unwrap_or(0);
                            let prompt = parts.next().unwrap_or("");
                            let reply = answer(token.trim(), kind.trim(), pid, &prompt.replace('\x01', "\n"));
                            let mut stream = &stream;
                            let _ = stream.write_all(reply.as_bytes());
                        });
                    }
                })
                .map_err(|e| e.to_string())?;
            Ok(())
        })
        .clone()
}

/// Run as ssh's askpass (the token is set): print the answer to ssh's question and say so. The password question
/// is answered with the token's password the first time; anything else is asked in the terminal, or by the app.
#[cfg(unix)]
pub fn helper() -> bool {
    let Ok(token) = std::env::var(TOKEN_VAR) else { return false };
    let prompt = std::env::args().nth(1).unwrap_or_default();
    let lower = prompt.to_lowercase();
    let mut answer = None;
    if lower.contains("password") && !lower.contains("passphrase") {
        answer = fetch(&token, "password", &prompt).filter(|p| !p.is_empty());
    }
    let answer = answer
        .or_else(|| ask_in_terminal(&prompt, !lower.contains("yes/no")))
        .or_else(|| fetch(&token, "ask", &prompt))
        .unwrap_or_default();
    println!("{answer}");
    true
}

#[cfg(not(unix))]
pub fn helper() -> bool {
    false
}

/// Ask the running app
#[cfg(unix)]
fn fetch(token: &str, kind: &str, prompt: &str) -> Option<String> {
    let mut stream = std::os::unix::net::UnixStream::connect(socket_path()).ok()?;
    // a question waits on the user
    stream.set_read_timeout(Some(Duration::from_secs(if kind == "ask" { 600 } else { 5 }))).ok()?;
    // ssh is this helper's parent: each ssh gets the password once
    // SAFETY: getppid has no preconditions
    let ssh_pid = unsafe { libc::getppid() };
    stream.write_all(format!("{token}\n{kind}\n{ssh_pid}\n{}", prompt.replace('\n', "\x01")).as_bytes()).ok()?;
    stream.shutdown(std::net::Shutdown::Write).ok()?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply).ok()?;
    Some(reply)
}

/// Ask in the terminal, hiding what's typed for a secret; None when there's no terminal
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
