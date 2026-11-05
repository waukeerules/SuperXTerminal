//! Profiles: how each user logs in, matched by name to a session's PuTTY profile (docs/PLAN.md §2 Profiles).
//!
//! Shared data (Sessions.xml, the Database table) names a profile ("root", "Default Settings"); each user's own
//! profile of that name says how they log in, as their PuTTY profile does on Windows. Profiles are per user, in
//! `Profiles.json` in the user's folder; passwords are in the OS's secret store (the Keychain on a Mac), never in a
//! file and never sent to the UI. "Default Settings" always exists, and every other profile falls back to it.
//!
//! A profile is exported to a file of the same shape as `Profiles.json` (Settings > Profiles, Export…), without its
//! password or whether it has one; an import reads that, a whole `Profiles.json`, or one profile on its own.

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
    /// Which addresses a host name is reached at, as PuTTY's Internet protocol version: "" (Default Settings'), "auto",
    /// "ipv4" or "ipv6" (ssh's -4 and -6; Telnet and Raw try only those addresses)
    pub address_family: String,
    /// Typed into the session after `anti_idle_seconds` with nothing typed, and again after each such spell, as
    /// KiTTY's anti-idle string: a device's or shell's own idle timeout (Cisco's exec-timeout, TMOUT) doesn't see
    /// ssh's keep-alives and logs the session out. KiTTY's escapes (\x05, \k08, \r...), turned into keys by the tab.
    /// "" for Default Settings'
    pub anti_idle: String,
    /// Seconds without typing before the anti-idle string is sent; 0 for Default Settings', else KiTTY's 180
    pub anti_idle_seconds: u32,
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
    /// The colour of its sessions' tabs, "#rrggbb", whatever the Colours choice; "" for Default Settings', else none.
    /// A session's or folder's environment colour still wins, as it marks production
    pub tab_color: String,
    /// The button bar (a bar's name in ButtonBars.xml) shown while one of its sessions' tabs is active; "" for
    /// Default Settings', else the bar the tab's icon picks. Matched whatever its case
    pub button_bar: String,
    /// An SPSL script its sessions run as they open, when a session has no script of its own (Edit Session > Terminal):
    /// a file (absolute, file://, or relative to the settings folder) or an http(s) address, as a session's. "" for
    /// Default Settings', else none. So a session can be just a name and a host, and its profile logs in and sets up
    pub script: String,
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

