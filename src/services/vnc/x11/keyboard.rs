//! Translate keysyms through the selected server's current XKB key types and group.
//! Temporary transitions touch only this controller's owned keys and are restored per event.
use super::*;
use x11rb::protocol::xkb::{self, ConnectionExt as _, MapPart, VMod};
const DEVICE: u16 = 256; // XkbUseCoreKbd
pub(super) struct Held {
    pub sym: u32,
    pub code: u8,
    pub owned: bool,
}
pub(super) struct Keyboard {
    first: u8,
    syms: Vec<xkb::KeySymMap>,
    types: Vec<xkb::KeyType>,
    mods: [Vec<u8>; 8],
}
impl Keyboard {
    pub fn load(conn: &RustConnection<Timed>) -> io::Result<Self> {
        if !conn
            .xkb_use_extension(1, 0)
            .map_err(error)?
            .reply()
            .map_err(error)?
            .supported
        {
            return Err(error("XKB keyboard translation is unavailable"));
        }
        let map = conn
            .xkb_get_map(
                DEVICE,
                MapPart::KEY_TYPES | MapPart::KEY_SYMS,
                MapPart::default(),
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                0,
                VMod::default(),
                0,
                0,
                0,
                0,
                0,
                0,
            )
            .map_err(error)?
            .reply()
            .map_err(error)?;
        let syms = map
            .map
            .syms_rtrn
            .ok_or_else(|| error("Missing XKB symbols"))?;
        let types = map
            .map
            .types_rtrn
            .ok_or_else(|| error("Missing XKB key types"))?;
        let modifier = conn
            .get_modifier_mapping()
            .map_err(error)?
            .reply()
            .map_err(error)?;
        let n = usize::from(modifier.keycodes_per_modifier());
        if n == 0 || modifier.keycodes.len() != 8 * n || syms.len() > 256 || types.len() > 256 {
            return Err(error("Invalid XKB keyboard map"));
        }
        let mods = std::array::from_fn(|i| {
            modifier.keycodes[i * n..(i + 1) * n]
                .iter()
                .copied()
                .filter(|c| *c != 0)
                .collect()
        });
        Ok(Self {
            first: map.first_key_sym,
            syms,
            types,
            mods,
        })
    }
    fn code_for(&self, sym: u32) -> Option<u8> {
        self.syms
            .iter()
            .position(|row| row.syms.contains(&sym))
            .and_then(|i| self.first.checked_add(i as u8))
    }
    fn locking(&self, code: u8) -> bool {
        code.checked_sub(self.first)
            .and_then(|i| self.syms.get(usize::from(i)))
            .is_some_and(|row| {
                row.syms
                    .iter()
                    .any(|s| matches!(*s, 0xffe5 | 0xff7f | 0xff14))
            })
    }
    fn group(row: &xkb::KeySymMap, group: u8) -> Option<usize> {
        let n = row.group_info & 15;
        if n == 0 {
            return None;
        }
        Some(usize::from(if group < n {
            group
        } else {
            match row.group_info & 0xc0 {
                0x40 => n - 1,
                0x80 => ((row.group_info >> 4) & 3).min(n - 1),
                _ => group % n,
            }
        }))
    }
    fn symbol(&self, index: usize, group: u8, mods: u8) -> Option<u32> {
        let row = self.syms.get(index)?;
        let group = Self::group(row, group)?;
        let ty = self.types.get(usize::from(*row.kt_index.get(group)?))?;
        let mask = u16::from(ty.mods_mask) as u8;
        let level = ty
            .map
            .iter()
            .find(|entry| entry.active && (u16::from(entry.mods_mask) as u8) == mods & mask)
            .map_or(0, |entry| usize::from(entry.level));
        if level >= usize::from(row.width) {
            return None;
        }
        row.syms
            .get(group * usize::from(row.width) + level)
            .copied()
    }
}
fn pressed(keys: &[u8; 32], code: u8) -> bool {
    keys[usize::from(code / 8)] & (1 << (code % 8)) != 0
}
impl X11 {
    pub(super) fn keyboard_event(&mut self, sym: u32, down: bool) -> io::Result<()> {
        let sym = super::super::keyboard::canonical_keysym(sym);
        // Refresh the server map rather than caching a client keyboard layout across MappingNotify.
        let keyboard = Keyboard::load(&self.conn)?;
        let old = self.held.iter().position(|h| h.sym == sym);
        if !down && old.is_none() {
            return Ok(());
        }
        let character = super::super::keyboard::keysym_character(sym);
        let keys = self
            .conn
            .query_keymap()
            .map_err(error)?
            .reply()
            .map_err(error)?
            .keys;
        let state = self
            .conn
            .xkb_get_state(DEVICE)
            .map_err(error)?
            .reply()
            .map_err(error)?;
        let actual = u16::from(state.mods) as u8;
        let locked = (u16::from(state.locked_mods) as u8) | (u16::from(state.latched_mods) as u8);
        let group = u8::from(state.group);
        let mut changes = Vec::new();
        let code = if !down {
            self.held[old.unwrap()].code
        } else if character.is_some() {
            let mut candidates = Vec::new();
            for (index, row) in keyboard.syms.iter().enumerate() {
                let Some(code) = keyboard.first.checked_add(index as u8) else {
                    continue;
                };
                if !down && self.held[old.unwrap()].code != code {
                    continue;
                }
                let Some(g) = Keyboard::group(row, group) else {
                    continue;
                };
                let Some(ty) = row
                    .kt_index
                    .get(g)
                    .and_then(|i| keyboard.types.get(usize::from(*i)))
                else {
                    continue;
                };
                let affect = (u16::from(ty.mods_mask) as u8) & !locked & !2; // Never toggle CapsLock or latched/locked state.
                for desired in 0u8..=255 {
                    if desired & !affect != actual & !affect
                        || desired & (actual & 0x4c) != (actual & 0x4c)
                        || keyboard
                            .symbol(index, group, desired)
                            .and_then(super::super::keyboard::keysym_character)
                            != character
                    {
                        continue;
                    }
                    let mut transitions = Vec::new();
                    let mut valid = true;
                    for bit in 0..8 {
                        let mask = 1 << bit;
                        if (actual ^ desired) & mask == 0 {
                            continue;
                        }
                        if desired & mask == 0 {
                            for key in &keyboard.mods[bit] {
                                if pressed(&keys, *key) {
                                    if !self.held.iter().any(|h| h.code == *key && h.owned) {
                                        valid = false;
                                        break;
                                    }
                                    transitions.push((*key, false));
                                }
                            }
                        } else if let Some(key) = keyboard.mods[bit]
                            .iter()
                            .find(|key| !keyboard.locking(**key) && !pressed(&keys, **key))
                        {
                            transitions.push((*key, true));
                        } else {
                            valid = false
                        }
                    }
                    if valid {
                        candidates.push((transitions.len(), code, transitions));
                    }
                }
            }
            candidates.sort_by_key(|(n, code, _)| (*n, *code));
            let Some((_, code, transitions)) = candidates.into_iter().next() else {
                // Unsupported input is a bounded no-op, not a framebuffer/source failure.
                // No held keys or modifier transitions have been changed at this point.
                return Ok(());
            };
            changes = transitions;
            code
        } else {
            match old {
                Some(index) => self.held[index].code,
                None => match keyboard.code_for(sym) {
                    Some(code) => code,
                    None => return Ok(()),
                },
            }
        };
        // Own only presses introduced by this source. A pre-existing physical press is borrowed.
        let owned = old
            .map(|i| self.held[i].owned)
            .unwrap_or_else(|| !pressed(&keys, code));
        if down && old.is_none() {
            if self.held.len() >= 64 {
                return Err(error("Too many pressed VNC keys"));
            }
            self.held.push(Held { sym, code, owned });
        }
        for (key, press) in &changes {
            if *press {
                self.temporary.push(*key);
            }
            self.fake(
                if *press {
                    KEY_PRESS_EVENT
                } else {
                    KEY_RELEASE_EVENT
                },
                *key,
                0,
                0,
            )?;
        }
        let should_send = owned
            && (down
                || !self
                    .held
                    .iter()
                    .any(|h| h.sym != sym && h.code == code && h.owned));
        let result = if should_send {
            self.fake(
                if down {
                    KEY_PRESS_EVENT
                } else {
                    KEY_RELEASE_EVENT
                },
                code,
                0,
                0,
            )
        } else {
            Ok(())
        };
        // Restore explicit client-held modifiers immediately, without keeping synthesized Shift down.
        for (key, press) in changes.into_iter().rev() {
            self.fake(
                if press {
                    KEY_RELEASE_EVENT
                } else {
                    KEY_PRESS_EVENT
                },
                key,
                0,
                0,
            )?;
            if press {
                self.temporary.retain(|k| *k != key);
            }
        }
        if !down {
            self.held.retain(|h| h.sym != sym);
        }
        self.keyboard = Some(keyboard);
        result
    }
}
