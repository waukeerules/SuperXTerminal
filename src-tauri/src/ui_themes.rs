//! The window's own themes, beside Light and Dark (Settings > General > Look > Theme): a name, the built-in theme it
//! starts from (light or dark, for any colour it leaves alone), and its colours, by the names the window's styles use
//! (`bgWindow` for `--bg-window`...; `src/lib/settings/uiThemes.svelte.ts` lists them). The terminal's own themes are
//! separate (Settings > Terminal).
//!
//! They're `UiThemes.json` in the user's own folder (`sessions::user_folder`, AppData on Windows): this user's, on this
//! computer, never the shared settings folder. Saved whole, by the Edit Themes window's Save.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// The themes' file name in the user's folder
const FILE: &str = "UiThemes.json";

/// One theme
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct UiTheme {
    pub name: String,
    /// "light" or "dark": the built-in theme it starts from, for the colours it doesn't set, and for what follows
    /// the app's darkness (the terminal's Follow the App's Look, scroll bars, the system's own controls)
    pub base: String,
    /// Its colours, "#rrggbb", by the window's names for them; one it doesn't have is the base's
    pub colors: std::collections::BTreeMap<String, String>,
}

/// The file's shape: `{"themes": [...]}`
#[derive(Default, Serialize, Deserialize)]
struct File {
    #[serde(default)]
    themes: Vec<UiTheme>,
}

/// The themes saved; none when there's no file, or one that can't be read
pub fn load(user_folder: &Path) -> Vec<UiTheme> {
    std::fs::read_to_string(user_folder.join(FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<File>(&text).ok())
        .map(|file| file.themes)
        .unwrap_or_default()
}

/// Why a set of themes can't be saved, or None: each needs a name of its own (ignoring case), not a built-in one's,
/// and colours as "#rrggbb"
pub fn problem(themes: &[UiTheme]) -> Option<String> {
    let mut seen = std::collections::HashSet::new();
    for theme in themes {
        let name = theme.name.trim();
        if name.is_empty() {
            return Some("A theme needs a name.".into());
        }
        if ["system", "light", "dark"].contains(&name.to_lowercase().as_str()) {
            return Some(format!("\"{name}\" is a built-in theme's name: give yours another."));
        }
        if !seen.insert(name.to_lowercase()) {
            return Some(format!("There are two themes named \"{name}\"."));
        }
        if let Some((key, value)) = theme.colors.iter().find(|(_, v)| !is_colour(v)) {
            return Some(format!("{name}'s {key} isn't a colour (#rrggbb): {value}"));
        }
    }
    None
}

/// Whether a value is a colour as the themes keep it: "#" and six hex digits
fn is_colour(value: &str) -> bool {
    value.len() == 7 && value.starts_with('#') && value[1..].chars().all(|c| c.is_ascii_hexdigit())
}

/// Save the themes, whole (saving.rs: tried again while antivirus holds the file), names trimmed and a base other
/// than "dark" kept as "light"
pub fn save(user_folder: &Path, themes: &[UiTheme]) -> Result<Vec<UiTheme>, String> {
    if let Some(problem) = problem(themes) {
        return Err(problem);
    }
    let themes: Vec<UiTheme> = themes
        .iter()
        .map(|t| UiTheme { name: t.name.trim().to_string(), base: if t.base == "dark" { "dark".into() } else { "light".into() }, colors: t.colors.clone() })
        .collect();
    std::fs::create_dir_all(user_folder).map_err(|e| e.to_string())?;
    let text = serde_json::to_string_pretty(&File { themes: themes.clone() }).map_err(|e| e.to_string())?;
    crate::saving::save(&user_folder.join(FILE), "json.saving", text)?;
    Ok(themes)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A theme of this name and base, with these colours
    fn theme(name: &str, base: &str, colors: &[(&str, &str)]) -> UiTheme {
        UiTheme { name: name.into(), base: base.into(), colors: colors.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
    }

    #[test]
    fn themes_are_saved_in_the_users_folder_and_read_back() {
        let folder = std::env::temp_dir().join(format!("sxt-ui-themes-{}", std::process::id()));
        assert_eq!(load(&folder), vec![]);
        let saved = save(&folder, &[theme(" Ocean ", "dark", &[("accent", "#1e90ff"), ("bgWindow", "#0b1622")]), theme("Paper", "", &[])]).unwrap();
        assert_eq!(saved[0].name, "Ocean");
        assert_eq!(saved[1].base, "light");
        assert_eq!(load(&folder), saved);
        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[test]
    fn a_set_with_a_nameless_built_in_twice_named_or_bad_colour_theme_isnt_saved() {
        assert_eq!(problem(&[theme("", "dark", &[])]).as_deref(), Some("A theme needs a name."));
        assert!(problem(&[theme("Dark", "dark", &[])]).unwrap().contains("built-in"));
        assert!(problem(&[theme("Ocean", "dark", &[]), theme("ocean", "light", &[])]).unwrap().contains("two themes"));
        assert!(problem(&[theme("Ocean", "dark", &[("accent", "blue")])]).unwrap().contains("isn't a colour"));
        assert_eq!(problem(&[theme("Ocean", "dark", &[("accent", "#1E90ff")])]), None);
    }
}
