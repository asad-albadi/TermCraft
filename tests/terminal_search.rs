use harbor_workspace::terminal::Screen;
#[test]
fn active_search_only_indexes_new_history_rows() {
    let mut screen = Screen::new(40, 4, 1000);
    for _ in 0..500 {
        screen.feed(b"needle line\r\n");
    }
    assert_eq!(screen.search_count("needle"), 500);
    let indexed = screen.stats().indexed_history_lines;
    screen.feed(b"new needle\r\n");
    assert_eq!(screen.search_count("needle"), 501);
    assert_eq!(
        screen.stats().indexed_history_lines - indexed,
        1,
        "Unchanged history must not be rescanned while output continues"
    );
}
