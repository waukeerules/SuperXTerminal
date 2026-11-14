//! Environment Variables (Settings > Environment Variables): listing and saving them, the built-in ones, where
//! each is used (sessions, profiles, button bars), which profiles name one that isn't set, and filling them into
//! a text. The variables and their secrets are `crate::variables`.

use std::collections::BTreeMap;

use tauri::State;

use crate::{overrides, profiles, variables, AppState};

/// The user's Environment Variables for the window; secrets' values stay in the Keychain
#[tauri::command]
pub(crate) fn variables_list(state: State<'_, AppState>) -> Vec<variables::Variable> {
    variables::list(&state.user_folder)
}

/// Where each {placeholder} is used, by name in lower case: sessions (host, username, extra arguments, jump host),
/// folders, profiles and the user's own local overrides and presets, for Environment Variables to say what's used and
/// what has no value yet
#[tauri::command]
pub(crate) fn variables_usage(state: State<'_, AppState>) -> BTreeMap<String, Vec<String>> {
    let mut usage: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut note = |texts: &[&str], place: String| {
        for text in texts {
            for name in variables::names_in(text) {
                let places = usage.entry(name).or_default();
                if !places.contains(&place) {
                    places.push(place.clone());
                }
            }
        }
    };
    {
        let sessions = state.sessions.lock().unwrap();
        for session in sessions.saved.iter().chain(&sessions.private).chain(&sessions.database) {
            let name = session.id().trim_start_matches("@db/").to_string();
            note(&[session.get("Host"), session.get("Username"), session.get("ExtraArgs"), session.get("JumpHost")], format!("session {name}"));
        }
        for folders in sessions.folders.values() {
            for folder in folders.values() {
                note(&[&folder.username, &folder.jump_host], format!("folder {}", folder.path));
            }
        }
    }
    for profile in &state.profiles.lock().unwrap().profiles {
        note(
            &[&profile.username, &profile.host, &profile.jump_host, &profile.key_fetch, &profile.ssh_options],
            format!("profile {}", profile.name),
        );
    }
    for (id, o) in overrides::load(&state.user_folder) {
        let texts = [o.host.as_deref(), o.username.as_deref(), o.extra_args.as_deref(), o.jump_host.as_deref()].map(|t| t.unwrap_or(""));
        note(&texts, format!("your override for {}", id.trim_start_matches("@db/")));
    }
    for (name, o) in overrides::presets(&state.user_folder) {
        let texts = [o.host.as_deref(), o.username.as_deref(), o.extra_args.as_deref(), o.jump_host.as_deref()].map(|t| t.unwrap_or(""));
        note(&texts, format!("preset {name}"));
    }
    usage
}

/// The profiles that sessions, folders, local overrides and presets name but that aren't set up on this computer,
/// each with the places naming it ([`profiles::missing`]), for Settings > Profiles to list under the user's own. Those
/// sessions open with Default Settings until it's set up.
#[tauri::command]
pub(crate) fn profiles_missing(state: State<'_, AppState>) -> Vec<(String, Vec<String>)> {
    let mut named: Vec<(String, String)> = Vec::new();
    {
        let sessions = state.sessions.lock().unwrap();
        for session in sessions.saved.iter().chain(&sessions.private).chain(&sessions.database) {
            let name = session.id().trim_start_matches("@db/").to_string();
            named.push((session.get("PuttySession").to_string(), format!("session {name}")));
        }
        for folders in sessions.folders.values() {
            for folder in folders.values() {
                named.push((folder.putty_profile.clone(), format!("folder {}", folder.path)));
            }
        }
    }
    for (id, o) in overrides::load(&state.user_folder) {
        named.push((o.putty_profile.unwrap_or_default(), format!("your override for {}", id.trim_start_matches("@db/"))));
    }
    for (name, o) in overrides::presets(&state.user_folder) {
        named.push((o.putty_profile.unwrap_or_default(), format!("preset {name}")));
    }
    let profiles = state.profiles.lock().unwrap();
    profiles::missing(&profiles, named.iter().map(|(n, p)| (n.as_str(), p.clone())))
}

/// The placeholders SuperXTerminal fills itself, with what each is
#[tauri::command]
pub(crate) fn variables_built_in() -> Vec<(String, String)> {
    variables::BUILT_IN.iter().map(|(n, d)| (n.to_string(), d.to_string())).collect()
}

/// Save the Environment Variables window; a secret left empty keeps its stored value
#[tauri::command]
pub(crate) fn variables_save(state: State<'_, AppState>, variables: Vec<variables::Variable>) -> Result<Vec<variables::Variable>, String> {
    variables::save(&state.user_folder, &variables)?;
    Ok(variables::list(&state.user_folder))
}

/// Text with its {key} placeholders filled from Environment Variables, and the keys that have no value (left as
/// written), for Send Commands, the button bar and scripts, as SuperXPuTTY's SessionVariables
#[derive(serde::Serialize)]
pub(crate) struct Filled {
    text: String,
    missing: Vec<String>,
}

/// Put the user's environment variables into `text`, and name any it uses that aren't set
#[tauri::command]
pub(crate) fn variables_fill(state: State<'_, AppState>, text: String) -> Filled {
    let values = variables::values(&state.user_folder);
    Filled { missing: variables::missing(&text, &values), text: variables::substitute(&text, &values) }
}
