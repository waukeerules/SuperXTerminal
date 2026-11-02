//! How a stored session is started: the program, its arguments, and a password to type at the first prompt.
//!
//! SuperXPuTTY's `PuttyStartInfo` / `PlinkCommandLine` for the Mac: SSH goes through OpenSSH (`ssh -tt`), with the
//! session's folder defaults applied first (`FolderSettings.WithDefaults`) and PuTTY-style extra arguments translated
//! (docs/PLAN.md §2, SSH and other connections). cmd and WSL sessions open the user's login shell instead.
//!
//! A `-pw` password is never put on the command line or in the environment: the pseudo terminal types it once, at
//! the first password prompt (`pty.rs`). That also leaves host-key questions in the terminal for the user to answer.

use portable_pty::CommandBuilder;

use crate::sessions::{Sessions, Store, StoredSession};

/// A session ready to start
pub struct Launch {
    pub command: CommandBuilder,
    /// Typed once at the first "password:" prompt
    pub password: Option<String>,
    /// The tab should behave as a local shell (no reconnect, Ctrl+C as a signal)
    pub local: bool,
    /// Things the user should know, shown in the terminal before it starts
    pub notices: Vec<String>,
}

/// Algorithms older devices offer and OpenSSH 10 no longer accepts by default; added for each connection only.
/// They're the "Allow older algorithms" profile option (docs/PLAN.md §2 Profiles), on for every session until
/// profiles exist. Checked against an IOS lab switch.
const OLDER_ALGORITHMS: [&str; 8] = [
    "-o", "KexAlgorithms=+diffie-hellman-group14-sha1,diffie-hellman-group-exchange-sha1",
    "-o", "HostKeyAlgorithms=+ssh-rsa",
    "-o", "PubkeyAcceptedAlgorithms=+ssh-rsa",
    "-o", "Ciphers=+aes256-cbc,aes192-cbc,aes128-cbc",
];

/// Build the start of a stored session, or say why it can't start on this Mac yet
pub fn launch(sessions: &Sessions, store: Store, session: &StoredSession) -> Result<Launch, String> {
    match session.get("Proto").to_ascii_uppercase().as_str() {
        "SSH" | "" => ssh(sessions, store, session),
        "WINCMD" | "WSL" => {
            let mut launch = local_shell();
            launch.notices.push(format!("{} session opened as a Local Shell on this Mac.", session.get("Proto")));
            Ok(launch)
        }
        other => Err(format!("{other} sessions can't be opened yet.")),
    }
}

/// The user's login shell, as for the New Local Shell button
pub fn local_shell() -> Launch {
    Launch { command: CommandBuilder::new_default_prog(), password: None, local: true, notices: Vec::new() }
}

/// `ssh -tt` for an SSH session, with its folder defaults applied
fn ssh(sessions: &Sessions, store: Store, session: &StoredSession) -> Result<Launch, String> {
    let mut username = session.get("Username").to_string();
    let mut port = session.port();
    let mut jump_host = session.get("JumpHost").to_string();
    for folder in sessions.along(store, session.folder()) {
        if username.is_empty() {
            username = folder.username.clone();
        }
        if port == 0 {
            port = folder.port;
        }
        if jump_host.is_empty() {
            jump_host = folder.jump_host.clone();
        }
    }

    let host = session.get("Host").trim().to_string();
    if host.is_empty() {
        return Err("This session has no host.".into());
    }
    for (field, value) in [("Host", host.as_str()), ("Username", username.as_str())] {
        if value.contains('{') {
            return Err(format!("The {field} uses a {{placeholder}}; placeholders come later in M1."));
        }
    }

    let mut notices = Vec::new();
    let mut args: Vec<String> = vec!["-tt".into()];
    args.extend(OLDER_ALGORITHMS.iter().map(|s| s.to_string()));
    let extra = translate_extra_args(session.get("ExtraArgs"));
    notices.extend(extra.notices);
    if port != 0 && port != 22 {
        args.extend(["-p".into(), port.to_string()]);
    }
    if !username.is_empty() {
        args.extend(["-l".into(), username]);
    }
    if extra.password.is_some() {
        // try the password before any key: ssh would otherwise offer every key in ~/.ssh and the agent first, and
        // Cisco closes the connection after 3 failed attempts (ip ssh authentication-retries), before the password
        args.extend(["-o".into(), "PreferredAuthentications=keyboard-interactive,password,publickey".into()]);
    }
    if !jump_host.is_empty() {
        args.extend(["-J".into(), jump_address(sessions, store, &jump_host)?]);
    }
    args.extend(extra.args);
    args.push(host);

    let mut command = CommandBuilder::new("ssh");
    command.args(&args);
    Ok(Launch { command, password: extra.password, local: false, notices })
}

