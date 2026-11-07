//! A session's remote character set, as PuTTY's Window > Translation > Remote character set: what the device's bytes
//! mean, for one that doesn't speak UTF-8 (an older switch or Unix box sending Latin-1 accents, or CP437's line
//! drawing). Its output is turned into UTF-8 before the tab sees it (the screen, the log and a moved tab's replay all
//! get the same text), and what's typed is turned back into its bytes.
//!
//! Only single-byte sets, each a table of its own, so no decoder state crosses a read's end and no encoding crate is
//! needed: bytes below 0x80 are ASCII in all of them and pass through untouched, escape sequences included.
//!
//! Not for an SSH session on Windows: ssh runs in Windows' pseudo console (ConPTY), which turns the device's bytes into
//! UTF-8 itself before they reach the app, so the original bytes are already gone (pty.rs says so in the tab).

/// A remote character set
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Charset {
    /// UTF-8: bytes go through as they are
    #[default]
    Utf8,
    /// ISO-8859-1, Latin-1: each byte is the code point of the same number
    Latin1,
    /// ISO-8859-15, Latin-9: Latin-1 with the euro sign and eight others in place of rarely used signs
    Latin9,
    /// Windows-1252: Latin-1 with quotes, dashes, the euro sign and others in 0x80-0x9F
    Windows1252,
    /// Code page 437, the IBM PC's: accents, Greek letters, and the box-drawing and shading characters
    Cp437,
}

/// Windows-1252's 0x80-0x9F; the five it leaves undefined stay their own code points, as browsers decode them
const WINDOWS_1252: [char; 32] = [
    '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}',
    '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
];

/// Code page 437's 0x80-0xFF
const CP437: [char; 128] = [
    'Ç', 'ü', 'é', 'â', 'ä', 'à', 'å', 'ç', 'ê', 'ë', 'è', 'ï', 'î', 'ì', 'Ä', 'Å',
    'É', 'æ', 'Æ', 'ô', 'ö', 'ò', 'û', 'ù', 'ÿ', 'Ö', 'Ü', '¢', '£', '¥', '₧', 'ƒ',
    'á', 'í', 'ó', 'ú', 'ñ', 'Ñ', 'ª', 'º', '¿', '⌐', '¬', '½', '¼', '¡', '«', '»',
    '░', '▒', '▓', '│', '┤', '╡', '╢', '╖', '╕', '╣', '║', '╗', '╝', '╜', '╛', '┐',
    '└', '┴', '┬', '├', '─', '┼', '╞', '╟', '╚', '╔', '╩', '╦', '╠', '═', '╬', '╧',
    '╨', '╤', '╥', '╙', '╘', '╒', '╓', '╫', '╪', '┘', '┌', '█', '▄', '▌', '▐', '▀',
    'α', 'ß', 'Γ', 'π', 'Σ', 'σ', 'µ', 'τ', 'Φ', 'Θ', 'Ω', 'δ', '∞', 'φ', 'ε', '∩',
    '≡', '±', '≥', '≤', '⌠', '⌡', '÷', '≈', '°', '∙', '·', '√', 'ⁿ', '²', '■', '\u{a0}',
];

/// Latin-9's eight places that differ from Latin-1, and what they hold
const LATIN9: [(u8, char); 8] = [(0xa4, '€'), (0xa6, 'Š'), (0xa8, 'š'), (0xb4, 'Ž'), (0xb8, 'ž'), (0xbc, 'Œ'), (0xbd, 'œ'), (0xbe, 'Ÿ')];

impl Charset {
    /// The set a profile names ("latin1", "latin9", "windows1252", "cp437"); UTF-8 for "" or anything else
    pub fn named(name: &str) -> Charset {
        match name.trim().to_ascii_lowercase().as_str() {
            "latin1" => Charset::Latin1,
            "latin9" => Charset::Latin9,
            "windows1252" => Charset::Windows1252,
            "cp437" => Charset::Cp437,
            _ => Charset::Utf8,
        }
    }

    /// What it's called in the tab's notices
    pub fn label(self) -> &'static str {
        match self {
            Charset::Utf8 => "UTF-8",
            Charset::Latin1 => "ISO-8859-1 (Latin-1)",
            Charset::Latin9 => "ISO-8859-15 (Latin-9)",
            Charset::Windows1252 => "Windows-1252",
            Charset::Cp437 => "CP437",
        }
    }

    /// The character a byte stands for
    fn char_of(self, byte: u8) -> char {
        match (self, byte) {
            (_, 0..=0x7f) | (Charset::Latin1 | Charset::Utf8, _) => char::from(byte),
            (Charset::Latin9, _) => LATIN9.iter().find(|(b, _)| *b == byte).map_or(char::from(byte), |(_, c)| *c),
            (Charset::Windows1252, 0x80..=0x9f) => WINDOWS_1252[usize::from(byte - 0x80)],
            (Charset::Windows1252, _) => char::from(byte),
            (Charset::Cp437, _) => CP437[usize::from(byte - 0x80)],
        }
    }

    /// The device's output as UTF-8 for the tab; UTF-8 itself goes through untouched
    pub fn decode(self, bytes: &[u8]) -> Vec<u8> {
        if self == Charset::Utf8 {
            return bytes.to_vec();
        }
        let mut text = String::with_capacity(bytes.len() + bytes.len() / 4);
        text.extend(bytes.iter().map(|b| self.char_of(*b)));
        text.into_bytes()
    }

    /// What's typed, as the device's bytes; a character the set doesn't have is sent as "?", as PuTTY does
    pub fn encode(self, text: &str) -> Vec<u8> {
        if self == Charset::Utf8 {
            return text.as_bytes().to_vec();
        }
        text.chars()
            .map(|c| {
                if c.is_ascii() {
                    return c as u8;
                }
                (0x80..=0xffu8).find(|b| self.char_of(*b) == c).unwrap_or(b'?')
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_latin1_device_shows_its_accents_and_gets_them_back_as_it_sends_them() {
        let from_device = b"caf\xe9 \x1b[1mna\xefve\x1b[0m";
        assert_eq!(String::from_utf8(Charset::Latin1.decode(from_device)).unwrap(), "café \x1b[1mnaïve\x1b[0m");
        assert_eq!(Charset::Latin1.encode("café"), b"caf\xe9");
    }

    #[test]
    fn cp437_draws_boxes_and_windows_1252_its_quotes_and_euro() {
        assert_eq!(String::from_utf8(Charset::Cp437.decode(b"\xc9\xcd\xbb\r\n\xc8\xcd\xbc")).unwrap(), "╔═╗\r\n╚═╝");
        assert_eq!(String::from_utf8(Charset::Windows1252.decode(b"\x93ok\x94 \x80 5")).unwrap(), "“ok” € 5");
        assert_eq!(String::from_utf8(Charset::Latin9.decode(b"\xa4")).unwrap(), "€");
        assert_eq!(Charset::Cp437.encode("╔é"), b"\xc9\x82");
    }

    #[test]
    fn utf8_goes_through_and_a_character_the_set_lacks_is_a_question_mark() {
        assert_eq!(Charset::Utf8.decode("é╔".as_bytes()), "é╔".as_bytes());
        assert_eq!(Charset::Latin1.encode("€"), b"?");
        assert_eq!(Charset::named(" CP437 "), Charset::Cp437);
        assert_eq!(Charset::named("koi8-r"), Charset::Utf8);
    }
}
