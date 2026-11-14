//! Scripts: the login script a session or a Quick Connect runs as its tab opens (its own, else its profile's), the
//! Script Editor's files in the shared `scripts` folder, and where a script's saved output goes. The folder and
//! its files are `crate::scripts`; running a Rhai script is `crate::rhai_scripts`.

use std::path::PathBuf;

use tauri::State;

use crate::profiles::{self, Profiles};
use crate::{launch, overrides, scripts, sessions, shared_folder, AppState};

/// The script a session runs as it opens (Edit Session's login script, else its profile's), found as SuperXPuTTY finds
/// it: SPSL (#!/bin/spsl) or Rhai (#!/bin/rhai, or a .rhai file), which the window tells apart
#[derive(serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub(crate) enum SessionScript {
    /// The session has none, nor has its profile
    None,
    /// An http(s) address, which the window fetches
    Url { url: String, profile: Option<String> },
    /// The script's text, and where it's from as written (`source`: a path), whose .rhai says it's a Rhai script
    Text { text: String, profile: Option<String>, source: String },
    /// A file that isn't there, e.g. a Windows path (C:\...) on the Mac. `looked` is where a relative one was looked
    /// for, in this computer's settings folder, so the tab can say where to save a copy of its own
    Missing { path: String, looked: Option<String>, profile: Option<String> },
}

/// Which script a session runs as it opens, and the profile it comes from (None for the session's own): the session's
/// SPSL script, else its profile's (the session's, else its folders', as a launch picks it; local overrides
/// included), else Default Settings'
pub(crate) fn script_spec(sessions: &sessions::Sessions, profiles: &Profiles, store: sessions::Store, session: &sessions::StoredSession) -> Option<(String, Option<String>)> {
    let own = session.get("SPSLFileName").trim();
    if !own.is_empty() {
        return Some((own.to_string(), None));
    }
    let mut profile_name = session.get("PuttySession").to_string();
    for folder in sessions.along(store, session.folder()) {
        if profiles::is_default(&profile_name) && !folder.putty_profile.is_empty() {
            profile_name = folder.putty_profile.clone();
        }
    }
    profiles::script_of(profiles, &profile_name).map(|(spec, name)| (spec.to_string(), Some(name.to_string())))
}

/// The script a session runs when it opens: a file (absolute, file://, or relative to the settings folder), or a URL
#[tauri::command]
pub(crate) fn session_script(state: State<'_, AppState>, store: sessions::Store, id: String) -> SessionScript {
    // each lock taken and let go on its own: std's Mutex isn't reentrant
    let overrides_on = state.settings.lock().unwrap().use_local_overrides;
    let folder = shared_folder(&state);
    let profiles = state.profiles.lock().unwrap().clone();
    let found = {
        let sessions = state.sessions.lock().unwrap();
        sessions
            .find(store, &id)
            .map(|s| overrides::applied(&state.user_folder, s, overrides_on))
            .and_then(|s| script_spec(&sessions, &profiles, store, &s))
    };
    let Some((spec, profile)) = found else { return SessionScript::None };
    read_script(&spec, profile, &folder)
}

/// What a Quick connect tab's script needs: the script its profile runs, and the values the typed fields give it in
/// place of a saved session's ({host_address}, {session_username}...), so a jump server profile's script (Log in here)
/// goes on to the host typed
#[derive(serde::Serialize)]
pub(crate) struct QuickScript {
    script: SessionScript,
    values: std::collections::BTreeMap<String, String>,
}

/// The script a Quick connect tab runs as it opens (its profile's, else Default Settings'), and the typed fields as
/// the session's values, parsed as the launch parses them ("user@host:port")
#[tauri::command]
pub(crate) fn quick_connect_script(state: State<'_, AppState>, quick: launch::QuickConnect) -> Result<QuickScript, String> {
    let session = quick.session()?;
    let folder = shared_folder(&state);
    let profiles = state.profiles.lock().unwrap().clone();
    // a one-off session is in no folder, so no folder's profile applies
    let script = match script_spec(&sessions::Sessions::default(), &profiles, sessions::Store::Saved, &session) {
        Some((spec, profile)) => read_script(&spec, profile, &folder),
        None => SessionScript::None,
    };
    Ok(QuickScript { script, values: quick_values(&session) })
}

/// A Quick connect session's values for its scripts, as a saved session's are (sending.svelte.ts's sessionValues):
/// its host, port (the protocol's usual one when none is typed), username, profile and protocol, named by its host
pub(crate) fn quick_values(session: &sessions::StoredSession) -> std::collections::BTreeMap<String, String> {
    let protocol = session.get("Proto").to_string();
    let port = match session.port() {
        0 => match protocol.to_ascii_uppercase().as_str() {
            "" | "SSH" => "22".to_string(),
            "TELNET" => "23".to_string(),
            _ => String::new(),
        },
        port => port.to_string(),
    };
    [
        ("host_address", session.get("Host").to_string()),
        ("host_port", port),
        ("session_name", session.get("Host").to_string()),
        ("session_username", session.get("Username").to_string()),
        ("session_profile", session.get("PuttySession").to_string()),
        ("session_protocol", protocol),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_string(), v))
    .collect()
}

