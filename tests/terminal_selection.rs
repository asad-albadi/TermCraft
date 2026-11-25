use harbor_workspace::{
    model::{ClipboardSettings, RightClickBehavior},
    terminal::{
        Screen,
        selection::{
            ClipboardAction, InputEvent, SelectionMode, SelectionPoint, TerminalSelection,
        },
    },
};
fn point(row: usize, col: usize) -> SelectionPoint {
    SelectionPoint { row, col }
}
#[test]
fn rectangle_selection_preserves_columns_and_unicode() {
    let mut screen = Screen::new(8, 3, 100);
    screen.feed(b"abcd\r\n1234");
    let selection = TerminalSelection::new(&screen, point(0, 1), point(1, 3));
    assert_eq!(
        selection.extract(&screen, SelectionMode::Rectangle),
        "bc\n23"
    );
    screen.feed("\r\nA界B".as_bytes());
    let selection = TerminalSelection::new(&screen, point(2, 1), point(2, 3));
    assert_eq!(selection.extract(&screen, SelectionMode::Rectangle), "界");
    let selection = TerminalSelection::new(&screen, point(2, 2), point(2, 3));
    assert_eq!(selection.extract(&screen, SelectionMode::Rectangle), "界");
}
#[test]
fn right_click_mode_keeps_context_menu_reachable() {
    let settings = ClipboardSettings {
        right_click: RightClickBehavior::Paste,
        ..Default::default()
    };
    assert_eq!(
        ClipboardAction::from_input(&InputEvent::RightClick { shift: false }, &settings),
        Some(ClipboardAction::Paste)
    );
    assert_eq!(
        ClipboardAction::from_input(&InputEvent::RightClick { shift: true }, &settings),
        Some(ClipboardAction::Menu)
    );
    assert_eq!(
        ClipboardAction::from_input(
            &InputEvent::RightClick { shift: false },
            &ClipboardSettings::default()
        ),
        Some(ClipboardAction::Menu)
    );
    assert_eq!(
        ClipboardAction::from_input(&InputEvent::CopyShortcut, &settings),
        Some(ClipboardAction::Copy)
    );
}
#[test]
fn linear_history_selection_survives_scroll_and_expires_without_retargeting() {
    let mut screen = Screen::new(8, 2, 3);
    screen.feed(b"one\r\ntwo\r\nthree");
    let selection = TerminalSelection::new(&screen, point(0, 1), point(1, 2));
    assert_eq!(selection.extract(&screen, SelectionMode::Linear), "ne\ntw");
    screen.feed(b"\r\nfour");
    assert_eq!(selection.extract(&screen, SelectionMode::Linear), "ne\ntw");
    screen.feed(b"\r\nfive\r\nsix");
    assert_eq!(selection.extract(&screen, SelectionMode::Linear), "");
}
#[test]
fn configured_delimiters_word_selection_and_alternate_epoch() {
    let mut screen = Screen::new(30, 3, 100);
    screen.feed("prefix.foo/界 name".as_bytes());
    assert_eq!(
        TerminalSelection::word(&screen, point(0, 8), "./")
            .unwrap()
            .extract(&screen, SelectionMode::Linear),
        "foo"
    );
    assert_eq!(
        TerminalSelection::word(&screen, point(0, 6), "./")
            .unwrap()
            .extract(&screen, SelectionMode::Linear),
        "."
    );
    let selection = TerminalSelection::word(&screen, point(0, 12), "./").unwrap();
    assert_eq!(selection.extract(&screen, SelectionMode::Linear), "界");
    screen.feed(b"\x1b[?1049hOTHER");
    assert!(selection.extract(&screen, SelectionMode::Linear).is_empty());
    screen.feed(b"\x1b[?1049l");
    assert!(selection.extract(&screen, SelectionMode::Linear).is_empty());
}
#[test]
fn selection_padding_uses_the_terminal_column_bound() {
    let screen = Screen::new(8, 2, 100);
    let selection = TerminalSelection::new(&screen, point(0, 0), point(0, 600));
    assert!(selection.extract(&screen, SelectionMode::Rectangle).len() <= 500);
}
