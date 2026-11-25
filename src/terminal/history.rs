//! Immutable ordinary rows keep text and one rendition instead of a full cell grid.
use super::{Cell, ColorSource, Ink};
use std::collections::VecDeque;
#[derive(Clone, Copy)]
pub(super) struct Rendition {
    fg: Ink,
    bg: Ink,
    fg_source: ColorSource,
    bg_source: ColorSource,
    bold: bool,
    inverse: bool,
}
pub(super) enum HistoryLine {
    Compact {
        text: smol_str::SmolStr,
        rendition: Rendition,
        columns: usize,
    },
    Full(Vec<Cell>),
}
impl HistoryLine {
    pub fn new(cells: Vec<Cell>) -> Self {
        if let Some(first) = cells.first() {
            if cells.iter().all(|c| {
                c.width == 1
                    && !c.continuation
                    && c.text.len() == 1
                    && c.text.is_ascii()
                    && c.fg == first.fg
                    && c.bg == first.bg
                    && c.fg_source == first.fg_source
                    && c.bg_source == first.bg_source
                    && c.bold == first.bold
                    && c.inverse == first.inverse
            }) {
                let text = cells.iter().map(|c| c.text.as_str()).collect::<String>();
                return Self::Compact {
                    text: smol_str::SmolStr::new(text.trim_end()),
                    rendition: Rendition {
                        fg: first.fg,
                        bg: first.bg,
                        fg_source: first.fg_source,
                        bg_source: first.bg_source,
                        bold: first.bold,
                        inverse: first.inverse,
                    },
                    columns: cells.len(),
                };
            }
        }
        Self::Full(cells)
    }
    pub fn cells(&self) -> Vec<Cell> {
        match self {
            Self::Full(cells) => cells.clone(),
            Self::Compact {
                text,
                rendition,
                columns,
            } => {
                let template = Cell {
                    text: " ".into(),
                    fg: rendition.fg,
                    bg: rendition.bg,
                    fg_source: rendition.fg_source,
                    bg_source: rendition.bg_source,
                    bold: rendition.bold,
                    inverse: rendition.inverse,
                    width: 1,
                    continuation: false,
                };
                let mut cells = vec![template; *columns];
                for (col, byte) in text.bytes().enumerate() {
                    let mut buffer = [0; 4];
                    cells[col].text =
                        smol_str::SmolStr::new_inline(char::from(byte).encode_utf8(&mut buffer));
                }
                cells
            }
        }
    }
    pub fn plain(&self) -> String {
        match self {
            Self::Compact { text, .. } => text.to_string(),
            Self::Full(cells) => super::Screen::plain_line(cells),
        }
    }
    pub fn memory(&self) -> usize {
        // Reserve ring capacity and allocator overhead as well as retained text.
        let base = 2 * std::mem::size_of::<Self>();
        match self {
            Self::Compact { text, .. } => {
                base + if text.is_heap_allocated() {
                    text.len() + 64
                } else {
                    0
                }
            }
            Self::Full(cells) => {
                base + 32
                    + cells.capacity() * std::mem::size_of::<Cell>()
                    + cells
                        .iter()
                        .filter(|c| c.text.is_heap_allocated())
                        .map(|c| c.text.len() + 64)
                        .sum::<usize>()
            }
        }
    }
}
pub struct HistoryView<'a> {
    pub(super) lines: &'a VecDeque<HistoryLine>,
}
impl<'a> HistoryView<'a> {
    pub fn len(&self) -> usize {
        self.lines.len()
    }
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }
    pub fn get(&self, index: usize) -> Option<Vec<Cell>> {
        self.lines.get(index).map(HistoryLine::cells)
    }
    pub fn iter(&self) -> impl DoubleEndedIterator<Item = Vec<Cell>> + 'a {
        self.lines.iter().map(HistoryLine::cells)
    }
}
