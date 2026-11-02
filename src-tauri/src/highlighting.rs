//! Highlighting rule sets and themes from the user's and the team's files, as SuperXPuTTY's OutputHighlighting.
//!
//! `*.rules.json` and `*.theme.json` files in `<SettingsFolder>/highlighting` (shared with the team) and in the user's
//! own `highlighting` folder (theirs win). A rule set with the same `id`, or a theme with the same `name`, replaces
//! an earlier one, including the defaults the UI loads from `public/web/output`. The files may have `//` comments.
//! The terminal page compiles and applies them (`web/shared/rules.js`).

use std::path::Path;

use serde_json::Value;

/// What was read: rule sets, themes, and files that were skipped, with why
#[derive(serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Highlighting {
    pub rule_sets: Vec<Value>,
    pub themes: Vec<Value>,
    pub problems: Vec<String>,
}

/// Read the shared folder's files, then the user's, each replacing what came before by id or name
pub fn load(folders: &[&Path]) -> Highlighting {
    let mut found = Highlighting::default();
    for folder in folders {
        for (suffix, key, list) in [(".rules.json", "id", &mut found.rule_sets), (".theme.json", "name", &mut found.themes)] {
            for item in read_all(folder, suffix, &mut found.problems) {
                merge(list, item, key);
            }
        }
    }
    found
}

/// Every file in a folder ending in `suffix`, parsed, in name order; a missing folder is no files
fn read_all(folder: &Path, suffix: &str, problems: &mut Vec<String>) -> Vec<Value> {
    let Ok(entries) = std::fs::read_dir(folder) else { return Vec::new() };
    let mut paths: Vec<_> = entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.file_name().and_then(|n| n.to_str()).is_some_and(|n| n.to_ascii_lowercase().ends_with(suffix)))
        .collect();
    paths.sort();
    let mut items = Vec::new();
    for path in paths {
        let parsed = std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|text| serde_json::from_str::<Value>(&strip_comments(&text)).map_err(|e| e.to_string()));
        match parsed {
            Ok(value) if value.is_object() => items.push(value),
            Ok(_) => problems.push(format!("{}: not a JSON object", path.display())),
            Err(e) => problems.push(format!("{}: {e}", path.display())),
        }
    }
    items
}

/// Add an item, replacing one with the same key (ignoring case)
fn merge(list: &mut Vec<Value>, item: Value, key: &str) {
    let name = item.get(key).and_then(Value::as_str).unwrap_or("").to_lowercase();
    if name.is_empty() {
        list.push(item);
        return;
    }
    match list.iter().position(|v| v.get(key).and_then(Value::as_str).map(str::to_lowercase).as_deref() == Some(name.as_str())) {
        Some(index) => list[index] = item,
        None => list.push(item),
    }
}

/// JSON with `//` comments made plain JSON; "//" inside strings (regexes, URLs) is kept
fn strip_comments(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut chars = text.trim_start_matches('\u{feff}').chars().peekable();
    let (mut in_string, mut escaped) = (false, false);
    while let Some(c) = chars.next() {
        if in_string {
            out.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_string = false;
            }
        } else if c == '"' {
            in_string = true;
            out.push(c);
        } else if c == '/' && chars.peek() == Some(&'/') {
            // skip to the end of the line, keeping the line break
            for skipped in chars.by_ref() {
                if skipped == '\n' {
                    out.push('\n');
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn comments_go_but_slashes_in_strings_stay() {
        let text = "{ \"match\": \"https?://\\\\S+\", // a link\n  \"style\": \"info\" }";
        let value: Value = serde_json::from_str(&strip_comments(text)).unwrap();
        assert_eq!(value["match"], "https?://\\S+");
        assert_eq!(value["style"], "info");
    }

    #[test]
    fn a_later_file_replaces_an_earlier_one_by_id() {
        let mut list = vec![serde_json::json!({ "id": "cisco", "name": "old" })];
        merge(&mut list, serde_json::json!({ "id": "Cisco", "name": "new" }), "id");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0]["name"], "new");
    }
}
