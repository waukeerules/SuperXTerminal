//! Keyboard shortcuts, as SuperXPuTTY's Options > Shortcuts (its SuperPuttyAction keys), for the Mac: each action is
//! a menu bar item and its shortcut that item's key, so it works wherever the focus is, a terminal included. The user's
//! changes are kept in the settings (`shortcuts`: action to key, "" for none); the rest have the Mac defaults here.
//!
//! Keys are written as Tauri accelerators: "CmdOrCtrl+Shift+O", "Ctrl+CmdOrCtrl+F", "F5".

use std::collections::BTreeMap;

use serde::Serialize;

/// One action: its id (the menu item's, which the UI gets as a "menu" event), its name, and its key by default
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub default: &'static str,
}

/// Every action with a shortcut, in the order the editor lists them. SuperXPuTTY's actions are here under its names
/// where the Mac has the same thing (Close Tab, Next Tab, Options as Settings…, Full Screen, Open Session, ...).
pub const ACTIONS: &[Action] = &[
    Action { id: "newWindow", label: "New Window", default: "CmdOrCtrl+N" },
    Action { id: "newLocalShell", label: "New Local Shell", default: "CmdOrCtrl+T" },
    Action { id: "openSession", label: "Open Session…", default: "CmdOrCtrl+O" },
    Action { id: "switchSession", label: "Switch Session…", default: "CmdOrCtrl+Shift+O" },
    Action { id: "closeTab", label: "Close Tab", default: "CmdOrCtrl+W" },
    Action { id: "nextTab", label: "Next Tab", default: "CmdOrCtrl+Shift+]" },
    Action { id: "prevTab", label: "Previous Tab", default: "CmdOrCtrl+Shift+[" },
    Action { id: "duplicateSession", label: "Duplicate Session", default: "" },
    Action { id: "renameTab", label: "Rename Tab…", default: "" },
    Action { id: "restartSession", label: "Restart Session", default: "" },
    Action { id: "splitRight", label: "Split Right", default: "CmdOrCtrl+D" },
    Action { id: "splitDown", label: "Split Down", default: "CmdOrCtrl+Shift+D" },
    Action { id: "joinAll", label: "Join All Tabs", default: "" },
    Action { id: "find", label: "Find…", default: "CmdOrCtrl+F" },
    Action { id: "clearScrollback", label: "Clear Scrollback", default: "CmdOrCtrl+K" },
    Action { id: "resetTerminal", label: "Reset Terminal", default: "" },
    Action { id: "copyAll", label: "Copy All to Clipboard", default: "" },
    Action { id: "gotoCommandBar", label: "Go to Command Bar", default: "" },
    Action { id: "gotoConnectionBar", label: "Go to Quick Connect", default: "" },
    Action { id: "focusActiveSession", label: "Focus Terminal", default: "" },
    Action { id: "fullScreen", label: "Full Screen", default: "Ctrl+CmdOrCtrl+F" },
    Action { id: "settings", label: "Settings…", default: "CmdOrCtrl+," },
    Action { id: "scriptEditor", label: "Script Editor…", default: "" },
    Action { id: "saveLayout", label: "Save Layout", default: "" },
    Action { id: "saveLayoutAs", label: "Save Layout As…", default: "" },
    Action { id: "toggleSessionsAutoHide", label: "Auto Hide Sessions", default: "" },
    Action { id: "toggleSendCommands", label: "Send Commands", default: "" },
    Action { id: "toggleButtonBar", label: "Button Bar", default: "" },
    Action { id: "toggleLayouts", label: "Layouts", default: "" },
    Action { id: "toggleSessionDetail", label: "Session Detail", default: "" },
];

/// The keys macOS and the Edit menu keep: they can't be given to an action
pub const RESERVED: &[&str] = &[
    "CmdOrCtrl+Q", "CmdOrCtrl+H", "CmdOrCtrl+Alt+H", "CmdOrCtrl+M", "CmdOrCtrl+C", "CmdOrCtrl+V", "CmdOrCtrl+X",
    "CmdOrCtrl+A", "CmdOrCtrl+Z", "CmdOrCtrl+Shift+Z", "CmdOrCtrl+Tab", "CmdOrCtrl+Space",
];

/// An action's label, for the menu
pub fn label(id: &str) -> &'static str {
    ACTIONS.iter().find(|a| a.id == id).map(|a| a.label).unwrap_or("")
}

/// An action's key now: the user's, else the default; None for none
pub fn key(shortcuts: &BTreeMap<String, String>, id: &str) -> Option<String> {
    let key = match shortcuts.get(id) {
        Some(mine) => mine.clone(),
        None => ACTIONS.iter().find(|a| a.id == id).map(|a| a.default.to_string()).unwrap_or_default(),
    };
    (!key.is_empty()).then_some(key)
}

/// One row of the Shortcuts editor
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: &'static str,
    pub label: &'static str,
    pub default: &'static str,
    /// The key now ("" for none)
    pub key: String,
}

/// The editor's rows
pub fn rows(shortcuts: &BTreeMap<String, String>) -> Vec<Row> {
    ACTIONS.iter().map(|a| Row { id: a.id, label: a.label, default: a.default, key: key(shortcuts, a.id).unwrap_or_default() }).collect()
}

/// The user's changes as they should be kept: only keys that differ from the default, a key used twice given to the
/// last action that asked for it, and none of the reserved ones
pub fn cleaned(changes: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for action in ACTIONS {
        if let Some(key) = changes.get(action.id) {
            let key = if RESERVED.iter().any(|r| r.eq_ignore_ascii_case(key)) { "" } else { key.as_str() };
            if key != action.default {
                out.insert(action.id.to_string(), key.to_string());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_changes_and_reserved_keys() {
        let mut changes = BTreeMap::new();
        changes.insert("nextTab".to_string(), "CmdOrCtrl+Alt+Right".to_string());
        changes.insert("closeTab".to_string(), "CmdOrCtrl+W".to_string());
        changes.insert("find".to_string(), "CmdOrCtrl+C".to_string());
        changes.insert("splitRight".to_string(), String::new());
        let kept = cleaned(&changes);
        assert_eq!(kept.get("closeTab"), None, "the default isn't kept as a change");
        assert_eq!(kept.get("find").map(String::as_str), Some(""), "a reserved key leaves the action without one");
        assert_eq!(key(&kept, "nextTab").as_deref(), Some("CmdOrCtrl+Alt+Right"));
        assert_eq!(key(&kept, "splitRight"), None);
        assert_eq!(key(&kept, "openSession").as_deref(), Some("CmdOrCtrl+O"));
        assert_eq!(label("copyAll"), "Copy All to Clipboard");
    }
}
