//! The keyboard shortcut that shows or hides the strip, written in `strip.json` as `ctrl+alt+w`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Hotkey {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub win: bool,
    /// A virtual-key code: the letter or digit itself (`'A'`..`'Z'`, `'0'`..`'9'`), or `0x70 + n - 1` for Fn.
    pub key: u32,
}

const VK_F1: u32 = 0x70;

impl Hotkey {
    pub const DEFAULT: Hotkey = Hotkey { ctrl: true, alt: true, shift: false, win: false, key: b'W' as u32 };

    /// Modifiers and one key, joined by `+` in any order and case. A key alone is refused: it would
    /// take that key away from every other program.
    pub fn parse(text: &str) -> Option<Hotkey> {
        let mut hotkey = Hotkey { ctrl: false, alt: false, shift: false, win: false, key: 0 };
        for part in text.split('+').map(|p| p.trim().to_ascii_lowercase()) {
            match part.as_str() {
                "ctrl" | "control" => hotkey.ctrl = true,
                "alt" => hotkey.alt = true,
                "shift" => hotkey.shift = true,
                "win" | "super" => hotkey.win = true,
                key if hotkey.key == 0 => hotkey.key = key_code(key)?,
                _ => return None, // two keys
            }
        }
        (hotkey.key != 0 && (hotkey.ctrl || hotkey.alt || hotkey.shift || hotkey.win)).then_some(hotkey)
    }

    /// As `parse` reads it back.
    pub fn name(&self) -> String {
        self.parts(true)
    }

    /// As people write it: `Ctrl+Alt+W`.
    pub fn label(&self) -> String {
        self.parts(false)
    }

    fn parts(&self, lower: bool) -> String {
        let mut parts: Vec<String> = Vec::new();
        for (on, word) in [(self.ctrl, "Ctrl"), (self.alt, "Alt"), (self.shift, "Shift"), (self.win, "Win")] {
            if on {
                parts.push(word.to_owned());
            }
        }
        parts.push(match self.key {
            k @ 0x70..=0x7b => format!("F{}", k - VK_F1 + 1),
            k => char::from_u32(k).unwrap_or('?').to_string(),
        });
        let joined = parts.join("+");
        if lower { joined.to_ascii_lowercase() } else { joined }
    }

    /// `MOD_ALT | MOD_CONTROL | MOD_SHIFT | MOD_WIN`, plus `MOD_NOREPEAT` so holding the keys down is one press.
    pub fn modifiers(&self) -> u32 {
        const MOD_ALT: u32 = 0x1;
        const MOD_CONTROL: u32 = 0x2;
        const MOD_SHIFT: u32 = 0x4;
        const MOD_WIN: u32 = 0x8;
        const MOD_NOREPEAT: u32 = 0x4000;
        MOD_NOREPEAT
            | if self.alt { MOD_ALT } else { 0 }
            | if self.ctrl { MOD_CONTROL } else { 0 }
            | if self.shift { MOD_SHIFT } else { 0 }
            | if self.win { MOD_WIN } else { 0 }
    }
}

fn key_code(key: &str) -> Option<u32> {
    let mut chars = key.chars();
    let first = chars.next()?;
    if chars.next().is_none() {
        return match first {
            'a'..='z' => Some(first.to_ascii_uppercase() as u32),
            '0'..='9' => Some(first as u32),
            _ => None,
        };
    }
    let number: u32 = key.strip_prefix('f')?.parse().ok()?;
    (1..=12).contains(&number).then(|| VK_F1 + number - 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_modifiers_and_a_key_in_any_order_and_case() {
        assert_eq!(Hotkey::parse("ctrl+alt+w"), Some(Hotkey::DEFAULT));
        assert_eq!(Hotkey::parse(" W + Alt + CTRL "), Some(Hotkey::DEFAULT));
        let f = Hotkey::parse("win+shift+f9").unwrap();
        assert_eq!((f.win, f.shift, f.ctrl, f.key), (true, true, false, 0x78));
        assert_eq!(Hotkey::parse("ctrl+7").unwrap().key, u32::from(b'7'));
    }

    #[test]
    fn refuses_what_would_take_a_key_from_everyone() {
        assert_eq!(Hotkey::parse("w"), None);
        assert_eq!(Hotkey::parse("f5"), None);
        assert_eq!(Hotkey::parse("ctrl+alt"), None);
        assert_eq!(Hotkey::parse("ctrl+a+b"), None);
        assert_eq!(Hotkey::parse("ctrl+f13"), None);
        assert_eq!(Hotkey::parse("ctrl+ñ"), None);
        assert_eq!(Hotkey::parse(""), None);
    }

    #[test]
    fn names_read_back_and_labels_read_well() {
        for text in ["ctrl+alt+w", "shift+f1", "ctrl+alt+shift+win+0", "alt+f12"] {
            assert_eq!(Hotkey::parse(text).unwrap().name(), text);
        }
        assert_eq!(Hotkey::DEFAULT.label(), "Ctrl+Alt+W");
        assert_eq!(Hotkey::parse("shift+f3").unwrap().label(), "Shift+F3");
    }

    #[test]
    fn modifiers_are_the_ones_windows_expects() {
        assert_eq!(Hotkey::DEFAULT.modifiers(), 0x4000 | 0x1 | 0x2);
        assert_eq!(Hotkey::parse("win+shift+a").unwrap().modifiers(), 0x4000 | 0x4 | 0x8);
    }
}
