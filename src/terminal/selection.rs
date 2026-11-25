//! Selection uses cell boundaries and stable retained-line IDs, never UTF-8 character offsets.
use super::{Cell, Screen};
use crate::model::{ClipboardSettings, RightClickBehavior};
use std::{borrow::Cow, ops::Range};
const MAX_COPY_BYTES: usize = 8 * 1024 * 1024;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SelectionMode {
    #[default]
    Linear,
    Rectangle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SelectionPoint {
    pub row: usize,
    pub col: usize,
}
#[derive(Clone, Debug)]
pub struct TerminalSelection {
    anchor: SelectionPoint,
    focus: SelectionPoint,
    base: u64,
    epoch: u64,
}
fn rows(screen: &Screen) -> usize {
    if screen.is_alternate() {
        screen.rows
    } else {
        screen.history().len() + screen.rows
    }
}
fn cells(screen: &Screen, row: usize) -> Option<Cow<'_, [Cell]>> {
    let history = if screen.is_alternate() {
        0
    } else {
        screen.history().len()
    };
    if row < history {
        screen.history().get(row).map(Cow::Owned)
    } else {
        screen
            .grid()
            .get(row - history)
            .map(|cells| Cow::Borrowed(cells.as_slice()))
    }
}
impl TerminalSelection {
    /// Columns are boundary offsets: selecting 1..3 copies columns 1 and 2.
    pub fn new(screen: &Screen, anchor: SelectionPoint, focus: SelectionPoint) -> Self {
        Self {
            anchor: SelectionPoint {
                row: anchor.row,
                col: anchor.col.min(500),
            },
            focus: SelectionPoint {
                row: focus.row,
                col: focus.col.min(500),
            },
            base: screen.first_line_id(),
            epoch: screen.selection_epoch(),
        }
    }
    fn points(&self, screen: &Screen) -> Option<(SelectionPoint, SelectionPoint)> {
        if self.epoch != screen.selection_epoch() {
            return None;
        }
        let point = |p: SelectionPoint| {
            let id = self.base.checked_add(p.row as u64)?;
            let row = usize::try_from(id.checked_sub(screen.first_line_id())?).ok()?;
            (row < rows(screen)).then_some(SelectionPoint { row, col: p.col })
        };
        Some((point(self.anchor)?, point(self.focus)?))
    }
    pub fn columns_for_row(
        &self,
        screen: &Screen,
        row: usize,
        mode: SelectionMode,
    ) -> Option<Range<usize>> {
        let (a, b) = self.points(screen)?;
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        if row < start.row || row > end.row {
            return None;
        }
        Some(match mode {
            SelectionMode::Rectangle => a.col.min(b.col)..a.col.max(b.col),
            SelectionMode::Linear => {
                let begin = if row == start.row { start.col } else { 0 };
                let end = if row == end.row {
                    end.col
                } else {
                    cells(screen, row)?.len()
                };
                begin..end
            }
        })
    }
    pub fn extract(&self, screen: &Screen, mode: SelectionMode) -> String {
        self.try_extract(screen, mode).unwrap_or_default()
    }
    pub fn try_extract(&self, screen: &Screen, mode: SelectionMode) -> Result<String, String> {
        let Some((a, b)) = self.points(screen) else {
            return Ok(String::new());
        };
        let mut output = String::new();
        for row in a.row.min(b.row)..=a.row.max(b.row) {
            let Some(cells) = cells(screen, row) else {
                break;
            };
            let range = self.columns_for_row(screen, row, mode).unwrap();
            let mut line = String::new();
            for (col, cell) in cells.iter().enumerate() {
                if !cell.continuation
                    && col < range.end
                    && col + cell.width.max(1) as usize > range.start
                {
                    line.push_str(&cell.text);
                }
            }
            if mode == SelectionMode::Rectangle && range.end > cells.len() {
                line.extend(std::iter::repeat_n(
                    ' ',
                    range.end - range.start.max(cells.len()),
                ));
            }
            if mode == SelectionMode::Linear {
                line.truncate(line.trim_end_matches(' ').len());
            }
            if output.len().saturating_add(line.len()).saturating_add(1) > MAX_COPY_BYTES {
                return Err(
                    "Selection exceeds 8 MiB. Select less text or export terminal output.".into(),
                );
            }
            if row > a.row.min(b.row) {
                output.push('\n');
            }
            output.push_str(&line);
        }
        Ok(output)
    }
    pub fn word(screen: &Screen, point: SelectionPoint, delimiters: &str) -> Option<Self> {
        let cells = cells(screen, point.row)?;
        let mut col = point.col.min(cells.len().checked_sub(1)?);
        while col > 0 && cells[col].continuation {
            col -= 1;
        }
        let delimiter = |cell: &Cell| {
            cell.text
                .chars()
                .any(|c| c.is_whitespace() || delimiters.contains(c))
        };
        let mut start = col;
        let mut end = col + cells[col].width.max(1) as usize;
        if !delimiter(&cells[col]) {
            while start > 0 {
                let mut previous = start - 1;
                while previous > 0 && cells[previous].continuation {
                    previous -= 1;
                }
                if delimiter(&cells[previous]) {
                    break;
                }
                start = previous;
            }
            while end < cells.len() && !delimiter(&cells[end]) {
                end += cells[end].width.max(1) as usize;
            }
        }
        Some(Self::new(
            screen,
            SelectionPoint {
                row: point.row,
                col: start,
            },
            SelectionPoint {
                row: point.row,
                col: end,
            },
        ))
    }
}
#[derive(Clone, Copy, Debug)]
pub enum InputEvent {
    RightClick { shift: bool },
    CopyShortcut,
    PasteShortcut,
    SelectionFinished,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardAction {
    Copy,
    Paste,
    Menu,
}
impl ClipboardAction {
    pub fn from_input(event: &InputEvent, settings: &ClipboardSettings) -> Option<Self> {
        match event {
            InputEvent::RightClick { shift } => Some(
                if *shift || settings.right_click == RightClickBehavior::ContextMenu {
                    Self::Menu
                } else {
                    Self::Paste
                },
            ),
            InputEvent::CopyShortcut => Some(Self::Copy),
            InputEvent::PasteShortcut => Some(Self::Paste),
            InputEvent::SelectionFinished => settings.copy_on_select.then_some(Self::Copy),
        }
    }
}
