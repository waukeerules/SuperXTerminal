//! The commands behind Settings > Profiles: listing, saving, removing, exporting and importing profiles, and the
//! team's profile templates. The profiles themselves, their file and their stored passwords are `crate::profiles`.

use tauri::State;

use crate::profiles::{self, Profile};
use crate::{applog, putty_reg, AppState};

/// The user's profiles, "Default Settings" first; passwords are only marked as stored, never sent
#[tauri::command]
pub(crate) fn profiles_list(state: State<'_, AppState>) -> Vec<Profile> {
    state.profiles.lock().unwrap().profiles.clone()
}

/// Add or change a profile (`previous_name` is its name before a rename; empty for a new one). `password`: None
/// keeps the stored one, "" forgets it, anything else replaces it.
#[tauri::command]
pub(crate) fn profiles_save(
    state: State<'_, AppState>,
    profile: Profile,
    previous_name: String,
    password: Option<String>,
) -> Result<Vec<Profile>, String> {
    let name = profile.name.trim().to_string();
    if name.is_empty() {
        return Err("A profile needs a name.".into());
    }
    let mut profiles = state.profiles.lock().unwrap();
    let renaming = !previous_name.is_empty() && !previous_name.eq_ignore_ascii_case(&name);
    if (previous_name.is_empty() || renaming) && profiles.get(&name).is_some() {
        return Err(format!("There's already a profile called {name}."));
    }
    if renaming && profiles::is_default(&previous_name) {
        return Err("Default Settings can't be renamed.".into());
    }
    // a renamed profile takes its stored password along
    if renaming && password.is_none() {
        if let Some(old) = profiles::password(&previous_name) {
            profiles::set_password(&name, &old)?;
        }
        profiles::set_password(&previous_name, "")?;
    }
    if let Some(password) = &password {
        profiles::set_password(&name, password)?;
    }
    let mut profile = Profile { name: name.clone(), ..profile };
    profile.has_password = profiles::password(&name).is_some();
    let key = if previous_name.is_empty() { name.clone() } else { previous_name };
    match profiles.profiles.iter().position(|p| p.name.eq_ignore_ascii_case(&key)) {
        Some(index) => profiles.profiles[index] = profile,
        None => profiles.profiles.push(profile),
    }
    // the profiles based on a renamed one follow it, rather than falling back to Default Settings
    if renaming {
        for other in profiles.profiles.iter_mut().filter(|p| p.based_on.trim().eq_ignore_ascii_case(&key)) {
            other.based_on = name.clone();
        }
    }
    profiles.save(&state.user_folder)?;
    Ok(profiles.profiles.clone())
}

/// Remove a profile and its stored password; "Default Settings" can't be removed
#[tauri::command]
pub(crate) fn profiles_delete(state: State<'_, AppState>, name: String) -> Result<Vec<Profile>, String> {
    if profiles::is_default(&name) {
        return Err("Default Settings can't be removed.".into());
    }
    let mut profiles = state.profiles.lock().unwrap();
    profiles.profiles.retain(|p| !p.name.eq_ignore_ascii_case(&name));
    profiles::set_password(&name, "")?;
    profiles.save(&state.user_folder)?;
    Ok(profiles.profiles.clone())
}

/// Export a profile to a file, without its password (Settings > Profiles, Export…)
#[tauri::command]
pub(crate) fn profiles_export(state: State<'_, AppState>, name: String, path: String) -> Result<(), String> {
    let profiles = state.profiles.lock().unwrap();
    let profile = profiles.get(&name).ok_or_else(|| format!("There's no profile called {name}."))?;
    std::fs::write(&path, profiles::export_text(profile)?).map_err(|e| e.to_string())
}

/// PuTTY's saved sessions in a `reg export` file, as profiles for Settings > Profiles' Import… to add (putty_reg.rs)
#[tauri::command]
pub(crate) fn profiles_read_putty(path: String) -> Result<Vec<Profile>, String> {
    putty_reg::read(&std::fs::read(&path).map_err(|e| e.to_string())?)
}

/// The team's profile templates (ProfileTemplates.json in the settings folder), for Settings > Profiles to offer the
/// ones not set up here; none without a settings folder
#[tauri::command]
pub(crate) fn profile_templates(state: State<'_, AppState>) -> Result<Vec<Profile>, String> {
    let folder = state.settings.lock().unwrap().settings_folder.clone();
    folder.map_or(Ok(Vec::new()), |f| profiles::templates(&f))
}

/// Share a profile with the team as a template in the settings folder, without its login (Settings > Profiles'
/// Share as Team Template)
#[tauri::command]
pub(crate) fn profile_template_share(state: State<'_, AppState>, name: String) -> Result<Vec<Profile>, String> {
    let folder = state.settings.lock().unwrap().settings_folder.clone().ok_or("Choose a settings folder first (Settings > General): the templates are kept there for the team.")?;
    let profile = state.profiles.lock().unwrap().get(&name).cloned().ok_or_else(|| format!("There's no profile called {name}."))?;
    let shared = profiles::share_template(&folder, &profile)?;
    applog::info(format!("Shared profile {name} as a team template"));
    Ok(shared)
}

/// The profiles in a file, for Settings > Profiles' Import… to add (it names them and saves each)
#[tauri::command]
pub(crate) fn profiles_read_file(path: String) -> Result<Vec<Profile>, String> {
    profiles::import_text(&std::fs::read_to_string(&path).map_err(|e| e.to_string())?)
}
