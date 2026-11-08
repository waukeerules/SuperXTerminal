//! 1Password secret references (`op://vault/item/field`) in place of stored secrets: an Environment Variable's value
//! or a password (a profile's, a session's own) can be one, and it's read from 1Password as a session opens, with its
//! own approval prompt (Windows Hello, Touch ID), rather than kept in Windows Credential Manager or the Keychain.
//!
//! Read by 1Password's CLI, `op read`, which signs in through the 1Password app ("Integrate with 1Password CLI" in its
//! Settings > Developer): there's no Rust SDK. A value read is kept in memory for [`KEPT_FOR`], so the tabs of one
//! sitting don't each ask again, and never written anywhere. A reference that can't be read says why, as a notice in
//! the tab ([`take_problems`]), and the session goes on without it: ssh asks for the password in the tab.
//!
//! Its SSH agent is separate: ssh reaches it by itself (Windows' OpenSSH agent pipe, which 1Password takes over), and
//! needs nothing here.
//!
//! All of it is off until turned on (Settings > General > 1Password, `AppSettings::one_password`), as most people
//! don't use 1Password: off, no reference is read and `op` never runs; a reference used says so in the tab
//! ([`TURNED_OFF`]) and the session goes on without it, as when one can't be read.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a value read stays in memory: about as long as 1Password's own CLI sign-in lasts while it's used
const KEPT_FOR: Duration = Duration::from_secs(10 * 60);

/// How long `op read` may wait for the user to approve it in 1Password before it's given up
#[cfg_attr(test, allow(dead_code))]
const APPROVE_WITHIN: Duration = Duration::from_secs(120);

/// Values read, by reference, with when; and what went wrong reading, for the tab opening now to say
struct Kept {
    values: HashMap<String, (String, Instant)>,
    problems: Vec<String>,
}

static KEPT: Mutex<Option<Kept>> = Mutex::new(None);

/// Whether 1Password is turned on (Settings > General > 1Password): set as the settings are read and saved. On in the
/// tests, whose references are other modules' to read; [`read_with`] is given it, so the off case is tested too
static ENABLED: AtomicBool = AtomicBool::new(cfg!(test));

/// What a reference used while 1Password is off says, in the tab and Check 1Password
pub const TURNED_OFF: &str = "1Password is turned off (Settings > General > 1Password)";

/// Turn 1Password on or off, as Settings > General has it
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// A value without the spaces and the quotes round it: 1Password's Copy Secret Reference puts a reference with a space
/// in it ("op://Private/Cisco Switch/password") in double quotes, for a shell
fn unquoted(value: &str) -> &str {
    let value = value.trim();
    for quote in ['"', '\''] {
        if let Some(inner) = value.strip_prefix(quote).and_then(|v| v.strip_suffix(quote)) {
            return inner.trim();
        }
    }
    value
}

/// Whether a value is a 1Password secret reference, to be read rather than used as it is, quoted or not
pub fn is_reference(value: &str) -> bool {
    unquoted(value).to_ascii_lowercase().starts_with("op://")
}

/// A value as it's used: a secret reference read from 1Password, anything else as it is. A reference that can't be
/// read gives "" and a problem for [`take_problems`]
pub fn resolved(value: &str) -> String {
    if !is_reference(value) {
        return value.to_string();
    }
    match read(unquoted(value)) {
        Ok(secret) => secret,
        Err(problem) => {
            with_kept(|kept| kept.problems.push(problem));
            String::new()
        }
    }
}

/// The reference to an SSH Key item's public key, from one to any of its fields ("op://Private/Lab Server key/private key",
/// with or without quotes or ?ssh-format=...), or to the item itself ("op://Private/Lab Server key")
fn public_key_reference(reference: &str) -> Option<String> {
    let reference = unquoted(reference);
    let path = reference.get(5..)?.split('?').next()?.trim_end_matches('/');
    let parts: Vec<&str> = path.split('/').collect();
    // the vault and item, whatever field (or none) follows
    (parts.len() >= 2 && !parts[1].is_empty()).then(|| format!("op://{}/{}/public key", parts[0], parts[1]))
}

