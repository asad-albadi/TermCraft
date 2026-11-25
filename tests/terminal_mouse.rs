use harbor_workspace::terminal::{
    Screen,
    mouse::{
        MouseAction, MouseButton, MouseEncoder, MouseMode, MouseModifiers, MouseTracking,
        TerminalMouseEvent,
    },
};
fn event(action: MouseAction, col: usize, row: usize) -> TerminalMouseEvent {
    TerminalMouseEvent {
        action,
        col,
        row,
        modifiers: MouseModifiers::default(),
    }
}
fn sgr() -> MouseMode {
    MouseMode {
        tracking: MouseTracking::Press,
        sgr: true,
    }
}
#[test]
fn sgr_mouse_coordinates_and_release_are_correct() {
    let mut encoder = MouseEncoder::default();
    assert_eq!(
        encoder
            .encode(sgr(), event(MouseAction::Press(MouseButton::Left), 0, 0))
            .unwrap(),
        b"\x1b[<0;1;1M"
    );
    assert_eq!(
        encoder
            .encode(sgr(), event(MouseAction::Release(MouseButton::Left), 0, 0))
            .unwrap(),
        b"\x1b[<0;1;1m"
    );
}
#[test]
fn disabled_mouse_mode_sends_nothing() {
    let mut encoder = MouseEncoder::default();
    for action in [
        MouseAction::Press(MouseButton::Left),
        MouseAction::Move,
        MouseAction::WheelUp,
    ] {
        assert!(
            encoder
                .encode(MouseMode::default(), event(action, 0, 0))
                .is_none()
        );
    }
    let mut screen = Screen::new(80, 24, 100);
    screen.feed(b"\x1b[?1006h");
    assert!(!screen.mouse_mode().enabled());
    screen.feed(b"\x1b[?1000h");
    assert_eq!(screen.mouse_mode(), sgr());
    screen.feed(b"\x1b[?1002h");
    assert_eq!(screen.mouse_mode().tracking, MouseTracking::ButtonMotion);
    screen.feed(b"\x1b[?1003h\x1b[?1003l");
    assert!(!screen.mouse_mode().enabled());
    screen.feed(b"\x1b[?1000h\x1bc");
    assert_eq!(screen.mouse_mode(), MouseMode::default());
}
#[test]
fn motion_modes_modifiers_and_legacy_coordinates() {
    let mut enc = MouseEncoder::default();
    let mode = MouseMode {
        tracking: MouseTracking::ButtonMotion,
        sgr: true,
    };
    assert!(enc.encode(mode, event(MouseAction::Move, 1, 2)).is_none());
    enc.encode(mode, event(MouseAction::Press(MouseButton::Right), 1, 2));
    let mut e = event(MouseAction::Move, 2, 2);
    e.modifiers = MouseModifiers {
        shift: false,
        alt: true,
        ctrl: true,
    };
    assert_eq!(enc.encode(mode, e).unwrap(), b"\x1b[<58;3;3M");
    assert!(enc.encode(mode, e).is_none());
    assert_eq!(
        enc.encode(mode, event(MouseAction::Release(MouseButton::Right), 2, 2))
            .unwrap(),
        b"\x1b[<2;3;3m"
    );
    assert!(enc.encode(mode, event(MouseAction::Move, 3, 2)).is_none());
    let mode = MouseMode {
        tracking: MouseTracking::AnyMotion,
        sgr: true,
    };
    assert_eq!(
        enc.encode(mode, event(MouseAction::Move, 3, 2)).unwrap(),
        b"\x1b[<35;4;3M"
    );
    let mode = MouseMode {
        tracking: MouseTracking::Press,
        sgr: false,
    };
    assert_eq!(
        enc.encode(mode, event(MouseAction::Press(MouseButton::Middle), 222, 1))
            .unwrap(),
        vec![27, 91, 77, 33, 255, 34]
    );
    assert_eq!(
        enc.encode(
            mode,
            event(MouseAction::Release(MouseButton::Middle), 222, 1)
        )
        .unwrap(),
        vec![27, 91, 77, 35, 255, 34]
    );
    assert!(
        enc.encode(mode, event(MouseAction::Press(MouseButton::Left), 223, 0))
            .is_none()
    );
}
#[test]
fn shift_override_releases_pressed_buttons_and_does_not_resume_drag() {
    let mut enc = MouseEncoder::default();
    let mode = MouseMode {
        tracking: MouseTracking::ButtonMotion,
        sgr: true,
    };
    enc.encode(mode, event(MouseAction::Press(MouseButton::Left), 4, 2));
    assert_eq!(enc.release_all(mode).unwrap(), b"\x1b[<0;5;3m");
    assert!(enc.encode(mode, event(MouseAction::Move, 5, 2)).is_none());
    assert!(
        enc.encode(mode, event(MouseAction::Release(MouseButton::Left), 5, 2))
            .is_none()
    );
}
#[test]
fn movement_within_same_cell_is_not_reported_even_after_press() {
    let mut enc = MouseEncoder::default();
    let mode = MouseMode {
        tracking: MouseTracking::ButtonMotion,
        sgr: true,
    };
    enc.encode(mode, event(MouseAction::Press(MouseButton::Left), 4, 2));
    assert!(enc.encode(mode, event(MouseAction::Move, 4, 2)).is_none());
    assert_eq!(
        enc.encode(mode, event(MouseAction::Move, 5, 2)).unwrap(),
        b"\x1b[<32;6;3M"
    );
}
#[test]
fn legacy_release_outside_coordinate_range_uses_last_representable_cell() {
    let mode = MouseMode {
        tracking: MouseTracking::Press,
        sgr: false,
    };
    let mut encoder = MouseEncoder::default();
    encoder
        .encode(mode, event(MouseAction::Press(MouseButton::Left), 10, 0))
        .unwrap();
    assert_eq!(
        encoder
            .encode(mode, event(MouseAction::Release(MouseButton::Left), 250, 0))
            .unwrap(),
        vec![27, 91, 77, 35, 43, 33]
    );
    assert!(encoder.release_all(mode).is_none());
}
