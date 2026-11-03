//! Passwords and secret values in the Keychain (Credential Manager on Windows, Secret Service on Linux), service
//! "SuperXTerminal". Each one is read from the Keychain once per run and then kept in memory, so the change check
//! every few seconds and each connection don't ask the Keychain again; macOS asks the user to allow the app the first
//! time a new build reads an item.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

const SERVICE: &str = "SuperXTerminal";

/// What's been read or written this run, by account; None when there's nothing stored
fn cache() -> &'static Mutex<HashMap<String, Option<String>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// The value stored for an account (e.g. "database", "profile:root"), if any
pub fn get(account: &str) -> Option<String> {
    if let Some(known) = cache().lock().unwrap().get(account) {
        return known.clone();
    }
    // tests never touch the Keychain
    #[cfg(not(test))]
    let value = keyring::Entry::new(SERVICE, account).ok().and_then(|e| e.get_password().ok()).filter(|v| !v.is_empty());
    #[cfg(test)]
    let value: Option<String> = None;
    cache().lock().unwrap().insert(account.to_string(), value.clone());
    value
}

/// Store a value for an account, or forget it when `value` is empty
pub fn set(account: &str, value: &str) -> Result<(), String> {
    let store = if cfg!(target_os = "macos") { "The Keychain" } else { "Windows Credential Manager" };
    let entry = keyring::Entry::new(SERVICE, account).map_err(|_| format!("{store} isn't available"))?;
    if value.is_empty() {
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(e) => return Err(e.to_string()),
        }
    } else {
        entry.set_password(value).map_err(|e| e.to_string())?;
    }
    cache().lock().unwrap().insert(account.to_string(), (!value.is_empty()).then(|| value.to_string()));
    Ok(())
}
