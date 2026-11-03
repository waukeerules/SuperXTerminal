//! Profiles: how each user logs in, matched by name to a session's PuTTY profile (docs/PLAN.md §2 Profiles).
//!
//! Shared data (Sessions.xml, the Database table) names a profile ("root", "Default Settings"); each user's own
//! profile of that name says how they log in, as their PuTTY profile does on Windows. Profiles are per user, in
//! `Profiles.json` in the user's folder; passwords are in the OS's secret store (the Keychain on a Mac), never in a
//! file and never sent to the UI. "Default Settings" always exists, and every other profile falls back to it.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::secrets;

/// The profile every session without one uses, and every profile falls back to, as PuTTY's
pub const DEFAULT_PROFILE: &str = "Default Settings";


/// One profile. Empty fields aren't set, and fall back to "Default Settings", then to ssh's own defaults.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Profile {
    pub name: String,
    pub username: String,
    /// 0 for not set
    pub port: u16,
    /// A private key file (OpenSSH format); empty: ssh's own keys and the agent
    pub key_file: String,
    /// A command that gets a new key into the key file, for keys issued for a short time (CyberArk's, Vault's, a
    /// Teleport certificate…), run in the session's tab when the key is missing or older than `key_lifetime`: its
    /// sign-in (a link, a PIN) happens there. `{key_file}` and the Environment Variables' placeholders are filled in.
    /// Run through the user's shell (its rc file read, so a shell function works) or PowerShell on Windows.
    pub key_fetch: String,
    /// Minutes a fetched key is good for, by the file's modified time; 0: fetched only when there's none
    pub key_lifetime: u32,
    /// Whether ssh offers its keys (~/.ssh and the agent) when there's no key file; None: the default's, else yes.
    /// Off for network devices: Cisco closes the connection after 3 failed attempts (ip ssh authentication-retries),
    /// and each key ssh offers is one, so with no stored password the password prompt never came
    pub offer_keys: Option<bool>,
    /// When a session has no username from anywhere, the tab asks for one ("login as:", as PuTTY and SecureCRT do)
    /// before connecting; None: the default's, else no (ssh then uses this computer's user name)
    pub ask_username: Option<bool>,
    pub jump_host: String,
    /// Where ssh connects, at the profile's port, for a session with no host of its own, as a PuTTY saved session's
    /// Host Name; and for one whose username names its host as {host_address} (a CyberArk SSH gateway, which takes the
    /// target in the username). Otherwise the session's host wins (docs/CYBERARK.md). Was "gatewayHost".
    #[serde(alias = "gatewayHost")]
    pub host: String,
    /// Seconds between keep-alives (ssh's ServerAliveInterval); 0 for not set
    pub keepalive: u32,
    /// More ssh arguments, e.g. "-L 8080:localhost:80 -o Compression=yes"
    pub ssh_options: String,
    /// Accept the older algorithms older devices offer (SHA-1 key exchange, ssh-rsa, CBC ciphers); None: the default's
    pub older_algorithms: Option<bool>,
    /// Its own algorithm lists, as PuTTY's Kex, Host keys and Cipher panels ("kex=a,b;mac=c", algorithms.rs); a kind it
    /// doesn't set comes from Default Settings, else ssh's own list
    pub algorithms: String,
    /// A Host from ~/.ssh/config, used for anything this profile doesn't set
    pub ssh_config_host: String,
    /// The terminal's cursor: "" (default), "block", "underline" or "bar"
    pub cursor: String,
    /// Whether the cursor blinks; None: the default's
    pub cursor_blink: Option<bool>,
    /// The bell: "" (default), "visual", "sound" or "none"
    pub bell: String,
    /// Which colours its tabs have: "" or "default" (Default Settings'), "theme" (Settings > Terminal's theme), "dark" or "light"
    /// (the Dark or Light theme), or "custom" (the colours below). A profile with colours and no choice is custom.
    pub colours: String,
    /// The terminal's colours when custom, "#rrggbb", over the theme's and Settings > Terminal's; "" for the default's
    pub background: String,
    pub foreground: String,
    pub selection: String,
    /// The cursor's colour; a session's or folder's environment colour still wins
    pub cursor_color: String,
    /// Whether a password is stored for it (the password itself stays in the Keychain)
    #[serde(skip_deserializing)]
    pub has_password: bool,
}

/// Every profile, "Default Settings" first
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Profiles {
    pub profiles: Vec<Profile>,
}

impl Profiles {
    /// Read `Profiles.json`; missing or unreadable means just "Default Settings"
    pub fn load(user_folder: &Path) -> Profiles {
        let mut profiles: Profiles = std::fs::read_to_string(user_folder.join("Profiles.json"))
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        profiles.ensure_default();
        for profile in &mut profiles.profiles {
            profile.has_password = password(&profile.name).is_some();
        }
        profiles
    }

    /// Write `Profiles.json`
    pub fn save(&self, user_folder: &Path) -> Result<(), String> {
        std::fs::create_dir_all(user_folder).map_err(|e| e.to_string())?;
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(user_folder.join("Profiles.json"), text).map_err(|e| e.to_string())
    }

    /// Make sure "Default Settings" exists, first; new, it accepts older algorithms, as PuTTY does
    fn ensure_default(&mut self) {
        if let Some(index) = self.profiles.iter().position(|p| is_default(&p.name)) {
            let default = self.profiles.remove(index);
            self.profiles.insert(0, default);
        } else {
            let default = Profile { name: DEFAULT_PROFILE.into(), older_algorithms: Some(true), ..Profile::default() };
            self.profiles.insert(0, default);
        }
    }

    /// A profile by name, ignoring case
    pub fn get(&self, name: &str) -> Option<&Profile> {
        self.profiles.iter().find(|p| p.name.eq_ignore_ascii_case(name.trim()))
    }

    /// "Default Settings"
    pub fn default_profile(&self) -> &Profile {
        self.get(DEFAULT_PROFILE).expect("Default Settings always exists")
    }
}

/// Whether a profile name means "Default Settings" (also empty, as SuperXPuTTY's FolderSettings.IsDefaultProfile)
pub fn is_default(name: &str) -> bool {
    name.trim().is_empty() || name.trim().eq_ignore_ascii_case(DEFAULT_PROFILE)
}

/// The Keychain account for a profile's password
fn account(profile: &str) -> String {
    format!("profile:{}", profile.to_lowercase())
}

/// A profile's stored password, if it has one
pub fn password(profile: &str) -> Option<String> {
    secrets::get(&account(profile))
}

/// Store a profile's password, or forget it when `password` is empty
pub fn set_password(profile: &str, password: &str) -> Result<(), String> {
    secrets::set(&account(profile), password)
}

#[cfg(test)]
mod tests {
    use super::Profile;

    #[test]
    fn a_profile_saved_with_gateway_host_reads_it_as_its_host() {
        let profile: Profile = serde_json::from_str(r#"{"name":"CyberArk","gatewayHost":"gw.example.test"}"#).unwrap();
        assert_eq!(profile.host, "gw.example.test");
        assert!(serde_json::to_string(&profile).unwrap().contains(r#""host":"gw.example.test""#));
    }
}
