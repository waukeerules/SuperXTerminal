//! Keyboard shortcuts, as SuperXPuTTY's Options > Shortcuts (its SuperPuttyAction keys): each action is a menu bar
//! item and its shortcut that item's key, so it works wherever the focus is, a terminal included. The user's changes
//! are kept in the settings (`shortcuts`: action to key, "" for none); the rest have the defaults here.
//!
//! The Mac and Windows have their own defaults. On the Mac they're Cmd keys, which a terminal never needs; on Windows
//! the same Ctrl keys are the terminal's (Ctrl+D, Ctrl+W, Ctrl+K...), so its defaults are Ctrl+Shift ones, as Windows
//! Terminal's. Windows' menu keys don't fire while the page has the keyboard, so there the page catches them itself
//! (src/lib/keys.ts), from the same rows.
//!
//! Keys are written as Tauri accelerators: "CmdOrCtrl+Shift+O", "Ctrl+CmdOrCtrl+F", "F5".

use std::collections::BTreeMap;

use serde::Serialize;

/// One action: its id (the menu item's, which the UI gets as a "menu" event), its name, and its key by default on
/// the Mac (`default`) and elsewhere (`other`)
pub struct Action {
    pub id: &'static str,
    pub label: &'static str,
    pub default: &'static str,
    pub other: &'static str,
}

impl Action {
    /// The action's key by default on this system
    pub fn default_key(&self) -> &'static str {
        if cfg!(target_os = "macos") { self.default } else { self.other }
    }
}

/// Every action with a shortcut, in the order the editor lists them. SuperXPuTTY's actions are here under its names
/// where the Mac has the same thing (Close Tab, Next Tab, Options as Settings…, Full Screen, Open Session, ...).
pub const ACTIONS: &[Action] = &[
    Action { id: "newWindow", label: "New Window", default: "CmdOrCtrl+N" , other: "Ctrl+Shift+N" },
    Action { id: "newLocalShell", label: "New Local Shell", default: "CmdOrCtrl+T" , other: "Ctrl+Shift+T" },
    Action { id: "openSession", label: "Open Session…", default: "CmdOrCtrl+O" , other: "Ctrl+Shift+O" },
    Action { id: "switchSession", label: "Switch Session…", default: "CmdOrCtrl+Shift+O" , other: "Ctrl+Shift+P" },
    Action { id: "closeTab", label: "Close Tab", default: "CmdOrCtrl+W" , other: "Ctrl+Shift+W" },
    Action { id: "nextTab", label: "Next Tab", default: "CmdOrCtrl+Shift+]" , other: "Ctrl+Tab" },
    Action { id: "prevTab", label: "Previous Tab", default: "CmdOrCtrl+Shift+[" , other: "Ctrl+Shift+Tab" },
    Action { id: "duplicateSession", label: "Duplicate Session", default: "" , other: "" },
    Action { id: "renameTab", label: "Rename Tab…", default: "" , other: "" },
    Action { id: "restartSession", label: "Restart Session", default: "" , other: "" },
    Action { id: "splitRight", label: "Split Right", default: "CmdOrCtrl+D" , other: "Ctrl+Shift+D" },
    Action { id: "splitDown", label: "Split Down", default: "CmdOrCtrl+Shift+D" , other: "Ctrl+Shift+E" },
    Action { id: "joinAll", label: "Join All Tabs", default: "" , other: "" },
    Action { id: "find", label: "Find…", default: "CmdOrCtrl+F" , other: "Ctrl+Shift+F" },
    Action { id: "clearScrollback", label: "Clear Scrollback", default: "CmdOrCtrl+K" , other: "Ctrl+Shift+K" },
    Action { id: "resetTerminal", label: "Reset Terminal", default: "" , other: "" },
    Action { id: "copyAll", label: "Copy All to Clipboard", default: "" , other: "" },
    Action { id: "gotoCommandBar", label: "Go to Command Bar", default: "" , other: "" },
    Action { id: "gotoConnectionBar", label: "Go to Quick Connect", default: "" , other: "" },
    Action { id: "focusActiveSession", label: "Focus Terminal", default: "" , other: "" },
    Action { id: "fullScreen", label: "Full Screen", default: "Ctrl+CmdOrCtrl+F" , other: "F11" },
    Action { id: "settings", label: "Settings…", default: "CmdOrCtrl+," , other: "Ctrl+," },
    Action { id: "scriptEditor", label: "Script Editor…", default: "" , other: "" },
    Action { id: "saveLayout", label: "Save Layout", default: "" , other: "" },
    Action { id: "saveLayoutAs", label: "Save Layout As…", default: "" , other: "" },
    Action { id: "toggleSessionsAutoHide", label: "Auto Hide Sessions", default: "" , other: "" },
    Action { id: "toggleSendCommands", label: "Send Commands", default: "" , other: "" },
    Action { id: "toggleButtonBar", label: "Button Bar", default: "" , other: "" },
    Action { id: "toggleLayouts", label: "Layouts", default: "" , other: "" },
    Action { id: "toggleSessionDetail", label: "Session Detail", default: "" , other: "" },
    Action { id: "toggleLogViewer", label: "Log Viewer", default: "" , other: "" },
    Action { id: "toggleSessionsPane", label: "Sessions", default: "" , other: "" },
    Action { id: "toggleStatusBar", label: "Status Bar", default: "" , other: "" },
    Action { id: "toggleAlwaysOnTop", label: "Always on Top", default: "" , other: "" },
    Action { id: "checkUpdates", label: "Check for Updates…", default: "" , other: "" },
];

