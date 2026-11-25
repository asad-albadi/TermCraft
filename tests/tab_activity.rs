use harbor_workspace::workspace::activity::ActivityState;
#[test]
fn background_output_marks_unread_until_viewed() {
    let mut state = ActivityState::default();
    state.on_output(2, false);
    assert!(state.unread(2));
    state.mark_seen(2);
    assert!(!state.unread(2));
    state.on_output(2, true);
    assert!(!state.unread(2));
}
#[test]
fn visible_detached_terminal_is_not_unread() {
    let mut state = ActivityState::default();
    state.on_output(9, false);
    state.mark_seen(9);
    state.on_output(9, true);
    assert!(!state.unread(9));
    state.set_error(9, true);
    assert!(state.error(9));
    assert!(!state.unread(9));
    state.remove(9);
    assert!(!state.error(9));
}
#[test]
fn screen_activity_ignores_protocol_queries_and_unfinished_sequences() {
    let mut screen = harbor_workspace::terminal::Screen::new(80, 24, 100);
    let initial = screen.activity_signature();
    screen.feed(b"\x1b[6n");
    assert_eq!(initial, screen.activity_signature());
    screen.feed(b"\x1b[");
    assert_eq!(initial, screen.activity_signature());
    screen.feed(b"mhello");
    assert_ne!(initial, screen.activity_signature());
}
