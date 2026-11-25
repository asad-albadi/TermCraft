use harbor_workspace::workspace::shortcuts::{Action, FocusContext, KeyChord, ShortcutMap};
#[test]
fn shortcut_conflicts_are_explicit() {
    let mut map = ShortcutMap::default();
    let chord = KeyChord::new("F12", false, false, false);
    map.bind(Action::NewTerminal, chord.clone()).unwrap();
    assert!(map.bind(Action::Find, chord).is_err());
    let before = map.clone();
    assert!(
        map.bind(
            Action::Find,
            KeyChord::new("unrecognised", false, false, false)
        )
        .is_err()
    );
    assert_eq!(map, before);
}
#[test]
fn text_editor_focus_does_not_trigger_terminal_action() {
    let map = ShortcutMap::default();
    let chord = KeyChord::new("PageUp", false, true, false);
    assert_eq!(
        map.resolve(&chord, FocusContext::Terminal),
        Some(Action::PageUp)
    );
    for focus in [
        FocusContext::TextEditor,
        FocusContext::Authentication,
        FocusContext::RemoteDesktop,
        FocusContext::Workspace,
    ] {
        assert!(map.resolve(&chord, focus).is_none());
    }
}
#[test]
fn history_shortcut_scrolls_without_sending_input() {
    let map = ShortcutMap::default();
    let action = map
        .resolve(
            &KeyChord::new("PageUp", false, true, false),
            FocusContext::Terminal,
        )
        .unwrap();
    assert_eq!(action.scroll_offset(900., 20., 400., 2000.), Some(520.));
    assert_eq!(
        Action::HistoryStart.scroll_offset(900., 20., 400., 2000.),
        Some(0.)
    );
    assert_eq!(
        Action::HistoryEnd.scroll_offset(900., 20., 400., 2000.),
        Some(2000.)
    );
    assert!(
        Action::NewTerminal
            .scroll_offset(900., 20., 400., 2000.)
            .is_none()
    );
}
#[test]
fn shortcuts_round_trip_and_old_preferences_get_defaults() {
    let mut map = ShortcutMap::default();
    let chord = KeyChord::new("F12", false, false, true);
    map.bind(Action::NewTerminal, chord.clone()).unwrap();
    let loaded: ShortcutMap = serde_json::from_str(&serde_json::to_string(&map).unwrap()).unwrap();
    assert_eq!(
        loaded.resolve(&chord, FocusContext::Workspace),
        Some(Action::NewTerminal)
    );
    let prefs: harbor_workspace::model::Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(prefs.shortcuts, ShortcutMap::default());
}
#[test]
fn key_aliases_are_canonical_and_bare_typing_is_rejected() {
    use eframe::egui::{Key, Modifiers};
    let mut map = ShortcutMap::default();
    assert_eq!(
        map.resolve(
            &KeyChord::from_key(
                Key::ArrowUp,
                Modifiers {
                    shift: true,
                    ..Modifiers::COMMAND
                }
            ),
            FocusContext::Terminal
        ),
        Some(Action::LineUp)
    );
    assert!(
        map.bind(Action::NewTerminal, KeyChord::new("A", false, false, false))
            .is_err()
    );
    assert!(
        map.bind(
            Action::NewTerminal,
            KeyChord::new("Space", false, true, false)
        )
        .is_err()
    );
}
