//! Remote Desktop options, as SuperXPuTTY's RdpOptions: the defaults (Settings > Remote Desktop, its Options > RDP)
//! with a session's own (its `RdpOptions` attribute, Edit Session's RDP options…) laid over them, in its format,
//! "Key=Value;Key=Value", so a session shared with SuperXPuTTY keeps them both ways. `Retina` is the Mac's own;
//! SuperXPuTTY skips keys it doesn't know.

use serde::{Deserialize, Serialize};

/// Every option; None isn't set (a session's then come from the defaults)
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RdpOptions {
    /// "FitTab" or "Fixed"
    pub display: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub color_depth: Option<u32>,
    pub smart_sizing: Option<bool>,
    pub clipboard: Option<bool>,
    pub drives: Option<bool>,
    pub printers: Option<bool>,
    pub smart_cards: Option<bool>,
    pub microphone: Option<bool>,
    /// 0 play on this computer, 1 on the remote computer, 2 don't play
    pub audio: Option<u32>,
    /// Windows key combinations: 0 on this computer, 1 on the remote computer, 2 only full screen
    pub keyboard: Option<u32>,
    /// Server authentication: 0 connect anyway, 1 don't connect, 2 warn
    pub authentication: Option<u32>,
    pub gateway: Option<String>,
    pub console: Option<bool>,
    /// The desktop at the screen's full Retina resolution (the Mac's own)
    pub retina: Option<bool>,
}

impl RdpOptions {
    /// SuperXPuTTY's defaults (its Settings.settings), except Windows key combinations on the remote computer (⌘ and
    /// a key as Windows and it whenever the tab has the keyboard, as asked for on the Mac), and Retina off
    pub fn defaults() -> RdpOptions {
        RdpOptions {
            display: Some("FitTab".into()),
            width: Some(1920),
            height: Some(1080),
            color_depth: Some(32),
            smart_sizing: Some(true),
            clipboard: Some(true),
            drives: Some(false),
            printers: Some(false),
            smart_cards: Some(false),
            microphone: Some(false),
            audio: Some(0),
            keyboard: Some(1),
            authentication: Some(2),
            gateway: Some(String::new()),
            console: Some(false),
            retina: Some(false),
        }
    }

    /// Read "Key=Value;Key=Value" (unknown keys and bad values are skipped, as RdpOptions.Parse)
    pub fn parse(text: &str) -> RdpOptions {
        let mut o = RdpOptions::default();
        let int = |v: &str| v.parse::<u32>().ok();
        let flag = |v: &str| match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" => Some(true),
            "0" | "false" | "no" => Some(false),
            _ => None,
        };
        for pair in text.split(';') {
            let Some((key, value)) = pair.split_once('=') else { continue };
            let (key, value) = (key.trim(), value.trim());
            match key {
                "Display" => o.display = Some(if value == "Fixed" { "Fixed" } else { "FitTab" }.into()),
                "Width" => o.width = int(value),
                "Height" => o.height = int(value),
                "ColorDepth" => o.color_depth = int(value),
                "SmartSizing" => o.smart_sizing = flag(value),
                "Clipboard" => o.clipboard = flag(value),
                "Drives" => o.drives = flag(value),
                "Printers" => o.printers = flag(value),
                "SmartCards" => o.smart_cards = flag(value),
                "Microphone" => o.microphone = flag(value),
                "Audio" => o.audio = int(value),
                "Keyboard" => o.keyboard = int(value),
                "Authentication" => o.authentication = int(value),
                "Gateway" => o.gateway = Some(value.to_string()),
                "Console" => o.console = flag(value),
                "Retina" => o.retina = flag(value),
                _ => {}
            }
        }
        o
    }

    /// Write the options that are set, in SuperXPuTTY's order ("" when none is); the UI writes its own (rdpOptions.ts)
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn format(&self) -> String {
        let mut pairs = Vec::new();
        let mut add = |key: &str, value: Option<String>| {
            if let Some(v) = value {
                // ; and = separate the pairs, so they can't be part of a value
                pairs.push(format!("{key}={}", v.replace([';', '='], "")));
            }
        };
        let b = |v: Option<bool>| v.map(|v| if v { "1" } else { "0" }.to_string());
        let n = |v: Option<u32>| v.map(|v| v.to_string());
        add("Display", self.display.clone());
        add("Width", n(self.width));
        add("Height", n(self.height));
        add("ColorDepth", n(self.color_depth));
        add("SmartSizing", b(self.smart_sizing));
        add("Clipboard", b(self.clipboard));
        add("Drives", b(self.drives));
        add("Printers", b(self.printers));
        add("SmartCards", b(self.smart_cards));
        add("Microphone", b(self.microphone));
        add("Audio", n(self.audio));
        add("Keyboard", n(self.keyboard));
        add("Authentication", n(self.authentication));
        add("Gateway", self.gateway.clone());
        add("Console", b(self.console));
        add("Retina", b(self.retina));
        pairs.join(";")
    }

    /// These options with `over`'s set ones in their place (RdpOptions.Overlay)
    pub fn overlay(&self, over: &RdpOptions) -> RdpOptions {
        RdpOptions {
            display: over.display.clone().or_else(|| self.display.clone()),
            width: over.width.or(self.width),
            height: over.height.or(self.height),
            color_depth: over.color_depth.or(self.color_depth),
            smart_sizing: over.smart_sizing.or(self.smart_sizing),
            clipboard: over.clipboard.or(self.clipboard),
            drives: over.drives.or(self.drives),
            printers: over.printers.or(self.printers),
            smart_cards: over.smart_cards.or(self.smart_cards),
            microphone: over.microphone.or(self.microphone),
            audio: over.audio.or(self.audio),
            keyboard: over.keyboard.or(self.keyboard),
            authentication: over.authentication.or(self.authentication),
            gateway: over.gateway.clone().or_else(|| self.gateway.clone()),
            console: over.console.or(self.console),
            retina: over.retina.or(self.retina),
        }
    }

    /// The options a session connects with: SuperXPuTTY's defaults, the user's (Settings), the session's
    pub fn effective(user_defaults: &str, session: &str) -> RdpOptions {
        RdpOptions::defaults().overlay(&RdpOptions::parse(user_defaults)).overlay(&RdpOptions::parse(session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_superxputty_options() {
        let text = "Display=Fixed;Width=1280;Height=800;Clipboard=0;Audio=2;Gateway=rdg.corp:443;Unknown=x";
        let o = RdpOptions::parse(text);
        assert_eq!((o.display.as_deref(), o.width, o.clipboard, o.audio), (Some("Fixed"), Some(1280), Some(false), Some(2)));
        assert_eq!(o.format(), "Display=Fixed;Width=1280;Height=800;Clipboard=0;Audio=2;Gateway=rdg.corp:443");
        let e = RdpOptions::effective("ColorDepth=16", text);
        assert_eq!((e.color_depth, e.smart_sizing, e.clipboard), (Some(16), Some(true), Some(false)));
        assert_eq!(RdpOptions::parse(&RdpOptions::defaults().format()), RdpOptions::defaults());
    }
}