/// A script's text from where it's named: an http(s) address is left for the window to fetch; a file is absolute,
/// file://, or relative to the settings folder
pub(crate) fn read_script(spec: &str, profile: Option<String>, folder: &std::path::Path) -> SessionScript {
    let lower = spec.to_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        return SessionScript::Url { url: spec.to_string(), profile };
    }
    let path = PathBuf::from(spec.strip_prefix("file://").unwrap_or(spec));
    // a relative one typed on Windows ("scripts\login.spsl") is found on the Mac too, where \ is a letter in a name
    let relative = !path.is_absolute();
    let path = if relative { folder.join(spec.replace('\\', "/")) } else { path };
    match std::fs::read_to_string(&path) {
        Ok(text) => SessionScript::Text { text, profile, source: spec.to_string() },
        Err(_) => SessionScript::Missing { path: spec.to_string(), looked: relative.then(|| path.display().to_string()), profile },
    }
}

/// A login script file picked with Choose… (Edit Session, Settings > Profiles) as the session or profile keeps it:
/// relative to the settings folder when it's in there, so the Mac and Windows both find it (scripts::as_spec)
#[tauri::command]
pub(crate) fn script_chosen(state: State<'_, AppState>, path: String) -> String {
    scripts::as_spec(&shared_folder(&state), std::path::Path::new(&path))
}

/// Copy a login script that's a full path into the settings folder's scripts folder, for the name a shared session
/// or profile can keep (scripts::copy_in); a different file of that name there is replaced only with `replace`
#[tauri::command]
pub(crate) fn script_copy_in(state: State<'_, AppState>, path: String, replace: bool) -> Result<scripts::CopiedIn, String> {
    let path = path.trim();
    scripts::copy_in(&shared_folder(&state), std::path::Path::new(path.strip_prefix("file://").unwrap_or(path)), replace)
}

/// The saved scripts
#[tauri::command]
pub(crate) fn scripts_list(state: State<'_, AppState>) -> Vec<scripts::Entry> {
    scripts::list(&shared_folder(&state))
}

/// A saved script's text
#[tauri::command]
pub(crate) fn script_load(state: State<'_, AppState>, name: String, kind: String) -> Result<String, String> {
    scripts::load(&shared_folder(&state), &name, &kind)
}

/// Save a script in the library
#[tauri::command]
pub(crate) fn script_save(state: State<'_, AppState>, name: String, kind: String, text: String) -> Result<Vec<scripts::Entry>, String> {
    scripts::save(&shared_folder(&state), &name, &kind, &text)?;
    Ok(scripts::list(&shared_folder(&state)))
}

/// Delete a saved script
#[tauri::command]
pub(crate) fn script_delete(state: State<'_, AppState>, name: String, kind: String) -> Result<Vec<scripts::Entry>, String> {
    scripts::delete(&shared_folder(&state), &name, &kind)?;
    Ok(scripts::list(&shared_folder(&state)))
}

/// Where a script's SAVE writes: a full path as it is, one in the home folder (`~/…`, or on Windows `~\…` and
/// `%USERPROFILE%\…`, as launch.rs's expand_home reads them) there, anything else in `output`
/// (Documents/SuperXTerminal/output, beside the session logs' own folder)
pub(crate) fn script_output_path(path: &str, home: &std::path::Path, output: &std::path::Path) -> PathBuf {
    let path = path.trim();
    let expanded = launch::expand_home_in(path, Some(home));
    if expanded != path {
        return PathBuf::from(expanded);
    }
    let given = PathBuf::from(path);
    if given.is_absolute() { given } else { output.join(given) }
}