/// A jump host as `ssh -J` takes it: "user@host:port" as it is, or a saved session's id turned into its address
fn jump_address(sessions: &Sessions, store: Store, jump: &str) -> Result<String, String> {
    let Some(session) = sessions.find(store, jump).or_else(|| sessions.find(Store::Saved, jump)) else {
        return Ok(jump.to_string());
    };
    let mut address = session.get("Host").to_string();
    if !session.get("Username").is_empty() {
        address = format!("{}@{address}", session.get("Username"));
    }
    if session.port() != 0 && session.port() != 22 {
        address = format!("{address}:{}", session.port());
    }
    Ok(address)
}

/// PuTTY-style extra arguments, translated for `ssh`
struct Translated {
    args: Vec<String>,
    password: Option<String>,
    notices: Vec<String>,
}

/// Translate PuTTY/plink arguments into `ssh`'s where it has an equivalent (docs/PLAN.md §2); the rest are named
fn translate_extra_args(extra: &str) -> Translated {
    let mut out = Translated { args: Vec::new(), password: None, notices: Vec::new() };
    let words = split_words(extra);
    let mut skipped = Vec::new();
    let mut i = 0;
    while i < words.len() {
        let word = words[i].as_str();
        let value = words.get(i + 1).cloned();
        match word {
            "-pw" => {
                out.password = value;
                i += 1;
            }
            "-P" => {
                if let Some(v) = value {
                    out.args.extend(["-p".into(), v]);
                }
                i += 1;
            }
            "-l" | "-i" | "-L" | "-R" | "-D" => {
                if let Some(v) = value {
                    if word == "-i" && v.to_ascii_lowercase().ends_with(".ppk") {
                        out.notices.push(format!("{v} is a PuTTY key; convert it with: puttygen key.ppk -O private-openssh"));
                    } else {
                        out.args.extend([word.to_string(), v]);
                    }
                }
                i += 1;
            }
            "-A" | "-X" | "-C" | "-4" | "-6" | "-v" => out.args.push(word.to_string()),
            "-ssh" | "-2" | "-batch" | "-t" => {}
            "-load" => i += 1,
            other => skipped.push(other.to_string()),
        }
        i += 1;
    }
    if !skipped.is_empty() {
        out.notices.push(format!("Not used on this Mac: {}", skipped.join(" ")));
    }
    out
}

/// Split a command line into words; double quotes group words, as PuTTY's command line does
fn split_words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut any = false;
    for c in text.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                any = true;
            }
            c if c.is_whitespace() && !quoted => {
                if any {
                    words.push(std::mem::take(&mut current));
                    any = false;
                }
            }
            c => {
                current.push(c);
                any = true;
            }
        }
    }
    if any {
        words.push(current);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extra_args_keep_the_password_off_the_command_line() {
        let t = translate_extra_args("-pw secret -P 2222 -A -load profile -sercfg 9600");
        assert_eq!(t.password.as_deref(), Some("secret"));
        assert_eq!(t.args, ["-p", "2222", "-A"]);
        assert_eq!(t.notices, ["Not used on this Mac: -sercfg 9600"]);
    }

    #[test]
    fn quoted_words_stay_together() {
        assert_eq!(split_words(r#"-i "/Users/me/my key" -v"#), ["-i", "/Users/me/my key", "-v"]);
    }
}