/// A key in 1Password, named as a profile's Key file by a reference to its SSH Key item: its public key, read from
/// 1Password and written to a file, for ssh's -i with IdentitiesOnly, so ssh asks 1Password's SSH agent for that key
/// alone. Its private key never leaves 1Password; the public one isn't secret. Also the item's name, for the tab
pub fn public_key_file(reference: &str) -> Result<(String, String), String> {
    let public = public_key_reference(reference).ok_or_else(|| format!("{} isn't a 1Password reference to an SSH Key item.", unquoted(reference)))?;
    let key = read(&public)?;
    let key = key.trim();
    if !(key.starts_with("ssh-") || key.starts_with("ecdsa-") || key.starts_with("sk-")) {
        return Err(format!("{public} isn't an SSH public key: is {} an SSH Key item?", unquoted(reference)));
    }
    let item = public.trim_start_matches("op://").trim_end_matches("/public key").to_string();
    let folder = std::env::temp_dir().join("SuperXTerminal-1Password");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let name: String = item.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' }).collect();
    let file = folder.join(format!("{name}.pub"));
    if std::fs::read_to_string(&file).ok().as_deref() != Some(key) {
        std::fs::write(&file, key).map_err(|e| e.to_string())?;
    }
    Ok((file.display().to_string(), item))
}

/// 1Password's SSH agent on the Mac, for ssh's IdentityAgent: its short link (~/.1password/agent.sock) when it's
/// there, else its socket in its Group Container, quoted for its space. None when neither is (1Password isn't
/// installed, or its agent is off), and on Windows, where it takes over the OpenSSH agent's pipe that ssh uses anyway
pub fn mac_agent(home: &std::path::Path) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let link = home.join(".1password/agent.sock");
    let socket = home.join("Library/Group Containers/2BUA8C4S2C.com.1password/t/agent.sock");
    if link.exists() {
        Some(link.display().to_string())
    } else if socket.exists() {
        Some(format!("\"{}\"", socket.display()))
    } else {
        None
    }
}

/// A value as it's used, saying why when it's a reference that can't be read (where [`resolved`] gives "" and leaves
/// the reason for the tab): for a Rhai script's `env.name`, which stops the script with it
pub fn value(value: &str) -> Result<String, String> {
    if is_reference(value) {
        read(unquoted(value))
    } else {
        Ok(value.to_string())
    }
}

/// What went wrong reading references since this was last asked, each once: for the tab opening now to say
pub fn take_problems() -> Vec<String> {
    with_kept(|kept| {
        let mut seen = Vec::new();
        for problem in kept.problems.drain(..) {
            if !seen.contains(&problem) {
                seen.push(problem);
            }
        }
        seen
    })
}

/// A secret reference's value: kept from a read in the last [`KEPT_FOR`], else read by `op read`
#[cfg(not(test))]
pub fn read(reference: &str) -> Result<String, String> {
    read_with(reference, ENABLED.load(Ordering::Relaxed), run_op)
}

/// In the tests, without 1Password: what [`keep`] put there, else as if op weren't installed
#[cfg(test)]
pub fn read(reference: &str) -> Result<String, String> {
    read_with(reference, ENABLED.load(Ordering::Relaxed), |r| Err(format!("{r} wasn't read: no op in the tests.")))
}

/// A value as if read from 1Password, for other modules' tests
#[cfg(test)]
pub fn keep(reference: &str, value: &str) {
    with_kept(|kept| kept.values.insert(reference.to_string(), (value.to_string(), Instant::now())));
}

/// [`read`], with whether 1Password is on and how `op read` is run given (the tests' stand-ins). Off, nothing is read,
/// not even a value kept from before it was turned off, and op isn't run
fn read_with(reference: &str, enabled: bool, run: impl Fn(&str) -> Result<String, String>) -> Result<String, String> {
    if !enabled {
        return Err(format!("{reference} wasn't read: {TURNED_OFF}."));
    }
    if let Some(value) = with_kept(|kept| kept.values.get(reference).filter(|(_, at)| at.elapsed() < KEPT_FOR).map(|(v, _)| v.clone())) {
        return Ok(value);
    }
    let value = run(reference)?;
    with_kept(|kept| kept.values.insert(reference.to_string(), (value.clone(), Instant::now())));
    Ok(value)
}