/// A script's SAVE: write (or with `append`, add to) a text file, making its folder; its full path, for messages.
/// Lines end as this system's do (CRLF on Windows), as the output came with LF
#[tauri::command]
pub(crate) fn script_output_save(path: String, text: String, append: bool) -> Result<PathBuf, String> {
    let home = dirs::home_dir().unwrap_or_else(std::env::temp_dir);
    let output = dirs::document_dir().unwrap_or_else(|| home.clone()).join("SuperXTerminal").join("output");
    let file = script_output_path(&path, &home, &output);
    if let Some(folder) = file.parent() {
        std::fs::create_dir_all(folder).map_err(|e| format!("{}: {e}", folder.display()))?;
    }
    let text = if cfg!(windows) { text.replace("\r\n", "\n").replace('\n', "\r\n") } else { text };
    use std::io::Write;
    let mut handle = std::fs::OpenOptions::new()
        .create(true)
        .write(true)
        .append(append)
        .truncate(!append)
        .open(&file)
        .map_err(|e| format!("{}: {e}", file.display()))?;
    handle.write_all(text.as_bytes()).map_err(|e| format!("{}: {e}", file.display()))?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_session_with_no_script_runs_its_profiles_found_as_a_launch_finds_the_profile() {
        use crate::sessions::{FolderSetting, Sessions, Store, StoredSession};
        let profiles: crate::profiles::Profiles = serde_json::from_str(
            r#"{"profiles":[{"name":"Default Settings"},{"name":"Linux","script":"linux.spsl"},{"name":"Cisco","script":"cisco.spsl"}]}"#,
        )
        .unwrap();
        let mut sessions = Sessions::default();
        let folder = FolderSetting { path: "Lab".into(), putty_profile: "Cisco".into(), ..FolderSetting::default() };
        sessions.folders.entry(Store::Saved).or_default().insert("lab".into(), folder);
        let at = |attributes: &[(&str, &str)]| {
            let mut all = vec![("SessionId".to_string(), "Lab/web".to_string()), ("Host".to_string(), "192.168.0.10".to_string())];
            all.extend(attributes.iter().map(|(n, v)| (n.to_string(), v.to_string())));
            StoredSession { attributes: all }
        };
        let spec = |session: &StoredSession| super::script_spec(&sessions, &profiles, Store::Saved, session);
        // just a name and a host: its profile's script
        assert_eq!(spec(&at(&[("PuttySession", "Linux")])), Some(("linux.spsl".into(), Some("Linux".into()))));
        // no profile of its own: the folder's profile's
        assert_eq!(spec(&at(&[("PuttySession", "Default Settings")])), Some(("cisco.spsl".into(), Some("Cisco".into()))));
        // its own script wins
        assert_eq!(spec(&at(&[("PuttySession", "Linux"), ("SPSLFileName", "own.spsl")])), Some(("own.spsl".into(), None)));
    }

    #[test]
    fn a_quick_connects_typed_fields_are_its_scripts_session_values() {
        let quick = crate::launch::QuickConnect {
            protocol: "SSH".into(),
            host: "admin@192.168.0.50".into(),
            username: String::new(),
            password: "never shown".into(),
            profile: "Jump Host".into(),
        };
        let values = super::quick_values(&quick.session().unwrap());
        let get = |k: &str| values.get(k).map(String::as_str).unwrap_or("?");
        assert_eq!(
            [get("host_address"), get("host_port"), get("session_name"), get("session_username"), get("session_profile"), get("session_protocol")],
            ["192.168.0.50", "22", "192.168.0.50", "admin", "Jump Host", "SSH"]
        );
        // the password typed stays out of the script's values
        assert!(!values.values().any(|v| v.contains("never shown")));
        let typed_port = crate::launch::QuickConnect { host: "sw1:2222".into(), username: "me".into(), ..quick };
        let values = super::quick_values(&typed_port.session().unwrap());
        assert_eq!((values["host_address"].as_str(), values["host_port"].as_str(), values["session_username"].as_str()), ("sw1", "2222", "me"));
    }

    #[test]
    fn a_scripts_save_goes_in_the_output_folder_unless_its_path_says_otherwise() {
        use std::path::Path;
        let (home, output) = (Path::new("/home/me"), Path::new("/home/me/Documents/SuperXTerminal/output"));
        assert_eq!(super::script_output_path("sw1-run.txt", home, output), output.join("sw1-run.txt"));
        assert_eq!(super::script_output_path("lab/sw1.txt", home, output), output.join("lab/sw1.txt"));
        assert_eq!(super::script_output_path("~/Desktop/run.txt", home, output), home.join("Desktop/run.txt"));
        // Windows' own ways of writing the home folder too, as a key file's
        assert_eq!(super::script_output_path("%USERPROFILE%\\Desktop\\run.txt", home, output), home.join("Desktop\\run.txt"));
        assert_eq!(super::script_output_path("~\\Desktop\\run.txt", home, output), home.join("Desktop\\run.txt"));
        let full = if cfg!(windows) { "C:\\backups\\run.txt" } else { "/tmp/backups/run.txt" };
        assert_eq!(super::script_output_path(full, home, output), Path::new(full));
    }

    /// A session or profile shared between the Mac and Windows names its login script relative to the settings folder;
    /// one typed on Windows, with backslashes, is found on the Mac as well, where \ would be a letter in the name
    #[test]
    fn a_relative_login_script_is_found_with_either_systems_slashes() {
        let dir = std::env::temp_dir().join(format!("sxt-read-script-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("scripts")).unwrap();
        std::fs::write(dir.join("scripts").join("login.spsl"), "SENDLINE x").unwrap();
        for spec in ["scripts/login.spsl", r"scripts\login.spsl"] {
            assert!(matches!(super::read_script(spec, None, &dir), super::SessionScript::Text { ref text, .. } if text == "SENDLINE x"), "{spec}");
        }
        // where a copy of its own would go, for the tab's warning
        let looked = dir.join("scripts/other.spsl").display().to_string();
        assert!(matches!(super::read_script("scripts/other.spsl", None, &dir), super::SessionScript::Missing { looked: Some(ref l), .. } if *l == looked));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