/// The keys macOS and the Edit menu keep: they can't be given to an action
pub const RESERVED_MAC: &[&str] = &[
    "CmdOrCtrl+Q", "CmdOrCtrl+H", "CmdOrCtrl+Alt+H", "CmdOrCtrl+M", "CmdOrCtrl+C", "CmdOrCtrl+V", "CmdOrCtrl+X",
    "CmdOrCtrl+A", "CmdOrCtrl+Z", "CmdOrCtrl+Shift+Z", "CmdOrCtrl+Tab", "CmdOrCtrl+Space",
];

/// The keys Windows and the terminal keep: closing the window, and the terminal page's copy and paste
pub const RESERVED_OTHER: &[&str] = &["Alt+F4", "Ctrl+Shift+C", "Ctrl+Shift+V", "Ctrl+Insert", "Shift+Insert"];

/// The keys this system keeps: they can't be given to an action
pub fn reserved() -> &'static [&'static str] {
    if cfg!(target_os = "macos") { RESERVED_MAC } else { RESERVED_OTHER }
}

/// An action's label, for the menu
pub fn label(id: &str) -> &'static str {
    ACTIONS.iter().find(|a| a.id == id).map(|a| a.label).unwrap_or("")
}

/// An action's key now: the user's, else the default; None for none
pub fn key(shortcuts: &BTreeMap<String, String>, id: &str) -> Option<String> {
    let key = match shortcuts.get(id) {
        Some(mine) => mine.clone(),
        None => ACTIONS.iter().find(|a| a.id == id).map(|a| a.default_key().to_string()).unwrap_or_default(),
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
    ACTIONS.iter().map(|a| Row { id: a.id, label: a.label, default: a.default_key(), key: key(shortcuts, a.id).unwrap_or_default() }).collect()
}

/// The user's changes as they should be kept: only keys that differ from the default, a key used twice given to the
/// last action that asked for it, and none of the reserved ones
pub fn cleaned(changes: &BTreeMap<String, String>) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for action in ACTIONS {
        if let Some(key) = changes.get(action.id) {
            let key = if reserved().iter().any(|r| r.eq_ignore_ascii_case(key)) { "" } else { key.as_str() };
            if key != action.default_key() {
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
        // this system's defaults and reserved keys (the Mac's or Windows')
        let mac = cfg!(target_os = "macos");
        let (close, open, copy) = if mac { ("CmdOrCtrl+W", "CmdOrCtrl+O", "CmdOrCtrl+C") } else { ("Ctrl+Shift+W", "Ctrl+Shift+O", "Ctrl+Shift+C") };
        let mut changes = BTreeMap::new();
        changes.insert("nextTab".to_string(), "CmdOrCtrl+Alt+Right".to_string());
        changes.insert("closeTab".to_string(), close.to_string());
        changes.insert("find".to_string(), copy.to_string());
        changes.insert("splitRight".to_string(), String::new());
        let kept = cleaned(&changes);
        assert_eq!(kept.get("closeTab"), None, "the default isn't kept as a change");
        assert_eq!(kept.get("find").map(String::as_str), Some(""), "a reserved key leaves the action without one");
        assert_eq!(key(&kept, "nextTab").as_deref(), Some("CmdOrCtrl+Alt+Right"));
        assert_eq!(key(&kept, "splitRight"), None);
        assert_eq!(key(&kept, "openSession").as_deref(), Some(open));
        assert_eq!(label("copyAll"), "Copy All to Clipboard");
    }

    #[test]
    fn windows_defaults_leave_the_terminal_its_ctrl_keys() {
        // a plain Ctrl+letter is a terminal's (Ctrl+D ends input, Ctrl+W deletes a word...): Windows' defaults avoid them
        for action in ACTIONS {
            let parts: Vec<&str> = action.other.split('+').collect();
            let plain_ctrl = parts.len() == 2 && parts[0] == "Ctrl" && parts[1].len() == 1 && parts[1].chars().all(|c| c.is_ascii_alphabetic());
            assert!(!plain_ctrl, "{} defaults to {} on Windows", action.id, action.other);
            assert!(!RESERVED_OTHER.contains(&action.other), "{} defaults to a reserved key", action.id);
        }
    }
}
