//! Environment Variables: the user's values for `{key}` placeholders in sessions (host, username, extra arguments,
//! jump host), so one Sessions.xml can serve a whole team, as SuperXPuTTY's SessionVariables.
//!
//! The file is SuperXPuTTY's `EnvironmentVariables.json` (`{"Variables":[{"Key","Value","Secret"}]}`) in the user's
//! folder, so a user can copy theirs over from Windows. Secret values go in the Keychain instead of the file. A
//! secret copied from Windows is DPAPI-encrypted (`ProtectedValue`), which a Mac can't read: it shows as needing to
//! be typed again, and its `ProtectedValue` is kept in the file so the file still works on Windows.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// The Keychain service secrets are kept under (with profiles' passwords)
const KEYCHAIN_SERVICE: &str = "SuperXTerminal";

/// One variable as the window shows it; a secret's value never leaves the core
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Variable {
    pub key: String,
    /// The value; for a secret, empty when sent to the window
    pub value: String,
    pub secret: bool,
    /// A secret with a value in the Keychain
    pub has_value: bool,
    /// A secret set on Windows (DPAPI) that has to be typed again here
    pub from_windows: bool,
}

/// The file's entries, each kept whole so fields this app doesn't use (Windows' ProtectedValue) are written back
fn read_entries(path: &Path) -> Vec<Map<String, Value>> {
    let Ok(text) = std::fs::read_to_string(path) else { return Vec::new() };
    let Ok(Value::Object(root)) = serde_json::from_str::<Value>(text.trim_start_matches('\u{feff}')) else { return Vec::new() };
    match root.get("Variables") {
        Some(Value::Array(list)) => list.iter().filter_map(|v| v.as_object().cloned()).collect(),
        _ => Vec::new(),
    }
}

/// The file's path in the user's folder
fn file(user_folder: &Path) -> std::path::PathBuf {
    user_folder.join("EnvironmentVariables.json")
}

/// The Keychain entry for a secret variable
fn entry(key: &str) -> Option<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &format!("variable:{}", key.to_lowercase())).ok()
}

/// Every variable for the window, secrets without their values
pub fn list(user_folder: &Path) -> Vec<Variable> {
    read_entries(&file(user_folder))
        .iter()
        .map(|e| {
            let key = e.get("Key").and_then(Value::as_str).unwrap_or("").to_string();
            let secret = e.get("Secret").and_then(Value::as_bool).unwrap_or(false);
            let has_value = secret && entry(&key).and_then(|k| k.get_password().ok()).is_some_and(|v| !v.is_empty());
            Variable {
                value: if secret { String::new() } else { e.get("Value").and_then(Value::as_str).unwrap_or("").to_string() },
                from_windows: secret && !has_value && e.get("ProtectedValue").is_some(),
                key,
                secret,
                has_value,
            }
        })
        .collect()
}

/// Every defined value by key (ignoring case), secrets from the Keychain; for substitution only
pub fn values(user_folder: &Path) -> HashMap<String, String> {
    let mut values = HashMap::new();
    for e in read_entries(&file(user_folder)) {
        let key = e.get("Key").and_then(Value::as_str).unwrap_or("").to_string();
        if key.is_empty() {
            continue;
        }
        let value = if e.get("Secret").and_then(Value::as_bool).unwrap_or(false) {
            entry(&key).and_then(|k| k.get_password().ok())
        } else {
            e.get("Value").and_then(Value::as_str).map(str::to_string)
        };
        if let Some(value) = value {
            values.insert(key.to_lowercase(), value);
        }
    }
    values
}

/// Save the window's variables. A secret's empty value keeps what's stored; a typed one replaces it. Removed
/// variables' secrets are forgotten.
pub fn save(user_folder: &Path, variables: &[Variable]) -> Result<(), String> {
    let old = read_entries(&file(user_folder));
    let mut entries = Vec::new();
    for variable in variables {
        let key = variable.key.trim();
        if key.is_empty() {
            continue;
        }
        if !key.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Err(format!("\"{key}\" can't be a placeholder name: use letters, digits and _ only."));
        }
        // start from the old entry, so Windows' ProtectedValue and any other field stay
        let mut e = old
            .iter()
            .find(|o| o.get("Key").and_then(Value::as_str).is_some_and(|k| k.eq_ignore_ascii_case(key)))
            .cloned()
            .unwrap_or_default();
        e.insert("Key".into(), Value::String(key.into()));
        e.insert("Secret".into(), Value::Bool(variable.secret));
        if variable.secret {
            e.remove("Value");
            if !variable.value.is_empty() {
                entry(key).ok_or("The Keychain isn't available")?.set_password(&variable.value).map_err(|e| e.to_string())?;
            }
        } else {
            e.insert("Value".into(), Value::String(variable.value.clone()));
            e.remove("ProtectedValue");
            forget(key);
        }
        entries.push(Value::Object(e));
    }
    for o in &old {
        let key = o.get("Key").and_then(Value::as_str).unwrap_or("");
        if !variables.iter().any(|v| v.key.trim().eq_ignore_ascii_case(key)) {
            forget(key);
        }
    }
    std::fs::create_dir_all(user_folder).map_err(|e| e.to_string())?;
    let mut root = Map::new();
    root.insert("Variables".into(), Value::Array(entries));
    let text = serde_json::to_string_pretty(&Value::Object(root)).map_err(|e| e.to_string())?;
    std::fs::write(file(user_folder), text).map_err(|e| e.to_string())
}

/// Forget a secret's stored value
fn forget(key: &str) {
    if let Some(entry) = entry(key) {
        let _ = entry.delete_credential();
    }
}

/// Replace each `{key}` with its value in one pass; undefined keys stay, as SessionVariables.Substitute
pub fn substitute(text: &str, values: &HashMap<String, String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) if close > 0 && after[..close].chars().all(|c| c.is_alphanumeric() || c == '_') => {
                match values.get(&after[..close].to_lowercase()) {
                    Some(value) => out.push_str(value),
                    None => out.push_str(&rest[open..open + close + 2]),
                }
                rest = &after[close + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// The `{key}` placeholders in a text that have no value, as SessionVariables.FindMissing
pub fn missing(text: &str, values: &HashMap<String, String>) -> Vec<String> {
    let mut found: Vec<String> = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let after = &rest[open + 1..];
        if let Some(close) = after.find('}') {
            let key = &after[..close];
            if !key.is_empty() && key.chars().all(|c| c.is_alphanumeric() || c == '_') && !values.contains_key(&key.to_lowercase())
                && !found.iter().any(|f| f.eq_ignore_ascii_case(&format!("{{{key}}}")))
            {
                found.push(format!("{{{key}}}"));
            }
        }
        rest = after;
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_are_replaced_once_ignoring_case() {
        let values = HashMap::from([("user".to_string(), "tim".to_string()), ("loop".to_string(), "{user}".to_string())]);
        assert_eq!(substitute("{User}@{domain} {loop} {prompt:x}", &values), "tim@{domain} {user} {prompt:x}");
        assert_eq!(missing("{User}@{domain} {DOMAIN} {prompt:x}", &values), ["{domain}"]);
    }
}
