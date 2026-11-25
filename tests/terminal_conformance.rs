use harbor_workspace::terminal::Screen;
#[test]
fn combining_and_wide_cells_survive_resize() {
    let mut screen = Screen::new(8, 3, 100);
    screen.feed("e\u{301}界".as_bytes());
    assert_eq!(screen.cell(0, 0).unwrap().text, "e\u{301}");
    assert_eq!(screen.cell(0, 1).unwrap().width, 2);
    assert!(screen.cell(0, 2).unwrap().continuation);
    assert_eq!(screen.col, 3);
    screen.resize(10, 4);
    assert_eq!(screen.cell(0, 1).unwrap().text, "界");
    assert!(screen.cell(0, 2).unwrap().continuation);
    screen.feed("\r\ne\u{1ab0}X".as_bytes());
    assert_eq!(screen.cell(1, 0).unwrap().text, "e\u{1ab0}");
    assert_eq!(screen.cell(1, 1).unwrap().text, "X");
}
#[test]
fn streamed_emoji_clusters_and_margin_wrapping_use_display_columns() {
    let mut screen = Screen::new(8, 3, 100);
    for byte in "👩\u{200d}💻X".as_bytes() {
        screen.feed(&[*byte]);
    }
    assert_eq!(screen.cell(0, 0).unwrap().text, "👩\u{200d}💻");
    assert_eq!(screen.cell(0, 0).unwrap().width, 2);
    assert!(screen.cell(0, 1).unwrap().continuation);
    assert_eq!(screen.cell(0, 2).unwrap().text, "X");
    let mut screen = Screen::new(4, 3, 100);
    screen.feed("abc界".as_bytes());
    assert_eq!(screen.cell(0, 3).unwrap().text, " ");
    assert_eq!(screen.cell(1, 0).unwrap().text, "界");
    assert_eq!(screen.col, 2);
}
#[test]
fn alternate_resize_restores_primary() {
    let mut screen = Screen::new(8, 3, 100);
    screen.feed(b"history\r\nprimary\r\ntail\r\n");
    let history = screen
        .history()
        .iter()
        .map(|l| l.iter().map(|c| c.text.as_str()).collect::<String>())
        .collect::<Vec<_>>();
    screen.feed(b"\x1b[?1049hALT");
    screen.resize(10, 4);
    screen.feed(b"\x1b[?1049l");
    assert!(!screen.is_alternate());
    assert!(screen.plain().contains("primary"));
    assert_eq!(
        screen
            .history()
            .iter()
            .map(|l| l.iter().map(|c| c.text.as_str()).collect::<String>())
            .collect::<Vec<_>>(),
        history
    );
    let mut screen = Screen::new(4, 3, 100);
    screen.feed(b"abcd\x1b[?1049hALT\x1b[?1049lX");
    assert_eq!(screen.cell(0, 3).unwrap().text, "d");
    assert_eq!(screen.cell(1, 0).unwrap().text, "X");
}
#[test]
fn scroll_region_does_not_corrupt_history() {
    let mut screen = Screen::new(8, 4, 100);
    screen.feed(b"top\r\nsecond\r\nthird\r\nbottom");
    assert!(screen.history().is_empty());
    screen.feed(b"\x1b[1;2r\x1b[2;1H\n");
    assert!(
        screen.history().is_empty(),
        "A partial-screen region must not append scrollback"
    );
    assert_eq!(screen.cell(3, 0).unwrap().text, "b");
}
#[test]
fn editing_either_half_of_a_wide_cell_never_leaves_an_orphan() {
    let mut screen = Screen::new(8, 3, 100);
    screen.feed("界X".as_bytes());
    screen.feed(b"\x1b[1;2Hq");
    assert_eq!(screen.cell(0, 0).unwrap().text, " ");
    assert!(!screen.cell(0, 1).unwrap().continuation);
    assert_eq!(screen.cell(0, 1).unwrap().text, "q");
    screen.feed("\r界X\x1b[1;2H\x1b[X".as_bytes());
    assert_eq!(screen.cell(0, 0).unwrap().text, " ");
    assert!(!screen.cell(0, 1).unwrap().continuation);
}
#[test]
fn long_scrollback_search_reuses_unchanged_index() {
    let mut screen = Screen::new(40, 4, 1000);
    for _ in 0..500 {
        screen.feed(b"literal needle line\r\n");
    }
    assert_eq!(screen.search_count("needle"), 500);
    let builds = screen.stats().index_builds;
    for _ in 0..100 {
        assert_eq!(screen.search_count("needle"), 500);
    }
    assert_eq!(screen.stats().index_builds, builds);
    screen.feed(b"needle");
    assert_eq!(screen.search_count("needle"), 501);
    assert_eq!(screen.stats().index_builds, builds + 1);
}
#[test]
fn scrollback_memory_and_grapheme_growth_are_bounded() {
    let mut screen = Screen::new(40, 4, 360_000);
    screen.set_history_memory_budget(128 * 1024);
    for _ in 0..3000 {
        screen.feed(b"bounded history\r\n");
    }
    assert!(screen.stats().history_bytes <= 128 * 1024);
    assert!(screen.history().len() < 3000);
    assert!(screen.stats().discarded_lines > 0);
    let mut text = String::from("e");
    for _ in 0..10000 {
        text.push('\u{301}');
    }
    screen.feed(text.as_bytes());
    assert!(screen.cell(screen.row, 0).unwrap().text.len() <= 128);
}
#[test]
fn dec_save_restore_preserves_wrap_and_rendition() {
    use harbor_workspace::terminal::PALETTE;
    let mut screen = Screen::new(4, 3, 100);
    screen.feed(b"\x1b[31mabcd\x1b7\r\n\x1b[32mx\x1b8Y");
    assert_eq!(screen.cell(0, 3).unwrap().text, "d");
    assert_eq!(screen.cell(1, 0).unwrap().text, "Y");
    assert_eq!(screen.cell(1, 0).unwrap().fg, PALETTE[1]);
}
#[test]
fn origin_insert_and_dec_graphics_modes_follow_vt_commands() {
    let mut screen = Screen::new(8, 5, 100);
    screen.feed(b"\x1b[2;4r\x1b[?6h\x1b[1;1H@");
    assert_eq!(screen.cell(1, 0).unwrap().text, "@");
    screen.replies.clear();
    screen.feed(b"\x1b[6n");
    assert_eq!(screen.replies, b"\x1b[1;2R");
    screen.feed(b"\x1b[999E!");
    assert_eq!(screen.cell(3, 0).unwrap().text, "!");
    screen.feed(b"\x1b[999F^");
    assert_eq!(screen.cell(1, 0).unwrap().text, "^");
    screen.feed(b"\x1b[?6l\x1b[1;1Habc\x1b[1;2H\x1b[4hX\x1b[4l");
    assert_eq!(screen.cell(0, 1).unwrap().text, "X");
    assert_eq!(screen.cell(0, 2).unwrap().text, "b");
    screen.feed(b"\x1b[5;1H\x1b(0lqk\x1b(B");
    assert_eq!(screen.cell(4, 0).unwrap().text, "┌");
    assert_eq!(screen.cell(4, 1).unwrap().text, "─");
    assert_eq!(screen.cell(4, 2).unwrap().text, "┐");
    screen.feed(b"\x1b[?1049hALT\x1bc");
    assert!(!screen.is_alternate());
    assert!(screen.grid().iter().flatten().all(|cell| cell.text == " "));
}
#[test]
fn unsupported_control_strings_and_cancelled_sequences_never_become_visible_text() {
    let mut screen = Screen::new(40, 3, 100);
    screen
        .feed(b"A\x1bPprivate-payload\x1b\\B\x1b_ignored-apc\x1b\\C\x1b]52;c;clipboard-data\x07D");
    assert_eq!(screen.plain().trim(), "ABCD");
    screen.feed(b"\r\n\x1b[123\x18visible");
    assert_eq!(screen.cell(1, 0).unwrap().text, "v");
    assert!(screen.plain().contains("visible"));
}
#[test]
fn plain_scrollback_retains_ordinary_history_within_a_small_budget() {
    let mut screen = Screen::new(40, 4, 1000);
    screen.set_history_memory_budget(256 * 1024);
    for n in 0..1000 {
        screen.feed(format!("owned line {n} needle\r\n").as_bytes());
    }
    assert!(
        screen.history().len() + screen.rows >= 1000,
        "Ordinary history expired before the configured line cap"
    );
    assert_eq!(screen.search_count("needle"), 1000);
    assert!(screen.plain().starts_with("owned line 0 needle"));
    assert!(screen.stats().history_bytes <= 256 * 1024);
}
#[test]
fn widening_a_pending_wrap_cursor_never_overwrites_the_last_character() {
    let mut screen = Screen::new(4, 3, 100);
    screen.feed(b"abcd");
    screen.resize(6, 3);
    screen.feed(b"X");
    assert_eq!(screen.cell(0, 3).unwrap().text, "d");
    assert_eq!(screen.cell(0, 4).unwrap().text, "X");
    let mut screen = Screen::new(4, 3, 100);
    screen.feed(b"abcd\x1b[?1049hALT");
    screen.resize(6, 3);
    screen.feed(b"\x1b[?1049lX");
    assert_eq!(screen.cell(0, 3).unwrap().text, "d");
    assert_eq!(screen.cell(0, 4).unwrap().text, "X");
}
#[test]
fn nonprinting_reports_and_modes_preserve_pending_wrap() {
    let mut screen = Screen::new(4, 3, 100);
    screen.feed(b"abcd\x1b[6n\x1b[?25hX");
    assert_eq!(screen.replies, b"\x1b[1;4R");
    assert_eq!(screen.cell(0, 3).unwrap().text, "d");
    assert_eq!(screen.cell(1, 0).unwrap().text, "X");
}
#[test]
fn dec_restore_reinstates_origin_mode_and_survives_widening() {
    let mut screen = Screen::new(8, 5, 100);
    screen.feed(b"\x1b[2;4r\x1b[?6h\x1b7\x1b[?6l\x1b8\x1b[1;1H@");
    assert_eq!(screen.cell(1, 0).unwrap().text, "@");
    let mut screen = Screen::new(4, 3, 100);
    screen.feed(b"abcd\x1b7");
    screen.resize(6, 3);
    screen.feed(b"\x1b8X");
    assert_eq!(screen.cell(0, 3).unwrap().text, "d");
    assert_eq!(screen.cell(0, 4).unwrap().text, "X");
}
#[test]
fn erasing_half_a_wide_glyph_uses_the_current_erase_rendition() {
    use harbor_workspace::terminal::PALETTE;
    let mut screen = Screen::new(8, 3, 100);
    screen.feed("界X\x1b[42m\x1b[1;2H\x1b[X".as_bytes());
    assert_eq!(screen.cell(0, 0).unwrap().text, " ");
    assert_eq!(screen.cell(0, 0).unwrap().bg, PALETTE[2]);
    assert_eq!(screen.cell(0, 1).unwrap().bg, PALETTE[2]);
}
#[test]
fn oversized_control_parameters_cannot_panic_or_escape_the_screen() {
    let mut screen = Screen::new(8, 3, 100);
    let huge = usize::MAX;
    screen.feed(
        format!("\x1b[{huge}B\x1b[{huge}B\x1b[{huge}C\x1b[{huge}C\x1b[38;5;{huge}mX").as_bytes(),
    );
    assert!(screen.row < screen.rows && screen.col < screen.cols);
    assert_eq!(screen.cell(2, 7).unwrap().text, "X");
}
#[test]
fn alternate_visible_copy_and_search_exclude_primary_history() {
    let mut screen = Screen::new(8, 3, 100);
    screen.feed(b"needle\r\none\r\ntwo\r\nthree");
    assert_eq!(screen.search_count("needle"), 1);
    screen.feed(b"\x1b[?1049hALT");
    assert!(!screen.plain().contains("needle"));
    assert_eq!(screen.search_count("needle"), 0);
    assert_eq!(screen.search_count("ALT"), 1);
    screen.clear();
    screen.feed(b"NEW");
    assert_eq!(screen.search_count("NEW"), 1);
    screen.feed(b"\x1b[?1049l");
    assert_eq!(screen.search_count("needle"), 1);
}
#[test]
fn insert_mode_extends_graphemes_once_and_standalone_marks_own_their_cells() {
    let mut screen = Screen::new(12, 2, 100);
    screen.feed(b"ABCD\r\x1b[4h");
    screen.feed("e\u{301}".as_bytes());
    assert_eq!(screen.plain().lines().next().unwrap(), "e\u{301}ABCD");
    let mut screen = Screen::new(8, 2, 100);
    screen.feed("界X\x1b[1;2H\u{301}".as_bytes());
    for col in 0..screen.cols {
        let cell = screen.cell(0, col).unwrap();
        if cell.width == 2 {
            assert!(
                screen.cell(0, col + 1).is_some_and(|c| c.continuation),
                "orphan wide head at {col}"
            );
        }
        if cell.continuation {
            assert!(col > 0 && screen.cell(0, col - 1).unwrap().width == 2);
        }
    }
    assert_eq!(screen.cell(0, 2).unwrap().text, "X");
}
#[test]
fn shrinking_height_preserves_live_prompt_and_primary_history_even_after_alternate() {
    for alternate in [false, true] {
        let mut screen = Screen::new(8, 4, 100);
        screen.feed(b"ONE\r\nTWO\r\nTHREE\r\nPROMPT");
        if alternate {
            screen.feed(b"\x1b[?1049hALT");
        }
        screen.resize(8, 2);
        if alternate {
            screen.feed(b"\x1b[?1049l");
        }
        assert_eq!(screen.cell(0, 0).unwrap().text, "T");
        assert_eq!(screen.cell(1, 0).unwrap().text, "P");
        assert_eq!(screen.row, 1);
        assert_eq!(screen.history().len(), 2);
        let plain = screen.plain();
        assert!(plain.starts_with("ONE\nTWO\nTHREE\nPROMPT"), "{plain}");
    }
}
#[test]
fn shrinking_unused_bottom_padding_keeps_visible_rows_in_place() {
    let mut screen = Screen::new(8, 30, 100);
    screen.feed(b"ABCD\r\n1234");
    screen.resize(8, 10);
    assert_eq!(
        screen.history().len(),
        0,
        "unused bottom rows should shrink before scrolling live text"
    );
    assert_eq!(screen.cell(0, 0).unwrap().text, "A");
    assert_eq!(screen.cell(1, 0).unwrap().text, "1");
    assert_eq!(screen.row, 1);
}
