//! Character aliases are normalized without turning function or keypad keysyms into text.
pub(super) fn keysym_character(sym: u32) -> Option<char> {
    if sym >= 0xfe00 && !(0x01000000..=0x0110ffff).contains(&sym) {
        return None;
    }
    xkeysym::Keysym::new(sym)
        .key_char()
        .filter(|ch| !ch.is_control())
}
pub(super) fn canonical_keysym(sym: u32) -> u32 {
    keysym_character(sym).map_or(sym, |ch| 0x01000000 | u32::from(ch))
}
#[cfg(any(windows, test))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CharacterPlan {
    pub code: u16,
    pub unicode: bool,
    pub required: u8,
}
#[cfg(any(windows, test))]
pub(super) fn windows_character(
    sym: u32,
    shortcut: bool,
    lookup: impl FnOnce(u16) -> i16,
) -> Option<CharacterPlan> {
    let value = u16::try_from(u32::from(keysym_character(sym)?)).ok()?;
    if !shortcut {
        return Some(CharacterPlan {
            code: value,
            unicode: true,
            required: 0,
        });
    }
    let mapping = lookup(value);
    if mapping == -1 {
        return None;
    }
    let mapping = mapping as u16;
    let required = (mapping >> 8) as u8;
    if required & !7 != 0 {
        return None;
    }
    Some(CharacterPlan {
        code: mapping & 255,
        unicode: false,
        required,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn character_aliases_share_identity_without_converting_function_keys() {
        for (legacy, unicode, character) in [(0x20ac, 0x010020ac, '€'), (0x07e1, 0x010003b1, 'α')]
        {
            assert_eq!(keysym_character(legacy), Some(character));
            assert_eq!(keysym_character(unicode), Some(character));
            assert_eq!(canonical_keysym(legacy), canonical_keysym(unicode));
        }
        for special in [0xff08, 0xff0d, 0xff51, 0xffe1, 0xfe03, 0xffb1, 0x0100d800] {
            assert_eq!(keysym_character(special), None);
            assert_eq!(canonical_keysym(special), special);
        }
    }
    #[test]
    fn windows_case_and_shifted_punctuation_use_literal_unicode_without_shortcuts() {
        for sym in [
            u32::from(b'A'),
            u32::from(b'#'),
            u32::from(b'a'),
            0x20ac,
            0x010020ac,
            0x07e1,
            0x010003b1,
        ] {
            let p = windows_character(sym, false, |_| {
                panic!("literal text must not depend on CapsLock, Shift or the target layout")
            })
            .unwrap();
            assert!(p.unicode);
            assert_eq!(u32::from(p.code), u32::from(keysym_character(sym).unwrap()));
            assert_eq!(p.required, 0);
        }
    }
    #[test]
    fn windows_shortcut_mapping_retains_shift_and_altgr_requirements() {
        for (sym, mapping, code, required) in [
            (b'A' as u32, 0x0141, 0x41, 1),
            (b'#' as u32, 0x0133, 0x33, 1),
            (0x010020ac, 0x0645, 0x45, 6),
            (0x20ac, 0x0645, 0x45, 6),
            (0x07e1, 0x0041, 0x41, 0),
            (0x010003b1, 0x0041, 0x41, 0),
        ] {
            assert_eq!(
                windows_character(sym, true, |value| {
                    assert_eq!(u32::from(value), u32::from(keysym_character(sym).unwrap()));
                    mapping
                }),
                Some(CharacterPlan {
                    code,
                    unicode: false,
                    required
                })
            );
        }
        assert!(windows_character(b'a' as u32, true, |_| -1).is_none());
        assert!(windows_character(b'a' as u32, true, |_| 0x0841).is_none());
    }
}
