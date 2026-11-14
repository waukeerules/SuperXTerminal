//! Opening a session in another app instead of a tab: Remote Desktop and VNC in the system's own clients, Connect
//! external's ssh command in a Terminal window, and FileZilla. The session is worked out as a tab would connect
//! it (its folders' username and port, its profile's login, local overrides), then handed to `crate::external`,
//! which knows each app's command line.

use tauri::State;

use crate::{askpass, external, launch, overrides, sessions, variables, AppState};

/// Open a session in another app: "open" for RDP (Windows App) and VNC (Screen Sharing), "terminal" for Connect
/// external (its ssh command in a Terminal window), "filezilla" for FileZilla; returns what to say in the status bar
// async: a 1Password reference in its values waits for the user's approval, which mustn't hold up the window
#[tauri::command(async)]
pub(crate) fn session_external(state: State<'_, AppState>, store: sessions::Store, id: String, how: String) -> Result<String, String> {
    let profiles = state.profiles.lock().unwrap().clone();
    let values = variables::values(&state.user_folder);
    let (serial_devices, overrides_on) = {
        let settings = state.settings.lock().unwrap();
        (settings.serial_devices.clone(), settings.use_local_overrides)
    };
    let sessions = state.sessions.lock().unwrap();
    let session = &overrides::applied(&state.user_folder, sessions.find(store, &id).ok_or_else(|| format!("The session {id} is gone."))?, overrides_on);
    // the folders' username and port, as for a tab
    let mut username = session.get("Username").to_string();
    let mut port = session.port();
    for folder in sessions.along(store, session.folder()) {
        if username.is_empty() {
            username = folder.username.clone();
        }
        if port == 0 {
            port = folder.port;
        }
    }
    let host = variables::substitute(session.get("Host").trim(), &values);
    let username = variables::substitute(&username, &values);
    match (how.as_str(), session.get("Proto").to_ascii_uppercase().as_str()) {
        ("open", "RDP") => external::open_rdp(session, &host, &username, &state.settings.lock().unwrap().rdp_defaults.clone()),
        ("open", "VNC") => external::open_vnc(&host, port, &username),
        ("filezilla", _) => {
            // an SSH session's login as it connects in a tab (session, folders, profile); others as stored
            let launch = launch::launch(&sessions, &profiles, &values, &serial_devices, store, session).ok();
            let login = launch.as_ref().and_then(|l| l.login.clone());
            let (host, port, username, key_file) = match &login {
                Some(l) => (l.host.clone(), l.port, l.username.clone(), l.key_file.clone()),
                None => (host, port, username, String::new()),
            };
            // no username anywhere: the one ssh would use, as a tab connects with it
            let username = if username.is_empty() { external::ssh_username(&host, port).unwrap_or_default() } else { username };
            let password = if state.settings.lock().unwrap().filezilla_passwords { launch.and_then(|l| l.password) } else { None };
            let local = variables::substitute(session.get("LocalPath"), &values);
            let remote = variables::substitute(session.get("RemotePath"), &values);
            let opened = external::open_filezilla(&host, port, &username, password.as_deref(), &local, &remote)?;
            // FileZilla takes no key on its command line: it uses ssh-agent and its own list
            Ok(if key_file.is_empty() {
                opened
            } else {
                format!("{opened}. Its key {key_file} isn't passed to FileZilla: add it in FileZilla's Settings > SFTP, or run ssh-add {key_file}")
            })
        }
        ("terminal", _) => {
            let launch = launch::launch(&sessions, &profiles, &values, &serial_devices, store, session)?;
            if launch.stream.is_some() || launch.local {
                return Err("Only SSH sessions open in Terminal.".into());
            }
            let words: Vec<String> = launch.command.get_argv().iter().map(|w| w.to_string_lossy().to_string()).collect();
            let mut line = external::shell_line(&words);
            // the password, if any, goes to ssh through this app (askpass.rs), never on the command line
            if let Some(password) = launch.password.as_deref() {
                let program = std::env::current_exe().map_err(|e| e.to_string())?.to_string_lossy().to_string();
                let token = askpass::register(password)?;
                line = format!(
                    "SSH_ASKPASS={} SSH_ASKPASS_REQUIRE=force {}={token} {line}",
                    external::shell_line(&[program]),
                    askpass::TOKEN_VAR
                );
            }
            external::open_in_terminal(&line)?;
            Ok(format!("Opened {} in Terminal", session.get("SessionName")))
        }
        (_, proto) => Err(format!("{proto} sessions don't open in another app.")),
    }
}
