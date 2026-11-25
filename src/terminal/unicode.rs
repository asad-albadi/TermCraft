//! Grapheme segmentation and terminal display width share the pinned Unicode implementations.
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};
pub(super) const MAX_GRAPHEME_BYTES: usize = 128;
pub(super) fn extension(base: &str, next: char) -> Option<String> {
    if base.is_ascii() && next.is_ascii() {
        return None;
    }
    let mut candidate = String::with_capacity(base.len() + next.len_utf8());
    candidate.push_str(base);
    candidate.push(next);
    if candidate.graphemes(true).count() != 1 {
        return None;
    }
    if candidate.len() > MAX_GRAPHEME_BYTES {
        return Some(base.to_owned());
    }
    Some(candidate)
}
pub(super) fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}
pub(super) fn char_width(c: char) -> usize {
    UnicodeWidthChar::width(c).unwrap_or(0)
}
