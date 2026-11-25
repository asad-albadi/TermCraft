use super::{Outcome, Rng, TerminalInput, neighbours};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SudokuScene {
    pub cells: [u8; 81],
    pub fixed: [bool; 81],
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Sudoku {
    pub cells: [u8; 81],
    pub fixed: [bool; 81],
}
impl Sudoku {
    pub fn new(r: &mut Rng) -> Self {
        let mut digits = [1, 2, 3, 4, 5, 6, 7, 8, 9];
        r.shuffle(&mut digits);
        let mut bands = [0, 1, 2];
        let mut stacks = [0, 1, 2];
        r.shuffle(&mut bands);
        r.shuffle(&mut stacks);
        let mut rows = [0; 9];
        let mut cols = [0; 9];
        for b in 0..3 {
            let mut local = [0, 1, 2];
            r.shuffle(&mut local);
            for j in 0..3 {
                rows[b * 3 + j] = bands[b] * 3 + local[j]
            }
            r.shuffle(&mut local);
            for j in 0..3 {
                cols[b * 3 + j] = stacks[b] * 3 + local[j]
            }
        }
        let mut cells = [0; 81];
        for y in 0..9 {
            for x in 0..9 {
                cells[y * 9 + x] = digits[(rows[y] * 3 + rows[y] / 3 + cols[x]) % 9]
            }
        }
        let mut positions: Vec<usize> = (0..81).collect();
        r.shuffle(&mut positions);
        let mut removed = 0;
        for c in positions {
            if removed >= 42 {
                break;
            }
            let old = cells[c];
            cells[c] = 0;
            let mut board = cells;
            if count_solutions(&mut board, 2, &mut 50_000) == 1 {
                removed += 1
            } else {
                cells[c] = old
            }
        }
        let fixed = cells.map(|x| x != 0);
        Self { cells, fixed }
    }
    pub fn scene(&self) -> SudokuScene {
        SudokuScene {
            cells: self.cells,
            fixed: self.fixed,
        }
    }
    pub fn outcome(&self) -> Outcome {
        if self.cells.iter().all(|&x| x != 0) {
            Outcome::Won
        } else {
            Outcome::Playing
        }
    }
    pub fn apply(&mut self, i: TerminalInput) -> Result<(), &'static str> {
        let TerminalInput::Digit { cell, value } = i else {
            return Err("Expected digit input");
        };
        if cell >= 81 || value > 9 {
            return Err("Invalid cell or digit");
        }
        if self.fixed[cell] {
            return Err("Starting digits are fixed");
        }
        if value != 0 && !allowed(&self.cells, cell, value) {
            return Err("Digit conflicts with row, column or box");
        }
        self.cells[cell] = value;
        Ok(())
    }
}
fn allowed(cells: &[u8; 81], c: usize, v: u8) -> bool {
    let (x, y) = (c % 9, c / 9);
    (0..81).all(|j| {
        j == c
            || cells[j] != v
            || !(j / 9 == y || j % 9 == x || (j / 27 == y / 3 && j % 9 / 3 == x / 3))
    })
}
fn count_solutions(cells: &mut [u8; 81], limit: u8, budget: &mut usize) -> u8 {
    if *budget == 0 {
        return limit;
    }
    *budget -= 1;
    let mut best = None;
    let mut values = Vec::new();
    for c in 0..81 {
        if cells[c] != 0 {
            continue;
        }
        let choices: Vec<u8> = (1..=9).filter(|&v| allowed(cells, c, v)).collect();
        if choices.is_empty() {
            return 0;
        }
        if best.is_none() || choices.len() < values.len() {
            best = Some(c);
            values = choices;
        }
        if values.len() == 1 {
            break;
        }
    }
    let Some(c) = best else { return 1 };
    let mut count = 0;
    for v in values {
        cells[c] = v;
        count += count_solutions(cells, limit - count, budget);
        if count >= limit {
            break;
        }
    }
    cells[c] = 0;
    count
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinePreset {
    Newbie,
    Easy,
    Medium,
    Hard,
}
impl MinePreset {
    pub fn dimensions(self) -> (usize, usize, usize) {
        match self {
            Self::Newbie => (16, 16, 40),
            Self::Easy => (16, 24, 75),
            Self::Medium => (30, 16, 99),
            Self::Hard => (38, 20, 160),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MineCell {
    Hidden,
    Flagged,
    Open(u8),
    Mine,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NimesweeperScene {
    pub width: usize,
    pub height: usize,
    pub mines: usize,
    pub cells: Vec<MineCell>,
    pub paused: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Mines {
    pub w: usize,
    pub h: usize,
    pub mines: Vec<bool>,
    pub open: Vec<bool>,
    pub flags: Vec<bool>,
    pub outcome: Outcome,
    pub first: bool,
    pub paused: bool,
}
impl Mines {
    pub fn new(r: &mut Rng, p: MinePreset) -> Self {
        let (w, h, n) = p.dimensions();
        let mut positions: Vec<usize> = (0..w * h).collect();
        r.shuffle(&mut positions);
        let mut mines = vec![false; w * h];
        for &c in &positions[..n] {
            mines[c] = true
        }
        Self {
            w,
            h,
            mines,
            open: vec![false; w * h],
            flags: vec![false; w * h],
            outcome: Outcome::Playing,
            first: true,
            paused: false,
        }
    }
    pub fn scene(&self) -> NimesweeperScene {
        NimesweeperScene {
            width: self.w,
            height: self.h,
            mines: self.mines.iter().filter(|&&b| b).count(),
            paused: self.paused,
            cells: (0..self.w * self.h)
                .map(|c| {
                    if self.outcome == Outcome::Lost && self.mines[c] {
                        MineCell::Mine
                    } else if self.open[c] {
                        MineCell::Open(self.adjacent(c))
                    } else if self.flags[c] {
                        MineCell::Flagged
                    } else {
                        MineCell::Hidden
                    }
                })
                .collect(),
        }
    }
    fn adjacent(&self, c: usize) -> u8 {
        neighbours(c, self.w, self.h)
            .iter()
            .filter(|&&j| self.mines[j])
            .count() as u8
    }
    fn reveal(&mut self, c: usize) {
        if self.flags[c] || self.open[c] {
            return;
        }
        if self.mines[c] {
            self.open[c] = true;
            self.outcome = Outcome::Lost;
            return;
        }
        let mut stack = vec![c];
        while let Some(j) = stack.pop() {
            if self.open[j] || self.flags[j] || self.mines[j] {
                continue;
            }
            self.open[j] = true;
            if self.adjacent(j) == 0 {
                stack.extend(neighbours(j, self.w, self.h));
            }
        }
    }
    pub fn apply(&mut self, i: TerminalInput) -> Result<(), &'static str> {
        if i == TerminalInput::Pause {
            self.paused = !self.paused;
            return Ok(());
        }
        if self.paused {
            return Ok(());
        }
        match i {
            TerminalInput::Flag { cell } => {
                if cell >= self.mines.len() {
                    return Err("Invalid cell");
                }
                if !self.open[cell] {
                    self.flags[cell] = !self.flags[cell]
                }
            }
            TerminalInput::Reveal { cell } => {
                if cell >= self.mines.len() {
                    return Err("Invalid cell");
                }
                if self.flags[cell] || self.open[cell] {
                    return Ok(());
                }
                if self.first {
                    if self.mines[cell] {
                        let replacement = (0..self.mines.len())
                            .find(|&j| j != cell && !self.mines[j])
                            .ok_or("No safe square")?;
                        self.mines[cell] = false;
                        self.mines[replacement] = true;
                    }
                    self.first = false;
                }
                self.reveal(cell)
            }
            TerminalInput::Chord { cell } => {
                if cell >= self.mines.len() {
                    return Err("Invalid cell");
                }
                if self.open[cell] {
                    let ns = neighbours(cell, self.w, self.h);
                    if ns.iter().filter(|&&j| self.flags[j]).count() == self.adjacent(cell) as usize
                    {
                        for j in ns {
                            self.reveal(j);
                            if self.outcome == Outcome::Lost {
                                break;
                            }
                        }
                    }
                }
            }
            _ => return Err("Expected reveal, flag, chord or pause"),
        }
        if self.outcome != Outcome::Lost
            && (0..self.mines.len()).all(|j| self.mines[j] || self.open[j])
        {
            self.outcome = Outcome::Won
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TermwalkScene {
    pub width: usize,
    pub height: usize,
    pub router: usize,
    pub pipes: Vec<u8>,
    pub connected: Vec<bool>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Walk {
    pub size: usize,
    pub pipes: Vec<u8>,
}
const DIR: [(i32, i32, u8, u8); 4] = [(0, -1, 1, 4), (1, 0, 2, 8), (0, 1, 4, 1), (-1, 0, 8, 2)];
fn rotate(p: u8, cw: bool) -> u8 {
    if cw {
        ((p << 1) & 15) | (p >> 3)
    } else {
        (p >> 1) | ((p & 1) << 3)
    }
}
impl Walk {
    pub fn new(r: &mut Rng) -> Self {
        Self::new_size(r, 5)
    }
    pub fn new_size(r: &mut Rng, size: usize) -> Self {
        let mut pipes = vec![0; size * size];
        let mut visited = vec![false; size * size];
        visited[size * size / 2] = true;
        let mut stack = vec![size * size / 2];
        while let Some(&c) = stack.last() {
            let (x, y) = (c % size, c / size);
            let opts: Vec<_> = DIR
                .iter()
                .filter_map(|&(dx, dy, b, o)| {
                    let (a, d) = (x as i32 + dx, y as i32 + dy);
                    if (0..size as i32).contains(&a) && (0..size as i32).contains(&d) {
                        let n = d as usize * size + a as usize;
                        if !visited[n] {
                            return Some((n, b, o));
                        }
                    }
                    None
                })
                .collect();
            if opts.is_empty() {
                stack.pop();
                continue;
            }
            let (n, b, o) = opts[r.below(opts.len())];
            pipes[c] |= b;
            pipes[n] |= o;
            visited[n] = true;
            stack.push(n)
        }
        for p in &mut pipes {
            for _ in 0..r.below(4) {
                *p = rotate(*p, true)
            }
        }
        let mut s = Self { pipes, size };
        if s.outcome() == Outcome::Won {
            s.pipes[0] = rotate(s.pipes[0], true)
        }
        s
    }
    fn connected(&self) -> Vec<bool> {
        let size = self.size;
        let mut seen = vec![false; size * size];
        seen[size * size / 2] = true;
        let mut stack = vec![size * size / 2];
        while let Some(c) = stack.pop() {
            for &(dx, dy, b, o) in &DIR {
                let (x, y) = (c % size, c / size);
                let (a, d) = (x as i32 + dx, y as i32 + dy);
                if self.pipes[c] & b != 0
                    && (0..size as i32).contains(&a)
                    && (0..size as i32).contains(&d)
                {
                    let n = d as usize * size + a as usize;
                    if self.pipes[n] & o != 0 && !seen[n] {
                        seen[n] = true;
                        stack.push(n)
                    }
                }
            }
        }
        seen
    }
    pub fn outcome(&self) -> Outcome {
        if self.connected().iter().all(|&x| x) {
            Outcome::Won
        } else {
            Outcome::Playing
        }
    }
    pub fn scene(&self) -> TermwalkScene {
        TermwalkScene {
            width: self.size,
            height: self.size,
            router: self.size * self.size / 2,
            pipes: self.pipes.clone(),
            connected: self.connected(),
        }
    }
    pub fn apply(&mut self, i: TerminalInput) -> Result<(), &'static str> {
        let TerminalInput::Rotate { cell, clockwise } = i else {
            return Err("Expected rotation");
        };
        if cell >= self.pipes.len() {
            return Err("Invalid cell");
        }
        self.pipes[cell] = rotate(self.pipes[cell], clockwise);
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreedScene {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<u8>,
    pub player: usize,
    pub erased: usize,
    pub legal_directions: Vec<(i8, i8)>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Greed {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<u8>,
    pub player: usize,
    pub erased: usize,
}
impl Greed {
    pub fn new(r: &mut Rng) -> Self {
        let mut cells: Vec<u8> = (0..200).map(|_| (r.below(9) + 1) as u8).collect();
        let player = 105;
        cells[player] = 0;
        Self {
            w: 20,
            h: 10,
            cells,
            player,
            erased: 0,
        }
    }
    fn path(&self, dx: i8, dy: i8) -> Option<Vec<usize>> {
        if !(-1..=1).contains(&dx) || !(-1..=1).contains(&dy) || (dx == 0 && dy == 0) {
            return None;
        }
        let (x, y) = (self.player % self.w, self.player / self.w);
        let (a, b) = (x as i32 + dx as i32, y as i32 + dy as i32);
        if a < 0 || b < 0 || a >= self.w as i32 || b >= self.h as i32 {
            return None;
        }
        let n = self.cells[b as usize * self.w + a as usize];
        if n == 0 {
            return None;
        }
        let mut path = Vec::new();
        for i in 1..=n as i32 {
            let (a, b) = (x as i32 + dx as i32 * i, y as i32 + dy as i32 * i);
            if a < 0 || b < 0 || a >= self.w as i32 || b >= self.h as i32 {
                return None;
            }
            let c = b as usize * self.w + a as usize;
            if self.cells[c] == 0 {
                return None;
            }
            path.push(c)
        }
        Some(path)
    }
    fn legal(&self) -> Vec<(i8, i8)> {
        (-1..=1)
            .flat_map(|dy| (-1..=1).map(move |dx| (dx, dy)))
            .filter(|&(dx, dy)| self.path(dx, dy).is_some())
            .collect()
    }
    pub fn outcome(&self) -> Outcome {
        if self.cells.iter().all(|&x| x == 0) {
            Outcome::Won
        } else if self.legal().is_empty() {
            Outcome::Lost
        } else {
            Outcome::Playing
        }
    }
    pub fn scene(&self) -> GreedScene {
        GreedScene {
            width: self.w,
            height: self.h,
            cells: self.cells.clone(),
            player: self.player,
            erased: self.erased,
            legal_directions: self.legal(),
        }
    }
    pub fn apply(&mut self, i: TerminalInput) -> Result<(), &'static str> {
        let TerminalInput::Move { dx, dy } = i else {
            return Err("Expected movement");
        };
        let path = self
            .path(dx, dy)
            .ok_or("Direction crosses erased squares or leaves board")?;
        self.player = *path.last().unwrap();
        self.erased += path.len();
        for c in path {
            self.cells[c] = 0
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sudoku_rejects_row_column_box_conflicts_and_fixed_edits() {
        let mut s = Sudoku {
            cells: [0; 81],
            fixed: [false; 81],
        };
        s.cells[0] = 1;
        s.fixed[0] = true;
        assert!(s.apply(TerminalInput::Digit { cell: 0, value: 2 }).is_err());
        for c in [1, 9, 10] {
            assert!(s.apply(TerminalInput::Digit { cell: c, value: 1 }).is_err())
        }
        assert!(s.apply(TerminalInput::Digit { cell: 40, value: 1 }).is_ok());
        assert_eq!(s.cells[40], 1);
    }
    #[test]
    fn mine_flood_stops_at_number_flags_and_detonation() {
        let mut s = Mines {
            w: 3,
            h: 3,
            mines: vec![false, false, false, false, false, false, false, false, true],
            open: vec![false; 9],
            flags: vec![false; 9],
            outcome: Outcome::Playing,
            first: false,
            paused: false,
        };
        s.apply(TerminalInput::Flag { cell: 1 }).unwrap();
        s.apply(TerminalInput::Reveal { cell: 0 }).unwrap();
        assert!(!s.open[1]);
        assert!(s.open[4]);
        assert!(!s.open[8]);
        s.apply(TerminalInput::Reveal { cell: 8 }).unwrap();
        assert_eq!(s.outcome, Outcome::Lost);
    }
    #[test]
    fn walk_rotates_connection_ports() {
        let mut s = Walk {
            size: 5,
            pipes: vec![1; 25],
        };
        s.apply(TerminalInput::Rotate {
            cell: 0,
            clockwise: true,
        })
        .unwrap();
        assert_eq!(s.pipes[0], 2);
        s.apply(TerminalInput::Rotate {
            cell: 0,
            clockwise: false,
        })
        .unwrap();
        assert_eq!(s.pipes[0], 1);
    }
    #[test]
    fn greed_uses_first_digit_erases_entire_path_and_blocks_gaps() {
        let mut s = Greed {
            w: 5,
            h: 1,
            cells: vec![0, 3, 1, 9, 1],
            player: 0,
            erased: 0,
        };
        s.apply(TerminalInput::Move { dx: 1, dy: 0 }).unwrap();
        assert_eq!(s.player, 3);
        assert_eq!(s.cells, vec![0, 0, 0, 0, 1]);
        assert_eq!(s.erased, 3);
        assert!(s.apply(TerminalInput::Move { dx: -1, dy: 0 }).is_err());
        s.apply(TerminalInput::Move { dx: 1, dy: 0 }).unwrap();
        assert_eq!(s.outcome(), Outcome::Won);
    }
}

#[cfg(test)]
mod generation_tests {
    use super::*;
    #[test]
    fn sudoku_boards_have_unique_valid_solution_for_many_seeds() {
        for seed in 0..64 {
            let s = Sudoku::new(&mut Rng(seed));
            assert!(s.cells.iter().filter(|&&v| v != 0).count() >= 39);
            for c in 0..81 {
                if s.cells[c] != 0 {
                    assert!(allowed(&s.cells, c, s.cells[c]));
                    assert!(s.fixed[c])
                }
            }
            let mut cells = s.cells;
            assert_eq!(count_solutions(&mut cells, 2, &mut 1_000_000), 1);
        }
    }
    #[test]
    fn sudoku_completes_only_after_last_legal_digit() {
        let mut s = Sudoku::new(&mut Rng(42));
        let mut solution = s.cells;
        assert_eq!(count_solutions(&mut solution, 2, &mut 1_000_000), 1);
        fn solve(cells: &mut [u8; 81]) -> bool {
            if let Some(c) = cells.iter().position(|&v| v == 0) {
                for v in 1..=9 {
                    if allowed(cells, c, v) {
                        cells[c] = v;
                        if solve(cells) {
                            return true;
                        }
                    }
                }
                cells[c] = 0;
                false
            } else {
                true
            }
        }
        assert!(solve(&mut solution));
        for (c, &v) in solution.iter().enumerate() {
            if !s.fixed[c] {
                s.apply(TerminalInput::Digit { cell: c, value: v }).unwrap();
            }
        }
        assert_eq!(s.outcome(), Outcome::Won);
    }
    #[test]
    fn first_mine_reveal_preserves_mine_count_and_safe_square() {
        for preset in [
            MinePreset::Newbie,
            MinePreset::Easy,
            MinePreset::Medium,
            MinePreset::Hard,
        ] {
            let mut s = Mines::new(&mut Rng(9), preset);
            let (w, h, n) = preset.dimensions();
            assert_eq!(s.mines.len(), w * h);
            assert_eq!(s.mines.iter().filter(|&&x| x).count(), n);
            let c = s.mines.iter().position(|&x| x).unwrap();
            s.apply(TerminalInput::Reveal { cell: c }).unwrap();
            assert!(s.open[c]);
            assert!(!s.mines[c]);
            assert_eq!(s.mines.iter().filter(|&&x| x).count(), n);
            assert_ne!(s.outcome, Outcome::Lost);
        }
    }
    #[test]
    fn mine_chord_with_wrong_flag_detonates_and_correct_flag_wins() {
        fn board() -> Mines {
            Mines {
                w: 2,
                h: 2,
                mines: vec![true, false, false, false],
                open: vec![false, false, false, true],
                flags: vec![false; 4],
                outcome: Outcome::Playing,
                first: false,
                paused: false,
            }
        }
        let mut s = board();
        s.flags[0] = true;
        s.apply(TerminalInput::Chord { cell: 3 }).unwrap();
        assert_eq!(s.outcome, Outcome::Won);
        let mut s = board();
        s.flags[1] = true;
        s.apply(TerminalInput::Chord { cell: 3 }).unwrap();
        assert_eq!(s.outcome, Outcome::Lost);
    }
    #[test]
    fn walk_spanning_tree_has_bounded_ports_and_can_be_oriented() {
        for seed in 0..64 {
            let mut s = Walk::new(&mut Rng(seed));
            assert_eq!(s.pipes.len(), 25);
            assert_eq!(s.pipes.iter().map(|p| p.count_ones()).sum::<u32>(), 48);
            assert!(s.pipes.iter().all(|&p| p > 0 && p < 16));
            assert_eq!(s.outcome(), Outcome::Playing);
            for c in 0..25 {
                let p = s.pipes[c];
                for _ in 0..4 {
                    s.apply(TerminalInput::Rotate {
                        cell: c,
                        clockwise: true,
                    })
                    .unwrap();
                }
                assert_eq!(s.pipes[c], p)
            }
        }
    }
    #[test]
    fn greed_cannot_jump_over_erased_cell_or_leave_grid() {
        let mut s = Greed {
            w: 4,
            h: 1,
            cells: vec![0, 3, 0, 1],
            player: 0,
            erased: 0,
        };
        let before = s.clone();
        assert!(s.apply(TerminalInput::Move { dx: 1, dy: 0 }).is_err());
        assert_eq!(s, before);
        assert_eq!(s.outcome(), Outcome::Lost);
        for (dx, dy) in [(127, 0), (-128, 1), (0, 0), (0, -1)] {
            assert!(s.apply(TerminalInput::Move { dx, dy }).is_err())
        }
    }
}
#[cfg(test)]
mod walk_win_test {
    use super::*;
    #[test]
    fn all_twenty_five_tiles_must_connect_to_central_router() {
        let mut path = Vec::new();
        for y in 0..5 {
            for i in 0..5 {
                path.push(y * 5 + if y % 2 == 0 { i } else { 4 - i });
            }
        }
        let mut s = Walk {
            size: 5,
            pipes: vec![0; 25],
        };
        for pair in path.windows(2) {
            let (a, b) = (pair[0], pair[1]);
            let (da, db) = if b == a + 1 {
                (2, 8)
            } else if a == b + 1 {
                (8, 2)
            } else {
                (4, 1)
            };
            s.pipes[a] |= da;
            s.pipes[b] |= db;
        }
        assert_eq!(s.outcome(), Outcome::Won);
        s.apply(TerminalInput::Rotate {
            cell: 0,
            clockwise: true,
        })
        .unwrap();
        assert_eq!(s.outcome(), Outcome::Playing);
        assert!(!s.scene().connected[0]);
    }
}

#[cfg(test)]
mod walk_size_test {
    use super::*;
    #[test]
    fn odd_sizes_have_central_router_and_bounded_trees() {
        for size in [3, 7, 9, 15] {
            let s = Walk::new_size(&mut Rng(0), size);
            let g = s.scene();
            assert_eq!(g.width, size);
            assert_eq!(g.height, size);
            assert_eq!(g.router, size * size / 2);
            assert_eq!(g.pipes.len(), size * size);
            assert_eq!(
                g.pipes.iter().map(|p| p.count_ones()).sum::<u32>(),
                (size * size * 2 - 2) as u32
            );
        }
    }
}
