//! A streaming VT screen. The transport and UI never parse terminal control text.
use std::{cell::RefCell, collections::VecDeque};
#[path = "terminal/conformance.rs"]
mod conformance;
#[path = "terminal/highlight.rs"]
pub mod highlight;
#[path = "terminal/history.rs"]
mod history;
#[path = "terminal/modes.rs"]
mod modes;
#[path = "terminal/mouse.rs"]
pub mod mouse;
#[path = "terminal/paste.rs"]
pub mod paste;
#[path = "terminal/selection.rs"]
pub mod selection;
#[path = "terminal/settings.rs"]
pub mod settings;
#[path = "terminal/unicode.rs"]
mod unicode;
pub use conformance::ScreenStats;
use conformance::{DEFAULT_HISTORY_BUDGET, SearchIndex};
use history::HistoryLine;
pub use history::HistoryView;
#[derive(Clone, Copy, Debug, PartialEq, Hash)]
pub struct Ink(pub u8, pub u8, pub u8);
pub const FOREGROUND: Ink = Ink(236, 236, 236);
pub const BACKGROUND: Ink = Ink(30, 30, 30);
pub const PALETTE: [Ink; 16] = [
    Ink(30, 30, 30),
    Ink(255, 96, 96),
    Ink(14, 177, 108),
    Ink(190, 190, 18),
    Ink(18, 150, 190),
    Ink(255, 77, 255),
    Ink(84, 204, 239),
    Ink(204, 204, 204),
    Ink(128, 128, 128),
    Ink(255, 128, 128),
    Ink(24, 237, 147),
    Ink(222, 220, 18),
    Ink(27, 186, 233),
    Ink(255, 125, 255),
    Ink(142, 221, 244),
    Ink(255, 255, 255),
];
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ColorSource {
    #[default]
    Default,
    Indexed(u8),
    Rgb,
}
#[derive(Clone, Debug, Hash)]
pub struct Cell {
    pub text: smol_str::SmolStr,
    pub fg: Ink,
    pub bg: Ink,
    pub fg_source: ColorSource,
    pub bg_source: ColorSource,
    pub bold: bool,
    pub inverse: bool,
    pub width: u8,
    pub continuation: bool,
}
impl Default for Cell {
    fn default() -> Self {
        Self {
            text: " ".into(),
            fg: FOREGROUND,
            bg: BACKGROUND,
            fg_source: ColorSource::Default,
            bg_source: ColorSource::Default,
            bold: false,
            inverse: false,
            width: 1,
            continuation: false,
        }
    }
}
#[derive(Clone)]
struct Saved {
    grid: Vec<Vec<Cell>>,
    row: usize,
    col: usize,
    wrap_pending: bool,
}
pub struct Screen {
    grid: Vec<Vec<Cell>>,
    history: VecDeque<HistoryLine>,
    pub cols: usize,
    pub rows: usize,
    pub row: usize,
    pub col: usize,
    pub cursor_visible: bool,
    pub bracketed_paste: bool,
    pub app_cursor: bool,
    mouse_mode: mouse::MouseMode,
    pub title: String,
    pub replies: Vec<u8>,
    cap: usize,
    style: Cell,
    pending: Vec<u8>,
    escape: u8,
    sequence: String,
    osc_esc: bool,
    saved_cursor: (usize, usize, bool),
    saved_style: Cell,
    saved_modes: (bool, bool),
    saved_charsets: ([bool; 2], usize),
    origin_mode: bool,
    insert_mode: bool,
    charsets: [bool; 2],
    charset_slot: usize,
    active_charset: usize,
    alternate: Option<Saved>,
    top: usize,
    bottom: usize,
    autowrap: bool,
    wrap_pending: bool,
    last_printed: Option<(usize, usize)>,
    revision: u64,
    activity_revision: u64,
    search_index: RefCell<SearchIndex>,
    history_bytes: usize,
    history_text: VecDeque<String>,
    history_start: u64,
    selection_epoch: u64,
    history_budget: usize,
    discarded_lines: u64,
}
impl Screen {
    pub fn new(cols: usize, rows: usize, cap: usize) -> Self {
        let cols = cols.clamp(2, 500);
        let rows = rows.clamp(2, 200);
        Self {
            grid: vec![vec![Cell::default(); cols]; rows],
            history: VecDeque::new(),
            cols,
            rows,
            row: 0,
            col: 0,
            cursor_visible: true,
            bracketed_paste: false,
            app_cursor: false,
            mouse_mode: mouse::MouseMode::default(),
            title: String::new(),
            replies: Vec::new(),
            cap,
            style: Cell::default(),
            pending: Vec::new(),
            escape: 0,
            sequence: String::new(),
            osc_esc: false,
            saved_cursor: (0, 0, false),
            saved_style: Cell::default(),
            saved_modes: (false, true),
            saved_charsets: ([false; 2], 0),
            origin_mode: false,
            insert_mode: false,
            charsets: [false; 2],
            charset_slot: 0,
            active_charset: 0,
            alternate: None,
            top: 0,
            bottom: rows - 1,
            autowrap: true,
            wrap_pending: false,
            last_printed: None,
            revision: 0,
            activity_revision: 0,
            search_index: RefCell::new(SearchIndex::default()),
            history_bytes: 0,
            history_text: VecDeque::new(),
            history_start: 0,
            selection_epoch: 0,
            history_budget: DEFAULT_HISTORY_BUDGET,
            discarded_lines: 0,
        }
    }
    pub fn mouse_mode(&self) -> mouse::MouseMode {
        self.mouse_mode
    }
    pub fn is_alternate(&self) -> bool {
        self.alternate.is_some()
    }
    pub fn resize(&mut self, cols: usize, rows: usize) {
        let cols = cols.clamp(2, 500);
        let rows = rows.clamp(2, 200);
        if cols == self.cols && rows == self.rows {
            return;
        }
        let previous_cols = self.cols;
        let was_pending = self.wrap_pending;
        let last = self.last_printed;
        let last_text =
            last.and_then(|(row, col)| self.cell(row, col).map(|cell| cell.text.clone()));
        let removed = self.resize_live_rows(rows, !self.is_alternate());
        let last = last.and_then(|(row, col)| {
            (row >= removed && row < removed + rows).then(|| (row - removed, col))
        });
        for line in &mut self.grid {
            line.resize(cols, Cell::default());
        }
        self.grid.resize(rows, vec![Cell::default(); cols]);
        self.cols = cols;
        self.rows = rows;
        self.row = self.row.min(rows - 1);
        self.col = self.col.min(cols - 1);
        if was_pending && cols > previous_cols {
            self.col = (self.col + 1).min(cols - 1);
        }
        self.top = 0;
        self.bottom = rows - 1;
        self.wrap_pending = was_pending && cols <= previous_cols && self.col == cols - 1;
        for line in &mut self.grid {
            Self::normalize(line);
        }
        self.last_printed = last.filter(|(row, col)| {
            self.cell(*row, *col)
                .is_some_and(|cell| Some(&cell.text) == last_text.as_ref() && !cell.continuation)
        });
        self.revision = self.revision.wrapping_add(1);
    }
    fn resize_live_rows(&mut self, rows: usize, primary: bool) -> usize {
        let removed = self
            .row
            .saturating_add(1)
            .saturating_sub(rows)
            .min(self.grid.len().saturating_sub(rows));
        let lines: Vec<_> = self.grid.drain(..removed).collect();
        if primary {
            for line in lines {
                self.remember_line(line);
            }
        }
        self.row = self.row.saturating_sub(removed);
        self.grid.resize(rows, vec![Cell::default(); self.cols]);
        removed
    }
    pub fn selection_epoch(&self) -> u64 {
        self.selection_epoch
    }
    pub fn first_line_id(&self) -> u64 {
        if self.is_alternate() {
            0
        } else {
            self.history_start
        }
    }
    pub fn clear(&mut self) {
        self.selection_epoch = self.selection_epoch.wrapping_add(1);
        self.grid = vec![vec![Cell::default(); self.cols]; self.rows];
        if !self.is_alternate() {
            self.clear_history();
            self.discarded_lines = 0;
        }
        self.wrap_pending = false;
        self.last_printed = None;
        self.revision = self.revision.wrapping_add(1);
        self.row = 0;
        self.col = 0;
    }
    pub fn cell(&self, row: usize, col: usize) -> Option<&Cell> {
        self.grid.get(row)?.get(col)
    }
    pub fn grid(&self) -> &[Vec<Cell>] {
        &self.grid
    }
    pub fn history(&self) -> HistoryView<'_> {
        HistoryView {
            lines: &self.history,
        }
    }
    fn plain_line(line: &[Cell]) -> String {
        let mut text = line.iter().map(|c| c.text.as_str()).collect::<String>();
        let length = text.trim_end().len();
        text.truncate(length);
        text
    }
    pub fn plain(&self) -> String {
        let mut text = String::new();
        for line in self.history_text.iter().take(if self.is_alternate() {
            0
        } else {
            self.history_text.len()
        }) {
            text.push_str(line);
            text.push('\n');
        }
        for line in &self.grid {
            text.push_str(&Self::plain_line(line));
            text.push('\n');
        }
        text.pop();
        text
    }
    pub fn search_count(&self, query: &str) -> usize {
        if query.is_empty() {
            return 0;
        }
        let mut index = self.search_index.borrow_mut();
        if index.revision == Some(self.revision) && index.query == query {
            return index.count;
        }
        if index.query != query {
            index.history_counts.clear();
            index.history_total = 0;
            index.history_start = self.history_start;
            index.query = query.to_owned();
        }
        if self.is_alternate() {
            index.count = self.plain().matches(query).count();
        } else if query.contains('\n') {
            index.count = self.plain().matches(query).count();
            index.indexed_history_lines += self.history.len() as u64;
        } else {
            let removed = self.history_start.saturating_sub(index.history_start) as usize;
            if removed >= index.history_counts.len() {
                index.history_counts.clear();
                index.history_total = 0;
            } else {
                for _ in 0..removed {
                    if let Some(count) = index.history_counts.pop_front() {
                        index.history_total -= count;
                    }
                }
            }
            index.history_start = self.history_start;
            for line in self.history_text.iter().skip(index.history_counts.len()) {
                let count = line.matches(query).count();
                index.history_total += count;
                index.history_counts.push_back(count);
                index.indexed_history_lines += 1;
            }
            index.count = index.history_total
                + self
                    .grid
                    .iter()
                    .map(|line| Self::plain_line(line).matches(query).count())
                    .sum::<usize>();
        }
        index.revision = Some(self.revision);
        index.builds += 1;
        index.count
    }
    pub fn stats(&self) -> ScreenStats {
        let index = self.search_index.borrow();
        ScreenStats {
            history_bytes: self.history_bytes,
            history_budget: self.history_budget,
            discarded_lines: self.discarded_lines,
            index_builds: index.builds,
            indexed_history_lines: index.indexed_history_lines,
        }
    }
    pub fn set_history_memory_budget(&mut self, bytes: usize) {
        self.history_budget = bytes;
        self.trim_history();
        self.history.shrink_to_fit();
        self.history_text.shrink_to_fit();
        let mut index = self.search_index.borrow_mut();
        index.history_counts.clear();
        index.history_counts.shrink_to_fit();
        index.query.clear();
        index.revision = None;
        drop(index);
        self.revision = self.revision.wrapping_add(1);
    }
    fn row_memory(line: &HistoryLine, text: &String) -> usize {
        line.memory()
            + 2 * std::mem::size_of::<String>()
            + 2 * std::mem::size_of::<usize>()
            + text.capacity()
    }
    fn clear_history(&mut self) {
        self.history_start = self.history_start.wrapping_add(self.history.len() as u64);
        self.history = VecDeque::new();
        self.history_text = VecDeque::new();
        self.history_bytes = 0;
        let index = self.search_index.get_mut();
        index.history_counts = VecDeque::new();
        index.history_total = 0;
        index.history_start = self.history_start;
        index.revision = None;
    }
    fn trim_history(&mut self) {
        while self.history.len() > self.cap || self.history_bytes > self.history_budget {
            if let Some(line) = self.history.pop_front() {
                let text = self.history_text.pop_front().unwrap();
                self.history_bytes = self
                    .history_bytes
                    .saturating_sub(Self::row_memory(&line, &text));
                self.discarded_lines += 1;
                self.history_start = self.history_start.wrapping_add(1);
            } else {
                break;
            }
        }
    }
    fn remember_line(&mut self, line: Vec<Cell>) {
        let line = HistoryLine::new(line);
        let text = line.plain();
        self.history_bytes = self
            .history_bytes
            .saturating_add(Self::row_memory(&line, &text));
        self.history.push_back(line);
        self.history_text.push_back(text);
        self.trim_history();
    }
    /// O(1) version of committed display operations; parser/protocol-only input does not advance it.
    pub fn activity_signature(&self) -> u64 {
        self.activity_revision
    }
    fn activity_meta(&self) -> (usize, usize, bool, bool, u64, usize) {
        (
            self.row,
            self.col,
            self.cursor_visible,
            self.is_alternate(),
            self.first_line_id(),
            self.history().len(),
        )
    }
    pub fn feed(&mut self, data: &[u8]) {
        let before = self.activity_meta();
        if !data.is_empty() {
            self.revision = self.revision.wrapping_add(1);
        }
        self.pending.extend_from_slice(data);
        let bytes = std::mem::take(&mut self.pending);
        let mut i = 0;
        while i < bytes.len() {
            let b = bytes[i];
            if b < 128 {
                self.character(b as char);
                i += 1;
            } else {
                let len = if b & 0xe0 == 0xc0 {
                    2
                } else if b & 0xf0 == 0xe0 {
                    3
                } else if b & 0xf8 == 0xf0 {
                    4
                } else {
                    1
                };
                if i + len > bytes.len() {
                    self.pending.extend_from_slice(&bytes[i..]);
                    break;
                }
                if let Ok(s) = std::str::from_utf8(&bytes[i..i + len]) {
                    for ch in s.chars() {
                        self.character(ch);
                    }
                    i += len;
                } else {
                    self.character('�');
                    i += 1;
                }
            }
        }
        if self.activity_meta() != before {
            self.activity_revision = self.activity_revision.wrapping_add(1);
        }
    }
    fn character(&mut self, c: char) {
        if matches!(c, '\x18' | '\x1a') {
            self.escape = 0;
            self.sequence.clear();
            self.osc_esc = false;
            self.wrap_pending = false;
            self.last_printed = None;
            return;
        }
        if c == '\x1b' && matches!(self.escape, 2 | 4) {
            self.escape = 1;
            self.sequence.clear();
            return;
        }
        match self.escape {
            1 => {
                self.escape = 0;
                match c {
                    '[' => {
                        self.escape = 2;
                        self.sequence.clear();
                    }
                    'P' | '_' | '^' | 'X' => {
                        self.escape = 5;
                        self.osc_esc = false;
                    }
                    ']' => {
                        self.escape = 3;
                        self.sequence.clear();
                    }
                    '7' => {
                        self.saved_cursor = (
                            self.row,
                            self.col + usize::from(self.wrap_pending),
                            self.wrap_pending,
                        );
                        self.saved_style = self.style.clone();
                        self.saved_modes = (self.origin_mode, self.autowrap);
                        self.saved_charsets = (self.charsets, self.active_charset);
                        self.last_printed = None;
                    }
                    '8' => {
                        self.origin_mode = self.saved_modes.0;
                        self.autowrap = self.saved_modes.1;
                        self.row = self.saved_cursor.0.min(self.rows - 1);
                        if self.origin_mode {
                            self.row = self.row.clamp(self.top, self.bottom);
                        }
                        self.col = self.saved_cursor.1.min(self.cols - 1);
                        self.wrap_pending = self.saved_cursor.2 && self.saved_cursor.1 >= self.cols;
                        self.style = self.saved_style.clone();
                        self.charsets = self.saved_charsets.0;
                        self.active_charset = self.saved_charsets.1;
                        self.last_printed = None;
                    }
                    'D' => self.newline(),
                    'M' => {
                        self.activity_revision = self.activity_revision.wrapping_add(1);
                        self.wrap_pending = false;
                        self.last_printed = None;
                        if self.row == self.top {
                            self.grid.remove(self.bottom);
                            self.grid.insert(self.top, vec![Cell::default(); self.cols]);
                        } else {
                            self.row = self.row.saturating_sub(1);
                        }
                    }
                    'E' => {
                        self.col = 0;
                        self.newline();
                    }
                    'c' => {
                        let budget = self.history_budget;
                        let epoch = self.selection_epoch.wrapping_add(1);
                        *self = Self::new(self.cols, self.rows, self.cap);
                        self.history_budget = budget;
                        self.selection_epoch = epoch;
                    }
                    '(' | ')' => {
                        self.escape = 4;
                        self.charset_slot = if c == '(' { 0 } else { 1 };
                        self.last_printed = None;
                    }
                    _ => {}
                }
                return;
            }
            2 => {
                if ('@'..='~').contains(&c) {
                    let s = std::mem::take(&mut self.sequence);
                    self.escape = 0;
                    self.csi(&s, c);
                } else if self.sequence.len() < 4096 {
                    self.sequence.push(c);
                } else {
                    self.escape = 0;
                }
                return;
            }
            3 => {
                if c == '\x07' || (self.osc_esc && c == '\\') {
                    if let Some((kind, title)) = self.sequence.split_once(';') {
                        if kind == "0" || kind == "2" {
                            let title: String = title
                                .chars()
                                .filter(|c| !c.is_control())
                                .take(120)
                                .collect();
                            if self.title != title {
                                self.title = title;
                                self.activity_revision = self.activity_revision.wrapping_add(1);
                            }
                        }
                    }
                    self.escape = 0;
                    self.osc_esc = false;
                } else {
                    self.osc_esc = c == '\x1b';
                    if self.sequence.len() < 4096 && !self.osc_esc {
                        self.sequence.push(c);
                    }
                }
                return;
            }
            5 => {
                if self.osc_esc && c == '\\' {
                    self.escape = 0;
                    self.osc_esc = false;
                } else {
                    self.osc_esc = c == '\x1b';
                }
                return;
            }
            4 => {
                self.charsets[self.charset_slot] = c == '0';
                self.escape = 0;
                return;
            }
            _ => {}
        }
        if c.is_control() && c != '\x1b' {
            self.wrap_pending = false;
            self.last_printed = None;
        }
        match c {
            '\x1b' => self.escape = 1,
            '\r' => self.col = 0,
            '\n' | '\x0b' | '\x0c' => self.newline(),
            '\x08' => self.col = self.col.saturating_sub(1),
            '\x0e' => self.active_charset = 1,
            '\x0f' => self.active_charset = 0,
            '\t' => self.col = ((self.col / 8 + 1) * 8).min(self.cols - 1),
            c if c.is_control() => {}
            _ => self.put(c),
        }
    }
    fn styled(&self, text: String, width: u8, continuation: bool) -> Cell {
        Cell {
            text: text.into(),
            fg: self.style.fg,
            bg: self.style.bg,
            fg_source: self.style.fg_source,
            bg_source: self.style.bg_source,
            bold: self.style.bold,
            inverse: self.style.inverse,
            width,
            continuation,
        }
    }
    fn erase_glyph(&mut self, row: usize, col: usize) {
        let mut start = col;
        while start > 0 && self.grid[row][start].continuation {
            start -= 1;
        }
        let width = self.grid[row][start].width.max(1) as usize;
        for i in start..(start + width).min(self.cols) {
            self.grid[row][i] = self.blank();
        }
    }
    fn normalize(line: &mut [Cell]) {
        Self::normalize_with(line, &Cell::default());
    }
    fn normalize_with(line: &mut [Cell], blank: &Cell) {
        let mut col = 0;
        while col < line.len() {
            if line[col].continuation {
                line[col] = blank.clone();
                col += 1;
                continue;
            }
            let width = line[col].width.max(1) as usize;
            if width > 1
                && (col + width > line.len()
                    || !(col + 1..col + width).all(|i| line[i].continuation))
            {
                line[col] = blank.clone();
                col += 1;
            } else {
                col += width;
            }
        }
    }
    fn put(&mut self, c: char) {
        self.activity_revision = self.activity_revision.wrapping_add(1);
        let c = if self.charsets[self.active_charset] {
            modes::graphics(c)
        } else {
            c
        };
        if let Some((row, col)) = self.last_printed {
            if let Some(text) = unicode::extension(&self.grid[row][col].text, c) {
                let mut old = self.grid[row][col].clone();
                let width = unicode::width(&text).max(1).min(self.cols);
                self.erase_glyph(row, col);
                self.row = row;
                self.col = col;
                self.wrap_pending = false;
                if col + width > self.cols && self.autowrap {
                    self.col = 0;
                    self.newline();
                    old.width = 0;
                }
                self.place(text.into(), width, Some(old));
                return;
            }
        }
        if unicode::char_width(c) == 0 {
            let text = format!(" {c}");
            self.erase_glyph(self.row, self.col);
            self.grid[self.row][self.col] = self.styled(text, 1, false);
            self.last_printed = Some((self.row, self.col));
            return;
        }
        let width = unicode::char_width(c).max(1).min(self.cols);
        if self.wrap_pending && self.autowrap || self.col + width > self.cols && self.autowrap {
            self.col = 0;
            self.newline();
        }
        let mut bytes = [0; 4];
        self.place(
            smol_str::SmolStr::new(c.encode_utf8(&mut bytes)),
            width,
            None,
        );
    }
    fn place(&mut self, mut text: smol_str::SmolStr, mut width: usize, style: Option<Cell>) {
        if self.col + width > self.cols {
            text = "�".into();
            width = 1;
        }
        let col = self.col;
        if self.insert_mode {
            let previous = style.as_ref().map(|cell| cell.width as usize).unwrap_or(0);
            if previous > width {
                for _ in 0..previous - width {
                    self.grid[self.row].remove((col + width).min(self.cols - 1));
                    let blank = self.blank();
                    self.grid[self.row].push(blank);
                }
            }
            for _ in 0..width.saturating_sub(previous) {
                self.grid[self.row].pop();
                let blank = self.blank();
                self.grid[self.row].insert((col + previous).min(self.cols - 1), blank);
            }
            Self::normalize(&mut self.grid[self.row]);
        }
        for i in col..col + width {
            self.erase_glyph(self.row, i);
        }
        let mut cell = style.unwrap_or_else(|| self.styled(String::new(), 1, false));
        cell.text = text;
        cell.width = width as u8;
        cell.continuation = false;
        self.grid[self.row][col] = cell;
        for i in col + 1..col + width {
            let mut next = self.grid[self.row][col].clone();
            next.text = smol_str::SmolStr::default();
            next.width = 0;
            next.continuation = true;
            self.grid[self.row][i] = next;
        }
        self.last_printed = Some((self.row, col));
        self.wrap_pending = col + width >= self.cols;
        self.col = (col + width).min(self.cols - 1);
    }
    fn newline(&mut self) {
        self.wrap_pending = false;
        self.last_printed = None;
        if self.row == self.bottom {
            self.activity_revision = self.activity_revision.wrapping_add(1);
            let line = self.grid.remove(self.top);
            if self.top == 0 && self.bottom == self.rows - 1 && self.alternate.is_none() {
                self.remember_line(line);
            }
            self.grid.insert(self.bottom, vec![self.blank(); self.cols]);
        } else {
            self.row = (self.row + 1).min(self.rows - 1);
        }
    }
    fn blank(&self) -> Cell {
        let mut s = self.style.clone();
        s.text = " ".into();
        s.width = 1;
        s.continuation = false;
        s
    }
    fn csi(&mut self, s: &str, final_char: char) {
        if matches!(
            final_char,
            'J' | 'K' | 'P' | '@' | 'X' | 'L' | 'M' | 'S' | 'T'
        ) {
            self.activity_revision = self.activity_revision.wrapping_add(1);
        }
        if matches!(
            final_char,
            'A' | 'B'
                | 'C'
                | 'D'
                | 'E'
                | 'F'
                | 'G'
                | 'H'
                | 'f'
                | 'd'
                | '`'
                | 'a'
                | 'J'
                | 'K'
                | 'P'
                | '@'
                | 'X'
                | 'L'
                | 'M'
                | 'S'
                | 'T'
                | 'r'
        ) {
            self.wrap_pending = false;
            self.last_printed = None;
        }
        let private = s.starts_with('?');
        let params: Vec<usize> = s
            .trim_start_matches(['?', '>', '!'])
            .split(';')
            .map(|p| p.parse::<usize>().unwrap_or(0).min(65535))
            .collect();
        let p = |i: usize, default: usize| {
            params
                .get(i)
                .copied()
                .filter(|v| *v != 0)
                .unwrap_or(default)
        };
        match final_char {
            'A' => {
                self.row = self.row.saturating_sub(p(0, 1)).max(if self.origin_mode {
                    self.top
                } else {
                    0
                })
            }
            'B' => {
                self.row = (self.row + p(0, 1)).min(if self.origin_mode {
                    self.bottom
                } else {
                    self.rows - 1
                })
            }
            'C' | 'a' => self.col = (self.col + p(0, 1)).min(self.cols - 1),
            'D' => self.col = self.col.saturating_sub(p(0, 1)),
            'E' => {
                self.row = (self.row + p(0, 1)).min(if self.origin_mode {
                    self.bottom
                } else {
                    self.rows - 1
                });
                self.col = 0;
            }
            'F' => {
                self.row = self.row.saturating_sub(p(0, 1)).max(if self.origin_mode {
                    self.top
                } else {
                    0
                });
                self.col = 0;
            }
            'G' | '`' => self.col = (p(0, 1) - 1).min(self.cols - 1),
            'd' => {
                self.row = if self.origin_mode {
                    (self.top + p(0, 1) - 1).min(self.bottom)
                } else {
                    (p(0, 1) - 1).min(self.rows - 1)
                }
            }
            'H' | 'f' => {
                self.row = if self.origin_mode {
                    (self.top + p(0, 1) - 1).min(self.bottom)
                } else {
                    (p(0, 1) - 1).min(self.rows - 1)
                };
                self.col = (p(1, 1) - 1).min(self.cols - 1);
            }
            'J' => {
                let mode = params[0];
                let blank = self.blank();
                match mode {
                    2 | 3 => {
                        self.grid = vec![vec![blank; self.cols]; self.rows];
                        if mode == 3 {
                            self.clear_history();
                        }
                    }
                    1 => {
                        for r in 0..self.row {
                            self.grid[r].fill(blank.clone());
                        }
                        self.grid[self.row][..=self.col.min(self.cols - 1)].fill(blank);
                    }
                    _ => {
                        self.grid[self.row][self.col.min(self.cols)..].fill(blank.clone());
                        for r in self.row + 1..self.rows {
                            self.grid[r].fill(blank.clone());
                        }
                    }
                }
            }
            'K' => {
                let blank = self.blank();
                match params[0] {
                    1 => self.grid[self.row][..=self.col.min(self.cols - 1)].fill(blank),
                    2 => self.grid[self.row].fill(blank),
                    _ => self.grid[self.row][self.col.min(self.cols)..].fill(blank),
                }
            }
            'm' => self.sgr(&params),
            's' => {
                self.saved_cursor = (
                    self.row,
                    self.col + usize::from(self.wrap_pending),
                    self.wrap_pending,
                )
            }
            'u' => {
                self.row = self.saved_cursor.0.min(self.rows - 1);
                self.col = self.saved_cursor.1.min(self.cols - 1);
                self.wrap_pending = self.saved_cursor.2 && self.saved_cursor.1 >= self.cols;
                self.last_printed = None;
            }
            'r' => {
                let t = p(0, 1) - 1;
                let b = p(1, self.rows) - 1;
                if t < b && b < self.rows {
                    self.top = t;
                    self.bottom = b;
                    self.row = if self.origin_mode { self.top } else { 0 };
                    self.col = 0;
                }
            }
            'S' => {
                for _ in 0..p(0, 1).min(self.rows) {
                    let r = self.row;
                    self.row = self.bottom;
                    self.newline();
                    self.row = r;
                }
            }
            'T' => {
                for _ in 0..p(0, 1).min(self.rows) {
                    self.grid.remove(self.bottom);
                    self.grid.insert(self.top, vec![self.blank(); self.cols]);
                }
            }
            'L' | 'M' => {
                if self.row >= self.top && self.row <= self.bottom {
                    for _ in 0..p(0, 1).min(self.bottom - self.row + 1) {
                        if final_char == 'L' {
                            self.grid.remove(self.bottom);
                            self.grid.insert(self.row, vec![self.blank(); self.cols]);
                        } else {
                            self.grid.remove(self.row);
                            self.grid.insert(self.bottom, vec![self.blank(); self.cols]);
                        }
                    }
                }
            }
            'P' | '@' | 'X' => {
                let col = self.col.min(self.cols - 1);
                let n = p(0, 1).min(self.cols - col);
                let blank = self.blank();
                if final_char == 'X' {
                    self.grid[self.row][col..col + n].fill(blank);
                } else {
                    for _ in 0..n {
                        if final_char == 'P' {
                            self.grid[self.row].remove(col);
                            self.grid[self.row].push(blank.clone());
                        } else {
                            self.grid[self.row].pop();
                            self.grid[self.row].insert(col, blank.clone());
                        }
                    }
                }
            }
            'h' | 'l' if private => {
                let on = final_char == 'h';
                for n in params {
                    match n {
                        1 => self.app_cursor = on,
                        6 => {
                            self.wrap_pending = false;
                            self.last_printed = None;
                            self.origin_mode = on;
                            self.row = if on { self.top } else { 0 };
                            self.col = 0;
                        }
                        7 => {
                            self.autowrap = on;
                            if !on {
                                self.wrap_pending = false;
                            }
                        }
                        25 => self.cursor_visible = on,
                        2004 => self.bracketed_paste = on,
                        1000 | 1002 | 1003 | 1006 => self.mouse_mode.set(n, on),
                        47 | 1047 | 1049 => {
                            if on != self.alternate.is_some() {
                                self.selection_epoch = self.selection_epoch.wrapping_add(1);
                            }
                            if on && self.alternate.is_none() {
                                self.alternate = Some(Saved {
                                    grid: std::mem::replace(
                                        &mut self.grid,
                                        vec![vec![Cell::default(); self.cols]; self.rows],
                                    ),
                                    row: self.row,
                                    col: self.col,
                                    wrap_pending: self.wrap_pending,
                                });
                                self.row = 0;
                                self.col = 0;
                                self.wrap_pending = false;
                                self.last_printed = None;
                            } else if !on {
                                if let Some(saved) = self.alternate.take() {
                                    let previous_cols =
                                        saved.grid.first().map(Vec::len).unwrap_or(self.cols);
                                    self.grid = saved.grid;
                                    self.row = saved.row;
                                    self.resize_live_rows(self.rows, true);
                                    for line in &mut self.grid {
                                        line.resize(self.cols, Cell::default());
                                    }
                                    self.row = self.row.min(self.rows - 1);
                                    self.col = saved.col.min(self.cols - 1);
                                    if saved.wrap_pending && self.cols > previous_cols {
                                        self.col = (self.col + 1).min(self.cols - 1);
                                    }
                                    self.wrap_pending = saved.wrap_pending
                                        && self.cols <= previous_cols
                                        && self.col == self.cols - 1;
                                    self.last_printed = None;
                                    for line in &mut self.grid {
                                        Self::normalize(line);
                                    }
                                }
                            }
                            self.top = 0;
                            self.bottom = self.rows - 1;
                        }
                        _ => {}
                    }
                }
            }
            'h' | 'l' if !private => {
                if params.contains(&4) {
                    self.insert_mode = final_char == 'h';
                }
            }
            'n' if params[0] == 6 => self.replies.extend_from_slice(
                format!(
                    "\x1b[{};{}R",
                    if self.origin_mode {
                        self.row.saturating_sub(self.top) + 1
                    } else {
                        self.row + 1
                    },
                    self.col + 1
                )
                .as_bytes(),
            ),
            'c' => self.replies.extend_from_slice(b"\x1b[?1;2c"),
            _ => {}
        }
        if matches!(final_char, 'J') {
            let blank = self.blank();
            for line in &mut self.grid {
                Self::normalize_with(line, &blank);
            }
        } else if matches!(final_char, 'K' | 'P' | '@' | 'X') {
            let blank = self.blank();
            Self::normalize_with(&mut self.grid[self.row], &blank);
        }
    }
    fn sgr(&mut self, p: &[usize]) {
        let mut i = 0;
        while i < p.len() {
            let n = p[i];
            match n {
                0 => self.style = Cell::default(),
                1 => self.style.bold = true,
                22 => self.style.bold = false,
                7 => self.style.inverse = true,
                27 => self.style.inverse = false,
                30..=37 => {
                    self.style.fg = PALETTE[n - 30];
                    self.style.fg_source = ColorSource::Indexed((n - 30) as u8);
                }
                40..=47 => {
                    self.style.bg = PALETTE[n - 40];
                    self.style.bg_source = ColorSource::Indexed((n - 40) as u8);
                }
                90..=97 => {
                    self.style.fg = PALETTE[n - 90 + 8];
                    self.style.fg_source = ColorSource::Indexed((n - 90 + 8) as u8);
                }
                100..=107 => {
                    self.style.bg = PALETTE[n - 100 + 8];
                    self.style.bg_source = ColorSource::Indexed((n - 100 + 8) as u8);
                }
                39 => {
                    self.style.fg = FOREGROUND;
                    self.style.fg_source = ColorSource::Default;
                }
                49 => {
                    self.style.bg = BACKGROUND;
                    self.style.bg_source = ColorSource::Default;
                }
                38 | 48 => {
                    let mut color = None;
                    let mut source = ColorSource::Rgb;
                    if p.get(i + 1) == Some(&5) && i + 2 < p.len() {
                        source = ColorSource::Indexed(p[i + 2].min(255) as u8);
                        color = Some(indexed(p[i + 2]));
                        i += 2;
                    } else if p.get(i + 1) == Some(&2) && i + 4 < p.len() {
                        color = Some(Ink(
                            p[i + 2].min(255) as u8,
                            p[i + 3].min(255) as u8,
                            p[i + 4].min(255) as u8,
                        ));
                        i += 4;
                    }
                    if let Some(c) = color {
                        if n == 38 {
                            self.style.fg = c;
                            self.style.fg_source = source;
                        } else {
                            self.style.bg = c;
                            self.style.bg_source = source;
                        }
                    }
                }
                _ => {}
            }
            i += 1;
        }
    }
}
fn indexed(n: usize) -> Ink {
    if n < 16 {
        PALETTE[n]
    } else if n < 232 {
        let n = n - 16;
        let cv = |x: usize| if x == 0 { 0 } else { (55 + x * 40) as u8 };
        Ink(cv(n / 36), cv(n / 6 % 6), cv(n % 6))
    } else {
        let v = (8 + (n.min(255) - 232) * 10) as u8;
        Ink(v, v, v)
    }
}
include!("terminal_tests.rs");
