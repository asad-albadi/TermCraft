use super::{Outcome, Rng, TerminalInput};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub kind: u8,
    pub rotation: u8,
    pub x: i32,
    pub y: i32,
}
impl Piece {
    /// Total for arbitrary public coordinates: offsets saturate at i32 bounds.
    /// Saturation may merge cells outside the valid board; session cells stay exact.
    pub fn cells(self) -> [(i32, i32); 4] {
        const SHAPES: [[(i32, i32); 4]; 7] = [
            [(0, 1), (1, 1), (2, 1), (3, 1)],
            [(1, 0), (2, 0), (1, 1), (2, 1)],
            [(1, 0), (0, 1), (1, 1), (2, 1)],
            [(1, 0), (2, 0), (0, 1), (1, 1)],
            [(0, 0), (1, 0), (1, 1), (2, 1)],
            [(0, 0), (0, 1), (1, 1), (2, 1)],
            [(2, 0), (0, 1), (1, 1), (2, 1)],
        ];
        let mut out = SHAPES[self.kind.min(6) as usize];
        for p in &mut out {
            if self.kind != 1 {
                let size = if self.kind == 0 { 4 } else { 3 };
                for _ in 0..self.rotation % 4 {
                    *p = (size - 1 - p.1, p.0)
                }
            }
            p.0 = self.x.saturating_add(p.0);
            p.1 = self.y.saturating_add(p.1)
        }
        out
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CtrisScene {
    pub width: usize,
    pub height: usize,
    pub settled: Vec<u8>,
    pub active: Piece,
    pub next: u8,
    pub score: u32,
    pub lines: u32,
    pub paused: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Blocks {
    pub board: Vec<u8>,
    pub active: Piece,
    pub next: u8,
    pub rng: Rng,
    pub score: u32,
    pub lines: u32,
    pub paused: bool,
    pub outcome: Outcome,
}
impl Blocks {
    pub fn new(r: &mut Rng) -> Self {
        let kind = r.below(7) as u8;
        let next = r.below(7) as u8;
        Self {
            board: vec![0; 200],
            active: Piece {
                kind,
                rotation: 0,
                x: 3,
                y: 0,
            },
            next,
            rng: r.clone(),
            score: 0,
            lines: 0,
            paused: false,
            outcome: Outcome::Playing,
        }
    }
    pub fn scene(&self) -> CtrisScene {
        CtrisScene {
            width: 10,
            height: 20,
            settled: self.board.clone(),
            active: self.active,
            next: self.next,
            score: self.score,
            lines: self.lines,
            paused: self.paused,
        }
    }
    fn fits(&self, p: Piece) -> bool {
        p.cells().iter().all(|&(x, y)| {
            x >= 0 && x < 10 && y >= 0 && y < 20 && self.board[y as usize * 10 + x as usize] == 0
        })
    }
    fn lock(&mut self) {
        for (x, y) in self.active.cells() {
            self.board[y as usize * 10 + x as usize] = self.active.kind + 1
        }
        let mut rows: Vec<Vec<u8>> = self
            .board
            .chunks(10)
            .filter(|row| row.iter().any(|&x| x == 0))
            .map(|r| r.to_vec())
            .collect();
        let cleared = 20 - rows.len();
        let mut board = vec![0; cleared * 10];
        for row in rows.drain(..) {
            board.extend(row)
        }
        self.board = board;
        self.lines = self.lines.saturating_add(cleared as u32);
        self.score = self
            .score
            .saturating_add([0, 100, 300, 500, 800][cleared.min(4)]);
        self.active = Piece {
            kind: self.next,
            rotation: 0,
            x: 3,
            y: 0,
        };
        self.next = self.rng.below(7) as u8;
        if !self.fits(self.active) {
            self.outcome = Outcome::Lost
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
            TerminalInput::Rotate { clockwise, .. } => {
                let mut p = self.active;
                p.rotation = (p.rotation + if clockwise { 1 } else { 3 }) % 4;
                if self.fits(p) {
                    self.active = p
                }
            }
            TerminalInput::Move { dx, dy } => {
                if !(-1..=1).contains(&dx) || !(0..=1).contains(&dy) || (dx != 0 && dy != 0) {
                    return Err("Invalid block direction");
                }
                let mut p = self.active;
                p.x += dx as i32;
                p.y += dy as i32;
                if self.fits(p) {
                    self.active = p
                } else if dy == 1 {
                    self.lock()
                }
            }
            TerminalInput::Tick => {
                let mut p = self.active;
                p.y += 1;
                if self.fits(p) {
                    self.active = p
                } else {
                    self.lock()
                }
            }
            TerminalInput::Drop => {
                loop {
                    let mut p = self.active;
                    p.y += 1;
                    if self.fits(p) {
                        self.active = p
                    } else {
                        break;
                    }
                }
                self.lock()
            }
            _ => return Err("Expected block movement, rotation, drop, tick or pause"),
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hard_drop_completes_row_and_preserves_upper_cells() {
        let mut s = Blocks::new(&mut Rng(0));
        for x in 0..6 {
            s.board[190 + x] = 2
        }
        s.active = Piece {
            kind: 0,
            rotation: 0,
            x: 6,
            y: 0,
        };
        s.apply(TerminalInput::Drop).unwrap();
        assert_eq!(s.lines, 1);
        assert!(s.board[190..].iter().all(|&x| x == 0));
        assert_eq!(s.score, 100);
    }
    #[test]
    fn wall_collision_and_pause_prevent_motion() {
        let mut s = Blocks::new(&mut Rng(0));
        s.active = Piece {
            kind: 1,
            rotation: 0,
            x: -1,
            y: 2,
        };
        let before = s.active;
        assert!(s.apply(TerminalInput::Move { dx: -1, dy: 0 }).is_ok());
        assert_eq!(s.active, before);
        s.apply(TerminalInput::Pause).unwrap();
        s.apply(TerminalInput::Tick).unwrap();
        assert_eq!(s.active, before);
    }
}
#[cfg(test)]
mod loss_tests {
    use super::*;
    #[test]
    fn blocked_next_spawn_loses_after_piece_locks() {
        let mut s = Blocks::new(&mut Rng(1));
        s.board[4] = 2;
        s.next = 1;
        s.active = Piece {
            kind: 1,
            rotation: 0,
            x: 0,
            y: 17,
        };
        s.apply(TerminalInput::Drop).unwrap();
        assert_eq!(s.outcome, Outcome::Lost);
    }
    #[test]
    fn rotation_cycles_and_preserves_four_distinct_cells() {
        for kind in 0..7 {
            let mut s = Blocks::new(&mut Rng(0));
            s.active = Piece {
                kind,
                rotation: 0,
                x: 3,
                y: 5,
            };
            let original = s.active.cells();
            for _ in 0..4 {
                s.apply(TerminalInput::Rotate {
                    cell: 0,
                    clockwise: true,
                })
                .unwrap();
            }
            assert_eq!(s.active.cells(), original);
            let mut cells = s.active.cells().to_vec();
            cells.sort();
            cells.dedup();
            assert_eq!(cells.len(), 4);
        }
    }
}
#[cfg(test)]
mod public_piece_safety {
    use super::*;
    #[test]
    fn arbitrary_public_piece_coordinates_saturate_instead_of_overflowing() {
        for kind in 0..=255 {
            for rotation in [0, 1, 2, 3, 255] {
                let p = Piece {
                    kind,
                    rotation,
                    x: i32::MAX,
                    y: i32::MAX,
                };
                assert!(
                    p.cells()
                        .iter()
                        .all(|&(x, y)| x == i32::MAX && y == i32::MAX)
                );
                let p = Piece {
                    kind,
                    rotation,
                    x: i32::MIN,
                    y: i32::MIN,
                };
                assert!(p.cells().iter().all(|&(x, y)| x >= i32::MIN
                    && x <= i32::MIN + 3
                    && y >= i32::MIN
                    && y <= i32::MIN + 3));
            }
        }
    }
}
