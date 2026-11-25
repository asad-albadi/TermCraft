// Explicit US physical key map: unsupported keys fail visibly rather than inject a wrong key.
pub(super) fn keycode(sym: u32) -> Option<u16> {
    let c = super::super::keyboard::keysym_character(sym);
    if let Some(ch) = c {
        let ch = ch.to_ascii_lowercase();
        // Carbon virtual key positions for the ANSI alphabet and digits.
        return Some(match ch {
            'a' => 0,
            's' => 1,
            'd' => 2,
            'f' => 3,
            'h' => 4,
            'g' => 5,
            'z' => 6,
            'x' => 7,
            'c' => 8,
            'v' => 9,
            'b' => 11,
            'q' => 12,
            'w' => 13,
            'e' => 14,
            'r' => 15,
            'y' => 16,
            't' => 17,
            '1' => 18,
            '2' => 19,
            '3' => 20,
            '4' => 21,
            '6' => 22,
            '5' => 23,
            '=' => 24,
            '9' => 25,
            '7' => 26,
            '-' => 27,
            '8' => 28,
            '0' => 29,
            ']' => 30,
            'o' => 31,
            'u' => 32,
            '[' => 33,
            'i' => 34,
            'p' => 35,
            'l' => 37,
            'j' => 38,
            '\'' => 39,
            'k' => 40,
            ';' => 41,
            '\\' => 42,
            ',' => 43,
            '/' => 44,
            'n' => 45,
            'm' => 46,
            '.' => 47,
            ' ' => 49,
            '`' => 50,
            _ => return None,
        });
    }
    Some(match sym {
        0xff0d => 36,
        0xff09 => 48,
        0xff08 => 51,
        0xff1b => 53,
        0xffe7 | 0xffeb => 55,
        0xffe8 | 0xffec => 54,
        0xffe1 => 56,
        0xffe2 => 60,
        0xffe9 => 58,
        0xffea => 61,
        0xffe3 => 59,
        0xffe4 => 62,
        0xff51 => 123,
        0xff53 => 124,
        0xff54 => 125,
        0xff52 => 126,
        0xffbe => 122,
        0xffbf => 120,
        0xffc0 => 99,
        0xffc1 => 118,
        0xffc2 => 96,
        0xffc3 => 97,
        0xffc4 => 98,
        0xffc5 => 100,
        0xffc6 => 101,
        0xffc7 => 109,
        0xffc8 => 103,
        0xffc9 => 111,
        0xffff => 117,
        0xff50 => 115,
        0xff57 => 119,
        0xff55 => 116,
        0xff56 => 121,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ansi_shortcuts_and_special_keys_are_explicit() {
        assert_eq!(keycode(0x01000061), Some(0));
        assert_eq!(keycode(0x01000041), Some(0));
        assert_eq!(keycode(0xffe1), Some(56));
        assert_eq!(keycode(0xffe2), Some(60));
        assert_eq!(keycode(0xffeb), Some(55));
        assert_eq!(keycode(0xff51), Some(123));
        assert_eq!(keycode(0xffbe), Some(122));
        assert_eq!(keycode(0x010020ac), None);
    }
}

// Returns shared modifier flag and opposite-side virtual key, preserving siblings.
pub(super) fn modifier_group(code: u16) -> Option<(u64, u16)> {
    Some(match code {
        56 => (1 << 17, 60),
        60 => (1 << 17, 56),
        59 => (1 << 18, 62),
        62 => (1 << 18, 59),
        58 => (1 << 19, 61),
        61 => (1 << 19, 58),
        55 => (1 << 20, 54),
        54 => (1 << 20, 55),
        _ => return None,
    })
}
pub(super) fn event_flags(
    current: u64,
    code: u16,
    down: bool,
    literal: bool,
    sibling_held: bool,
) -> u64 {
    if literal {
        return 0;
    }
    if let Some((mask, _)) = modifier_group(code) {
        if down || sibling_held {
            return current | mask;
        }
        return current & !mask;
    }
    current
}
#[cfg(test)]
mod modifier_tests {
    use super::*;
    #[test]
    fn literal_case_and_held_modifier_release_plans() {
        assert_eq!(event_flags(1 << 17, 0, true, true, false), 0);
        assert_eq!(event_flags(0, 56, true, false, false), 1 << 17);
        assert_eq!(
            event_flags((1 << 17) | (1 << 18), 56, false, false, false),
            1 << 18
        );
        assert_eq!(event_flags(1 << 17, 56, false, false, true), 1 << 17);
        assert_eq!(event_flags(1 << 20, 0, true, false, false), 1 << 20);
    }
}
