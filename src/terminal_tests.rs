#[cfg(test)] mod tests {
 use super::*;
 #[test] fn cursor_edit_and_erase(){let mut s=Screen::new(20,4,100);s.feed(b"hello world\r\x1b[6Gthere\x1b[K");assert!(s.plain().starts_with("hellothere"));}
 #[test] fn preserves_split_utf8(){let mut s=Screen::new(20,4,100);s.feed(&[0xc3]);s.feed(&[0xa9]);assert!(s.plain().starts_with('é'));}
 #[test] fn alternate_screen_restores_primary(){let mut s=Screen::new(20,4,100);s.feed(b"primary\x1b[?1049hother\x1b[?1049l");assert!(s.plain().starts_with("primary"));assert!(!s.plain().contains("other"));}
 #[test] fn scrolling_keeps_history(){let mut s=Screen::new(12,2,100);s.feed(b"first\r\nsecond\r\nthird");assert!(s.plain().contains("first"));assert!(s.plain().contains("third"));}
}
