use harbor_workspace::{
    model::{Preferences, Store},
    terminal::settings::{
        BackspaceMapping, CharacterEncoding, Dimensions, TerminalCodec, TerminalFont,
        TerminalOverrides, TerminalSettings,
    },
};
#[test]
fn old_store_inherits_terminal_defaults() {
    let store: Store = serde_json::from_str(r#"{"preferences":{}}"#).unwrap();
    let settings = store.preferences.terminal_settings();
    assert_eq!(settings.font_family, TerminalFont::JetBrainsMonoNerd);
    let explicit: TerminalSettings =
        serde_json::from_str(r#"{"font_family":"DejaVuSansMono"}"#).unwrap();
    assert_eq!(explicit.font_family, TerminalFont::DejaVuSansMono);
    assert_eq!(settings.font_size, 14.);
    assert_eq!(settings.encoding, CharacterEncoding::Utf8);
    let old: Preferences = serde_json::from_str(r#"{"font_size":19.0}"#).unwrap();
    assert_eq!(old.terminal_settings().font_size, 19.);
}
#[test]
fn session_override_does_not_modify_global() {
    let global = TerminalSettings::default();
    let original = global.clone();
    let mut session = TerminalOverrides {
        font_family: Some(TerminalFont::LiberationSans),
        font_size: Some(20.),
        backspace: Some(BackspaceMapping::ControlH),
        dimensions: Some(Dimensions::Fixed {
            cols: 132,
            rows: 40,
        }),
        ..Default::default()
    };
    let resolved = TerminalSettings::resolve(&global, &session);
    assert_eq!(resolved.font_size, 20.);
    assert_eq!(resolved.backspace.bytes(), b"\x08");
    assert_eq!(global, original);
    session.reset();
    let mut changed = global.clone();
    changed.font_size = 17.;
    assert_eq!(TerminalSettings::resolve(&changed, &session), changed);
}
#[test]
fn split_legacy_encoding_decodes_once() {
    let mut codec = TerminalCodec::new(CharacterEncoding::Windows1252);
    assert_eq!(codec.feed(&[0x80]), "€");
    codec.set_encoding(CharacterEncoding::ShiftJis);
    assert!(codec.feed(&[0x82]).is_empty());
    assert_eq!(codec.feed(&[0xa0]), "あ");
    assert_eq!(codec.source_bytes(), 3);
    assert_eq!(codec.encode("あ\r").unwrap(), vec![0x82, 0xa0, 13]);
    assert!(codec.encode("🦀").is_err());
    codec.set_encoding(CharacterEncoding::Utf8);
    assert!(codec.feed(&[0xc3]).is_empty());
    assert_eq!(codec.feed(&[0xa9]), "é");
}
#[test]
fn encoding_change_flushes_pending_once_and_dimensions_are_bounded() {
    let mut codec = TerminalCodec::new(CharacterEncoding::Utf8);
    assert!(codec.feed(&[0xe2]).is_empty());
    assert_eq!(codec.set_encoding(CharacterEncoding::Windows1252), "�");
    assert_eq!(codec.feed(b"A"), "A");
    let settings = TerminalSettings::resolve(
        &TerminalSettings::default(),
        &TerminalOverrides {
            font_size: Some(10000.),
            dimensions: Some(Dimensions::Fixed {
                cols: 0,
                rows: 65535,
            }),
            ..Default::default()
        },
    );
    assert_eq!(settings.font_size, 32.);
    assert_eq!(
        settings.dimensions,
        Dimensions::Fixed { cols: 2, rows: 200 }
    );
}
#[test]
fn palette_changes_recolor_indexes_but_preserve_explicit_truecolor() {
    use harbor_workspace::terminal::{Ink, Screen};
    let mut screen = Screen::new(8, 2, 100);
    screen.feed(b"\x1b[31mR\x1b[38;2;236;236;236mT\x1b[0mD");
    let mut palette = TerminalSettings::default().light_palette;
    palette.ansi[1] = [1, 2, 3];
    palette.foreground = [9, 8, 7];
    assert_eq!(palette.colors(screen.cell(0, 0).unwrap()).0, Ink(1, 2, 3));
    assert_eq!(
        palette.colors(screen.cell(0, 1).unwrap()).0,
        Ink(236, 236, 236)
    );
    assert_eq!(palette.colors(screen.cell(0, 2).unwrap()).0, Ink(9, 8, 7));
}
#[test]
fn palette_sources_survive_history_and_final_decoder_flush_is_once() {
    use harbor_workspace::terminal::{Ink, Screen};
    let mut screen = Screen::new(8, 2, 100);
    screen.feed(b"\x1b[31mA\x1b[38;2;255;96;96mBBBBBBB\r\nnext\r\nlast");
    let mut palette = TerminalSettings::default().light_palette;
    palette.ansi[1] = [1, 2, 3];
    let line = screen.history().get(0).unwrap();
    assert_eq!(palette.colors(&line[0]).0, Ink(1, 2, 3));
    assert_eq!(palette.colors(&line[1]).0, Ink(255, 96, 96));
    let mut codec = TerminalCodec::new(CharacterEncoding::ShiftJis);
    assert!(codec.feed(&[0x82]).is_empty());
    assert_eq!(codec.finish(), "�");
    assert!(codec.finish().is_empty());
}
