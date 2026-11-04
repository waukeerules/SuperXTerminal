//! The button bars (`ButtonBars.xml` in the settings folder, shared by the team), as SuperXPuTTY's ButtonBarData and
//! ButtonBarStore: bars of buttons that send a string or run an SPSL script, each bar optionally shown for tabs with
//! certain session icons. Written as .NET's XmlSerializer writes it (`<?xml version="1.0"?>`, CRLF, two-space indent,
//! a button's text with its own LF line breaks) through a temporary file, so either app can read the other's. Bars
//! move between people as SuperXPuTTY's JSON export.

use std::path::{Path, PathBuf};

use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

/// One button
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Button {
    pub label: String,
    #[serde(default)]
    pub tooltip: String,
    /// "#RRGGBB" or a colour name; a filled circle when there's no icon
    #[serde(default)]
    pub color: String,
    /// A session icon key, e.g. "switch#1E63B4"
    #[serde(default)]
    pub icon: String,
    /// "SendString" or "SpslScript"
    pub action: String,
    pub text: String,
}

/// One bar
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bar {
    pub name: String,
    pub buttons: Vec<Button>,
    /// Session icons whose tabs show this bar
    #[serde(default)]
    pub tab_icons: Vec<String>,
}

/// The file's path: in the settings folder, else beside the user's own settings (as ButtonBarStore.FilePath)
pub fn file(settings_folder: Option<&Path>, user_folder: &Path) -> PathBuf {
    settings_folder.unwrap_or(user_folder).join("ButtonBars.xml")
}

/// The bars; one empty "Default" bar when the file is missing, unreadable or has none, as ButtonBarStore.Load
pub fn load(path: &Path) -> Vec<Bar> {
    match std::fs::read_to_string(path).ok().and_then(|t| parse(&t).ok()) {
        Some(bars) if !bars.is_empty() => bars,
        _ => vec![Bar { name: "Default".into(), ..Bar::default() }],
    }
}

