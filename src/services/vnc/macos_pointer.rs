//! Pure Quartz event/button pairs. Numeric values are CGEventType/CGMouseButton
//! from the pinned maintained bindings; the native call consumes this exact pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Event {
    pub kind: u32,
    pub button: u32,
}
pub(super) fn movement(owned: u8) -> Event {
    if owned & 1 != 0 {
        Event { kind: 6, button: 0 }
    } else if owned & 4 != 0 {
        Event { kind: 7, button: 1 }
    } else if owned & 2 != 0 {
        Event {
            kind: 27,
            button: 2,
        }
    } else {
        Event { kind: 5, button: 0 }
    }
}
pub(super) fn button(bit: u8, down: bool) -> Event {
    match (bit, down) {
        (1, true) => Event { kind: 1, button: 0 },
        (1, false) => Event { kind: 2, button: 0 },
        (2, true) => Event {
            kind: 25,
            button: 2,
        },
        (2, false) => Event {
            kind: 26,
            button: 2,
        },
        (4, true) => Event { kind: 3, button: 1 },
        (4, false) => Event { kind: 4, button: 1 },
        _ => unreachable!("only owned RFB button bits are planned"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn center_down_drag_up_keeps_center_button() {
        assert_eq!(
            [button(2, true), movement(2), button(2, false)],
            [
                Event {
                    kind: 25,
                    button: 2
                },
                Event {
                    kind: 27,
                    button: 2
                },
                Event {
                    kind: 26,
                    button: 2
                }
            ]
        );
    }
    #[test]
    fn left_right_and_owned_multibutton_priority_are_preserved() {
        for (bit, expected) in [(1, [(1, 0), (6, 0), (2, 0)]), (4, [(3, 1), (7, 1), (4, 1)])] {
            let actual = [button(bit, true), movement(bit), button(bit, false)];
            assert_eq!(actual.map(|e| (e.kind, e.button)), expected);
        }
        assert_eq!(movement(0), Event { kind: 5, button: 0 });
        for mask in [3, 5, 7] {
            assert_eq!(movement(mask), movement(1));
        }
        assert_eq!(movement(6), movement(4));
        // Scroll bits and unowned requested buttons never create a drag.
        assert_eq!(movement(8 | 16 | 32), movement(0));
    }
}