/// Forget every value read, so the next use reads again: called when Environment Variables or a profile's password
/// are saved (a reference changed, or the secret behind it)
#[cfg(not(test))]
pub fn forget() {
    with_kept(|kept| kept.values.clear());
}

/// In the tests, nothing: they run together, and one saving a profile's password would take the values others keep
#[cfg(test)]
pub fn forget() {}

/// Work on what's kept, made the first time
fn with_kept<T>(work: impl FnOnce(&mut Kept) -> T) -> T {
    let mut guard = KEPT.lock().unwrap();
    work(guard.get_or_insert_with(|| Kept { values: HashMap::new(), problems: Vec::new() }))
}

/// How to install 1Password's CLI on this system, as its getting-started page says
pub const INSTALL: &str = if cfg!(windows) { "winget install 1password-cli" } else { "brew install 1password-cli" };

/// 1Password's CLI getting-started page, for installing it and turning on its link to the 1Password app
pub const GET_STARTED: &str = "https://www.1password.dev/cli/get-started";

/// Where 1Password's CLI is: on the PATH, else where winget or Homebrew put it. The app's PATH is the one it started
/// with (a CLI installed since isn't on it until the app starts afresh from a new shell), and a Mac app never gets
/// the shell's PATH, so the usual places are looked in too
fn op_program() -> Option<std::path::PathBuf> {
    let name = if cfg!(windows) { "op.exe" } else { "op" };
    let on_path = std::env::var_os("PATH").into_iter().flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>()).map(|d| d.join(name));
    let mut usual: Vec<std::path::PathBuf> = Vec::new();
    if cfg!(windows) {
        if let Some(local) = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from) {
            usual.push(local.join(r"Microsoft\WinGet\Links\op.exe"));
            // winget's own folder for it, its name ending in the source's id
            if let Ok(found) = std::fs::read_dir(local.join(r"Microsoft\WinGet\Packages")) {
                usual.extend(found.flatten().filter(|e| e.file_name().to_string_lossy().starts_with("AgileBits.1Password.CLI")).map(|e| e.path().join("op.exe")));
            }
        }
        usual.push(std::path::PathBuf::from(r"C:\Program Files\1Password CLI\op.exe"));
    } else {
        usual.extend(["/opt/homebrew/bin/op", "/usr/local/bin/op"].map(std::path::PathBuf::from));
    }
    on_path.chain(usual).find(|p| p.is_file())
}

/// 1Password's CLI, ready to run with its window hidden on Windows and nothing to read from; or why it can't be
fn op_command() -> Result<std::process::Command, String> {
    let program = op_program().ok_or_else(|| format!("1Password's CLI (op) isn't installed: {INSTALL}, then turn on Integrate with 1Password CLI in 1Password's Settings > Developer ({GET_STARTED})."))?;
    let mut command = std::process::Command::new(program);
    command.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // CREATE_NO_WINDOW: no console flashing up for it
        command.creation_flags(0x0800_0000);
    }
    Ok(command)
}

/// Whether 1Password's CLI works here, for Settings > General's Check 1Password: its version, and its vaults, which
/// asks 1Password to approve it as a session's reference would. What's wrong, in the user's words, otherwise
pub fn check() -> Result<String, String> {
    if !ENABLED.load(Ordering::Relaxed) {
        return Err(format!("{TURNED_OFF}."));
    }
    let mut version = op_command()?;
    let version = version.arg("--version").output().map_err(|e| format!("op didn't start ({e})."))?;
    let version = String::from_utf8_lossy(&version.stdout).trim().to_string();
    let mut vaults = op_command()?;
    vaults.args(["vault", "list", "--format=json"]);
    let output = finish(vaults.spawn().map_err(|e| format!("op didn't start ({e})."))?, "1Password")?;
    if !output.status.success() {
        return Err(explain("1Password", &String::from_utf8_lossy(&output.stderr)));
    }
    let count = serde_json::from_slice::<Vec<serde_json::Value>>(&output.stdout).map(|v| v.len()).unwrap_or(0);
    Ok(format!("1Password's CLI {version} works, through the 1Password app: {count} vault{} it can read.", if count == 1 { "" } else { "s" }))
}