/// Read the XML
pub fn parse(text: &str) -> Result<Vec<Bar>, String> {
    let mut reader = Reader::from_str(text.trim_start_matches('\u{feff}'));
    let mut bars: Vec<Bar> = Vec::new();
    // what character data goes to: a button's Text or a tab icon
    let mut collecting: Option<String> = None;
    let mut in_icons = false;
    let attrs = |e: &quick_xml::events::BytesStart| -> Result<Vec<(String, String)>, String> {
        e.attributes()
            .map(|a| {
                let a = a.map_err(|e| e.to_string())?;
                let value = a.normalized_value(quick_xml::XmlVersion::Implicit1_0).map_err(|e| e.to_string())?.to_string();
                Ok((a.key.as_ref().to_string(), value))
            })
            .collect()
    };
    loop {
        match reader.read_event().map_err(|e| e.to_string())? {
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == "ButtonBar" => {
                let name = attrs(&e)?.into_iter().find(|(k, _)| k == "Name").map(|(_, v)| v).unwrap_or_default();
                bars.push(Bar { name, ..Bar::default() });
            }
            Event::Start(e) | Event::Empty(e) if e.name().as_ref() == "Button" => {
                let mut button = Button { action: "SendString".into(), ..Button::default() };
                for (k, v) in attrs(&e)? {
                    match k.as_str() {
                        "Label" => button.label = v,
                        "Tooltip" => button.tooltip = v,
                        "Color" => button.color = v,
                        "ImageKey" => button.icon = v,
                        "Action" => button.action = v,
                        _ => {}
                    }
                }
                if let Some(bar) = bars.last_mut() {
                    bar.buttons.push(button);
                }
            }
            Event::Start(e) if e.name().as_ref() == "Text" => collecting = Some(String::new()),
            Event::Start(e) if e.name().as_ref() == "TabIcons" => in_icons = true,
            Event::End(e) if e.name().as_ref() == "TabIcons" => in_icons = false,
            Event::Start(e) if in_icons && e.name().as_ref() == "Icon" => collecting = Some(String::new()),
            Event::Text(t) => {
                if let Some(text) = collecting.as_mut() {
                    text.push_str(&t.xml_content(quick_xml::XmlVersion::Implicit1_0));
                }
            }
            Event::GeneralRef(r) => {
                if let Some(text) = collecting.as_mut() {
                    match r.resolve_char_ref().map_err(|e| e.to_string())? {
                        Some(c) => text.push(c),
                        None => text.push_str(&entity(&r.xml_content(quick_xml::XmlVersion::Implicit1_0))),
                    }
                }
            }
            Event::End(e) if e.name().as_ref() == "Text" => {
                let text = collecting.take().unwrap_or_default();
                if let Some(button) = bars.last_mut().and_then(|b| b.buttons.last_mut()) {
                    button.text = text;
                }
            }
            Event::End(e) if e.name().as_ref() == "Icon" => {
                let icon = collecting.take().unwrap_or_default();
                if let Some(bar) = bars.last_mut() {
                    bar.tab_icons.push(icon);
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(bars)
}

/// An XML entity's text: amp, lt, gt, quot, apos, or a character reference
fn entity(name: &str) -> String {
    match name {
        "amp" => "&".into(),
        "lt" => "<".into(),
        "gt" => ">".into(),
        "quot" => "\"".into(),
        "apos" => "'".into(),
        _ => {
            let code = name
                .strip_prefix("#x")
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| name.strip_prefix('#').and_then(|d| d.parse().ok()));
            code.and_then(char::from_u32).map(String::from).unwrap_or_else(|| format!("&{name};"))
        }
    }
}

/// Element text as XmlSerializer writes it: & < > escaped, line breaks as they are
fn escape_text(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// The XML, as XmlSerializer writes it
pub fn to_xml(bars: &[Bar]) -> String {
    let attr = crate::sessions::escape;
    let mut out = String::from("<?xml version=\"1.0\"?>\r\n");
    out.push_str("<ButtonBars xmlns:xsd=\"http://www.w3.org/2001/XMLSchema\" xmlns:xsi=\"http://www.w3.org/2001/XMLSchema-instance\">\r\n");
    for bar in bars {
        out.push_str(&format!("  <ButtonBar Name=\"{}\">\r\n", attr(&bar.name)));
        if bar.buttons.is_empty() {
            out.push_str("    <Buttons />\r\n");
        } else {
            out.push_str("    <Buttons>\r\n");
            for b in &bar.buttons {
                out.push_str(&format!("      <Button Label=\"{}\"", attr(&b.label)));
                for (name, value) in [("Tooltip", &b.tooltip), ("Color", &b.color), ("ImageKey", &b.icon)] {
                    if !value.is_empty() {
                        out.push_str(&format!(" {name}=\"{}\"", attr(value)));
                    }
                }
                let action = if b.action == "SpslScript" { "SpslScript" } else { "SendString" };
                out.push_str(&format!(" Action=\"{action}\">\r\n"));
                out.push_str(&format!("        <Text>{}</Text>\r\n", escape_text(&b.text)));
                out.push_str("      </Button>\r\n");
            }
            out.push_str("    </Buttons>\r\n");
        }
        if !bar.tab_icons.is_empty() {
            out.push_str("    <TabIcons>\r\n");
            for icon in &bar.tab_icons {
                out.push_str(&format!("      <Icon>{}</Icon>\r\n", escape_text(icon)));
            }
            out.push_str("    </TabIcons>\r\n");
        }
        out.push_str("  </ButtonBar>\r\n");
    }
    out.push_str("</ButtonBars>");
    out
}

/// Save through a temporary file, as ButtonBarStore.Save; no backup is kept
pub fn save(path: &Path, bars: &[Bar]) -> Result<(), String> {
    let temp = path.with_extension("xml.tmp");
    let explain = |e: std::io::Error| format!("Couldn't save {}: {e}. The settings folder may be read-only.", path.display());
    std::fs::write(&temp, to_xml(bars)).map_err(explain)?;
    std::fs::rename(&temp, path).map_err(explain)
}

/// SuperXPuTTY's export file
#[derive(Serialize, Deserialize)]
struct ExportFile {
    format: String,
    version: u32,
    bars: Vec<ExportBar>,
}

/// One bar in SuperXPuTTY's export JSON (camelCase, as it writes it)
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportBar {
    #[serde(default)]
    name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tab_icons: Vec<String>,
    #[serde(default)]
    buttons: Vec<ExportButton>,
}

/// One button in SuperXPuTTY's export JSON
#[derive(Serialize, Deserialize)]
struct ExportButton {
    #[serde(default)]
    label: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    tooltip: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    icon: Option<String>,
    #[serde(default)]
    action: String,
    #[serde(default)]
    text: String,
}

/// The export's "format", which SuperXPuTTY checks before it imports a file
const EXPORT_FORMAT: &str = "superxputty-button-bars";

/// One bar as SuperXPuTTY's export JSON
pub fn export(bar: &Bar) -> String {
    let some = |s: &String| (!s.is_empty()).then(|| s.clone());
    let file = ExportFile {
        format: EXPORT_FORMAT.into(),
        version: 1,
        bars: vec![ExportBar {
            name: bar.name.clone(),
            tab_icons: bar.tab_icons.clone(),
            buttons: bar
                .buttons
                .iter()
                .map(|b| ExportButton {
                    label: b.label.clone(),
                    tooltip: some(&b.tooltip),
                    color: some(&b.color),
                    icon: some(&b.icon),
                    action: b.action.clone(),
                    text: b.text.clone(),
                })
                .collect(),
        }],
    };
    serde_json::to_string_pretty(&file).unwrap_or_default()
}

/// Add the bars from an export file, as ButtonBarFile.Import: nameless bars are skipped, an unknown action is
/// SendString, and a clashing name becomes "Name (2)"; returns the last bar's name
pub fn import(bars: &mut Vec<Bar>, json: &str) -> Result<String, String> {
    let file: ExportFile = serde_json::from_str(json).map_err(|_| "That isn't a button bar file.".to_string())?;
    if file.format != EXPORT_FORMAT {
        return Err("That isn't a button bar file.".into());
    }
    let mut last = None;
    for bar in file.bars.into_iter().filter(|b| !b.name.trim().is_empty()) {
        let mut name = bar.name.trim().to_string();
        let mut n = 2;
        while bars.iter().any(|b| b.name.eq_ignore_ascii_case(&name)) {
            name = format!("{} ({n})", bar.name.trim());
            n += 1;
        }
        bars.push(Bar {
            name: name.clone(),
            tab_icons: bar.tab_icons,
            buttons: bar
                .buttons
                .into_iter()
                .map(|b| Button {
                    label: b.label,
                    tooltip: b.tooltip.unwrap_or_default(),
                    color: b.color.unwrap_or_default(),
                    icon: b.icon.unwrap_or_default(),
                    action: if b.action == "SpslScript" { "SpslScript".into() } else { "SendString".into() },
                    text: b.text,
                })
                .collect(),
        });
        last = Some(name);
    }
    last.ok_or_else(|| "The file has no button bars.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_what_it_reads() {
        let bars = vec![
            Bar {
                name: "Default".into(),
                buttons: vec![
                    Button { label: "Show ver".into(), color: "#3FB950".into(), action: "SendString".into(), text: "show version\\r".into(), ..Button::default() },
                    Button { label: "Login & go".into(), icon: "ipod".into(), action: "SpslScript".into(), text: "WAITFOR login:\nSENDLINE <me>".into(), ..Button::default() },
                ],
                tab_icons: vec![],
            },
            Bar { name: "Empty".into(), buttons: vec![], tab_icons: vec!["switch".into()] },
        ];
        let xml = to_xml(&bars);
        assert!(xml.starts_with("<?xml version=\"1.0\"?>\r\n<ButtonBars "));
        assert!(xml.contains("<Text>WAITFOR login:\nSENDLINE &lt;me&gt;</Text>\r\n"));
        assert_eq!(parse(&xml).unwrap(), bars);
        let mut more = bars.clone();
        assert_eq!(import(&mut more, &export(&bars[0])).unwrap(), "Default (2)");
        assert_eq!(more[2].buttons, bars[0].buttons);
    }

    /// The real file from Windows (reference/, git-ignored) reads and writes back byte for byte
    #[test]
    fn round_trips_superxputty_file() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../reference/ButtonBars.xml");
        let Ok(text) = std::fs::read_to_string(&path) else { return };
        let bars = parse(&text).unwrap();
        assert!(!bars.is_empty());
        assert_eq!(to_xml(&bars), text);
    }
}