/// The script a session with no script of its own runs: its profile's, else Default Settings'; "" for none. With
/// the profile's name, for saying whose script it is
pub fn script_of<'a>(profiles: &'a Profiles, profile_name: &str) -> Option<(&'a str, &'a str)> {
    let default = profiles.default_profile();
    let profile = if is_default(profile_name) { default } else { profiles.get(profile_name).unwrap_or(default) };
    [profile, default].into_iter().find(|p| !p.script.trim().is_empty()).map(|p| (p.script.trim(), p.name.as_str()))
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

/// A profile as an exported file: `{"profiles": [ … ]}`, the shape of `Profiles.json`. No password: it's in the secret
/// store, never in a file, and whether one is stored isn't written either, as it means nothing on another computer
pub fn export_text(profile: &Profile) -> Result<String, String> {
    let mut value = serde_json::to_value(profile).map_err(|e| e.to_string())?;
    if let Some(fields) = value.as_object_mut() {
        fields.remove("hasPassword");
    }
    serde_json::to_string_pretty(&serde_json::json!({ "profiles": [value] })).map_err(|e| e.to_string())
}

/// The profiles in an exported file, a whole `Profiles.json`, or one profile on its own, their names trimmed; an
/// error when there are none, or one has no name
pub fn import_text(text: &str) -> Result<Vec<Profile>, String> {
    let value: serde_json::Value = serde_json::from_str(text).map_err(|e| format!("Not a profiles file: {e}"))?;
    let list = match value.get("profiles") {
        Some(list) => serde_json::from_value::<Vec<Profile>>(list.clone()),
        None => serde_json::from_value::<Profile>(value).map(|p| vec![p]),
    }
    .map_err(|e| format!("Not a profiles file: {e}"))?;
    let mut profiles = Vec::new();
    for mut profile in list {
        profile.name = profile.name.trim().to_string();
        if profile.name.is_empty() {
            return Err("A profile in the file has no name.".into());
        }
        profiles.push(profile);
    }
    if profiles.is_empty() {
        return Err("There are no profiles in the file.".into());
    }
    Ok(profiles)
}

#[cfg(test)]
mod tests {
    use super::{export_text, import_text, script_of, Profile, Profiles};

    #[test]
    fn an_exported_profile_has_no_password_and_imports_as_it_was() {
        let profile = Profile { name: "root".into(), username: "admin".into(), port: 2222, key_file: "~/.ssh/id_root".into(),
            has_password: true, ..Profile::default() };
        let text = export_text(&profile).unwrap();
        // the password is never in a profile, and whether one is stored isn't written
        assert!(!text.to_lowercase().contains("password"), "{text}");
        let imported = import_text(&text).unwrap();
        assert_eq!(imported.len(), 1);
        assert_eq!((imported[0].name.as_str(), imported[0].username.as_str(), imported[0].port), ("root", "admin", 2222));
        assert_eq!(imported[0].key_file, "~/.ssh/id_root");
        assert!(!imported[0].has_password);
    }

    #[test]
    fn a_whole_profiles_json_or_one_profile_on_its_own_imports() {
        let all = import_text(r#"{"profiles":[{"name":"Default Settings","hasPassword":true},{"name":" root ","username":"admin"}]}"#).unwrap();
        assert_eq!(all.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(), ["Default Settings", "root"]);
        // hasPassword in a Profiles.json copied over is ignored: the password stays on the computer it came from
        assert!(all.iter().all(|p| !p.has_password));
        assert_eq!(import_text(r#"{"name":"lab","port":22}"#).unwrap()[0].name, "lab");
    }

    #[test]
    fn a_file_that_isnt_profiles_is_refused() {
        assert!(import_text("not json").unwrap_err().starts_with("Not a profiles file"));
        assert_eq!(import_text(r#"{"profiles":[]}"#).unwrap_err(), "There are no profiles in the file.");
        assert_eq!(import_text(r#"{"profiles":[{"name":"  "}]}"#).unwrap_err(), "A profile in the file has no name.");
        assert!(import_text(r#"{"profiles":"root"}"#).is_err());
    }

    #[test]
    fn a_profiles_tab_colour_and_button_bar_are_kept_and_go_with_an_export() {
        // the page sends them in its own names; a field Rust didn't know was dropped when Profiles.json was written
        let profile: Profile = serde_json::from_str(r##"{"name":"Lab","tabColor":"#2da44e","buttonBar":"Cisco"}"##).unwrap();
        assert_eq!((profile.tab_color.as_str(), profile.button_bar.as_str()), ("#2da44e", "Cisco"));
        let imported = import_text(&export_text(&profile).unwrap()).unwrap();
        assert_eq!((imported[0].tab_color.as_str(), imported[0].button_bar.as_str()), ("#2da44e", "Cisco"));
        // a profile from before them has neither
        let older: Profile = serde_json::from_str(r#"{"name":"root"}"#).unwrap();
        assert_eq!((older.tab_color.as_str(), older.button_bar.as_str()), ("", ""));
    }

    #[test]
    fn a_session_with_no_script_runs_its_profiles_else_default_settings() {
        let mut profiles: Profiles = serde_json::from_str(
            r#"{"profiles":[{"name":"Linux","script":" omv.spsl "},{"name":"Cisco"},{"name":"Default Settings"}]}"#,
        )
        .unwrap();
        profiles.ensure_default();
        assert_eq!(script_of(&profiles, "linux"), Some(("omv.spsl", "Linux")));
        assert_eq!(script_of(&profiles, "Cisco"), None);
        assert_eq!(script_of(&profiles, ""), None);
        profiles.profiles[0].script = "https://example.com/all.spsl".into();
        assert_eq!(script_of(&profiles, "Cisco"), Some(("https://example.com/all.spsl", "Default Settings")));
        assert_eq!(script_of(&profiles, "Gone"), Some(("https://example.com/all.spsl", "Default Settings")));
        assert_eq!(script_of(&profiles, "Linux"), Some(("omv.spsl", "Linux")));
    }

    #[test]
    fn a_profiles_script_is_kept_and_goes_with_an_export() {
        let profile: Profile = serde_json::from_str(r#"{"name":"Linux","script":"scripts/omv.spsl"}"#).unwrap();
        let imported = import_text(&export_text(&profile).unwrap()).unwrap();
        assert_eq!(imported[0].script, "scripts/omv.spsl");
        let older: Profile = serde_json::from_str(r#"{"name":"Old"}"#).unwrap();
        assert_eq!(older.script, "");
    }

    #[test]
    fn a_profile_saved_with_gateway_host_reads_it_as_its_host() {
        let profile: Profile = serde_json::from_str(r#"{"name":"CyberArk","gatewayHost":"gw.example.test"}"#).unwrap();
        assert_eq!(profile.host, "gw.example.test");
        assert!(serde_json::to_string(&profile).unwrap().contains(r#""host":"gw.example.test""#));
    }
}