/// A started op's output, giving up after [`APPROVE_WITHIN`] as it may be waiting for the user's approval
#[cfg_attr(test, allow(dead_code))]
fn finish(child: std::process::Child, what: &str) -> Result<std::process::Output, String> {
    let (sent, got) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sent.send(child.wait_with_output());
    });
    match got.recv_timeout(APPROVE_WITHIN) {
        Ok(output) => output.map_err(|e| format!("{what} wasn't read: {e}.")),
        Err(_) => Err(format!("{what} wasn't read: 1Password wasn't approved within {} seconds.", APPROVE_WITHIN.as_secs())),
    }
}

/// Run `op read --no-newline <reference>`, giving up after [`APPROVE_WITHIN`]
#[cfg_attr(test, allow(dead_code))]
fn run_op(reference: &str) -> Result<String, String> {
    let mut command = op_command().map_err(|why| format!("{reference} wasn't read: {why}"))?;
    command.args(["read", "--no-newline", reference]);
    let child = command.spawn().map_err(|e| format!("{reference} wasn't read: op didn't start ({e})."))?;
    let output = finish(child, reference)?;
    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).to_string());
    }
    Err(explain(reference, &String::from_utf8_lossy(&output.stderr)))
}

/// What to say for `op read`'s error: the usual causes in the user's words, else op's own message
fn explain(reference: &str, stderr: &str) -> String {
    let said = stderr.trim().trim_start_matches("[ERROR]").trim();
    // op's message starts with a date and time: "2026/11/07 19:40:01 isn't an item..."
    let said = said.splitn(3, ' ').nth(2).filter(|_| said.chars().next().is_some_and(|c| c.is_ascii_digit())).unwrap_or(said);
    let lower = said.to_lowercase();
    if lower.contains("not signed in") || lower.contains("no accounts") || lower.contains("account is not") {
        format!("{reference} wasn't read: op isn't signed in. Turn on Integrate with 1Password CLI in 1Password's Settings > Developer, and keep 1Password running.")
    } else if lower.contains("dismissed") || lower.contains("cancel") || lower.contains("denied") {
        format!("{reference} wasn't read: the request was turned down in 1Password.")
    } else {
        format!("{reference} wasn't read: {said}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn a_reference_is_told_from_a_plain_value() {
        assert!(is_reference("op://Lab/Core switch/password"));
        assert!(is_reference("  OP://lab/x/y"));
        assert!(!is_reference("cisco123"));
        assert!(!is_reference("see op://Lab/x/y"));
        // as 1Password's Copy Secret Reference gives one with a space in it, in quotes; or in single quotes
        assert!(is_reference("\"op://Private/Cisco Switch/password\""));
        assert!(is_reference(" 'op://Private/Cisco Switch/password' "));
        assert_eq!(unquoted("\"op://Private/Cisco Switch/password\""), "op://Private/Cisco Switch/password");
        // a password that only has a quote in it isn't touched
        assert_eq!(unquoted("pa\"ss"), "pa\"ss");
        assert!(!is_reference("\"cisco123\""));
        // a plain value is used as it is, without 1Password
        assert_eq!(resolved("cisco123"), "cisco123");
    }

    #[test]
    fn a_value_read_is_kept_for_a_while_so_tabs_together_ask_once() {
        let runs = Cell::new(0);
        let run = |_: &str| {
            runs.set(runs.get() + 1);
            Ok("s3cret".to_string())
        };
        let reference = "op://Lab/kept-test/password";
        assert_eq!(read_with(reference, true, run), Ok("s3cret".into()));
        assert_eq!(read_with(reference, true, run), Ok("s3cret".into()));
        assert_eq!(runs.get(), 1);
        // (forget(), which clears them all, isn't tried here: the tests run together, and others keep values too)
        // one that fails isn't kept: it's tried again next time
        let failing = "op://Lab/failing-test/password";
        assert!(read_with(failing, true, |_| Err("nope".into())).is_err());
        assert_eq!(read_with(failing, true, |_| Ok("now".into())), Ok("now".into()));
    }

    #[test]
    fn turned_off_nothing_is_read_and_op_never_runs_and_the_tab_is_told_why() {
        let runs = Cell::new(0);
        let run = |_: &str| {
            runs.set(runs.get() + 1);
            Ok("s3cret".to_string())
        };
        let reference = "op://Lab/off-test/password";
        // a value kept from while it was on isn't used either
        assert_eq!(read_with(reference, true, run), Ok("s3cret".into()));
        let off = read_with(reference, false, run);
        assert_eq!(off, Err("op://Lab/off-test/password wasn't read: 1Password is turned off (Settings > General > 1Password).".into()));
        assert_eq!(runs.get(), 1, "op ran while 1Password was off");
        // and the settings' default is off
        assert!(!crate::settings::AppSettings::default().one_password);
    }

    #[test]
    fn a_key_file_reference_becomes_its_items_public_key_for_the_agent() {
        // any of the item's fields, quoted as 1Password copies it, or the item itself
        for given in ["\"op://Private/Lab Server key/private key\"", "op://Private/Lab Server key/private key?ssh-format=openssh", "op://Private/Lab Server key"] {
            assert_eq!(public_key_reference(given).as_deref(), Some("op://Private/Lab Server key/public key"), "{given}");
        }
        assert_eq!(public_key_reference("op://Private"), None);
        keep("op://Private/Lab Server key/public key", "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample lab\n");
        let (file, item) = public_key_file("\"op://Private/Lab Server key/private key\"").unwrap();
        assert_eq!(item, "Private/Lab Server key");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIExample lab");
        // a field that isn't an SSH key's
        keep("op://Private/Not a key/public key", "hello");
        assert!(public_key_file("op://Private/Not a key/password").unwrap_err().contains("isn't an SSH public key"));
    }

    /// The Mac's agent, for a key in 1Password: its short link first, else its socket, quoted; none on Windows
    #[test]
    fn the_macs_1password_agent_is_its_link_else_its_socket() {
        let home = std::env::temp_dir().join(format!("sxt-op-agent-{}", std::process::id()));
        let socket = home.join("Library/Group Containers/2BUA8C4S2C.com.1password/t");
        std::fs::create_dir_all(&socket).unwrap();
        assert_eq!(mac_agent(&home), None);
        std::fs::write(socket.join("agent.sock"), "").unwrap();
        if cfg!(windows) {
            assert_eq!(mac_agent(&home), None);
        } else {
            assert_eq!(mac_agent(&home), Some(format!("\"{}\"", socket.join("agent.sock").display())));
            std::fs::create_dir_all(home.join(".1password")).unwrap();
            std::fs::write(home.join(".1password/agent.sock"), "").unwrap();
            assert_eq!(mac_agent(&home), Some(home.join(".1password/agent.sock").display().to_string()));
        }
        std::fs::remove_dir_all(&home).unwrap();
    }

    #[test]
    fn ops_errors_are_said_in_the_users_words() {
        let r = "op://Lab/x/password";
        assert_eq!(
            explain(r, "[ERROR] 2026/11/07 19:40:01 account is not signed in"),
            "op://Lab/x/password wasn't read: op isn't signed in. Turn on Integrate with 1Password CLI in 1Password's Settings > Developer, and keep 1Password running."
        );
        assert_eq!(explain(r, "[ERROR] 2026/11/07 19:40:01 authorization prompt dismissed, please try again"), "op://Lab/x/password wasn't read: the request was turned down in 1Password.");
        assert_eq!(explain(r, "[ERROR] 2026/11/07 19:40:01 \"x\" isn't an item in the \"Lab\" vault."), "op://Lab/x/password wasn't read: \"x\" isn't an item in the \"Lab\" vault.");
    }
}
