//! Passwords and secret values in the Keychain (Credential Manager on Windows, Secret Service on Linux), service
//! "SuperXTerminal". Each one is read from the Keychain once per run and then kept in memory, so the change check
//! every few seconds and each connection don't ask the Keychain again; macOS asks the user to allow the app the first
//! time a new build reads an item.
//!
//! Tests never touch the Keychain: what a test stores is kept in memory, and only for that test ([`with_cache`]).

use std::collections::HashMap;
#[cfg(not(test))]
use std::sync::{Mutex, OnceLock};

/// The Keychain's service name every item is kept under (tests never use it)
#[cfg_attr(test, allow(dead_code))]
const SERVICE: &str = "SuperXTerminal";

/// Work with what's been read or written this run, by account; None for an account with nothing stored
#[cfg(not(test))]
fn with_cache<T>(work: impl FnOnce(&mut HashMap<String, Option<String>>) -> T) -> T {
    /// The cache, made the first time it's used
    static CACHE: OnceLock<Mutex<HashMap<String, Option<String>>>> = OnceLock::new();
    work(&mut CACHE.get_or_init(Default::default).lock().unwrap())
}

/// In tests, what's stored is each test's own. Cargo runs the tests side by side, each on a thread of its own, and
/// several use the same session ("Lab/sw1"): with one cache for them all, a password one test stored for a moment
/// was the password another test's session logged in with, and that test failed now and then (on Windows first,
/// where the timing differs). So the stand-in for the Keychain is kept by thread.
#[cfg(test)]
fn with_cache<T>(work: impl FnOnce(&mut HashMap<String, Option<String>>) -> T) -> T {
    thread_local! {
        /// This test's stored values
        static CACHE: std::cell::RefCell<HashMap<String, Option<String>>> = Default::default();
    }
    CACHE.with(|cache| work(&mut cache.borrow_mut()))
}

/// The value stored for an account (e.g. "database", "profile:root"), if any
pub fn get(account: &str) -> Option<String> {
    if let Some(known) = with_cache(|cache| cache.get(account).cloned()) {
        return known;
    }
    // tests never touch the Keychain
    #[cfg(not(test))]
    let value = keyring::Entry::new(SERVICE, account).ok().and_then(|e| e.get_password().ok()).filter(|v| !v.is_empty());
    #[cfg(test)]
    let value: Option<String> = None;
    with_cache(|cache| cache.insert(account.to_string(), value.clone()));
    value
}

/// Store a value for an account, or forget it when `value` is empty
pub fn set(account: &str, value: &str) -> Result<(), String> {
    // tests never touch the Keychain: what they store is kept for the run only
    #[cfg(test)]
    {
        with_cache(|cache| cache.insert(account.to_string(), (!value.is_empty()).then(|| value.to_string())));
        return Ok(());
    }
    #[cfg(not(test))]
    store(account, value)
}

/// Store a value in the Keychain itself, then in the cache
#[cfg(not(test))]
fn store(account: &str, value: &str) -> Result<(), String> {
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
    with_cache(|cache| cache.insert(account.to_string(), (!value.is_empty()).then(|| value.to_string())));
    Ok(())
}

#[cfg(test)]
mod tests {
    /// Two tests that happen to use the same session or profile run at once, each on its own thread. One stored a
    /// password while the other was working out how its session logs in, which then went by that password.
    #[test]
    fn what_one_test_stores_is_not_there_for_a_test_running_beside_it() {
        std::thread::spawn(|| super::set("session:Lab/sw1", "hunter2").unwrap()).join().unwrap();
        assert_eq!(super::get("session:Lab/sw1"), None);
        // its own it still finds, and forgets
        super::set("session:Lab/sw1", "its-own").unwrap();
        assert_eq!(super::get("session:Lab/sw1").as_deref(), Some("its-own"));
        super::set("session:Lab/sw1", "").unwrap();
        assert_eq!(super::get("session:Lab/sw1"), None);
    }
}
