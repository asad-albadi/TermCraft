//! Independent pure Rust terminal games. See PROVENANCE.md for evidence boundaries.
mod blocks;
mod cards;
mod grids;
mod invaders;
#[cfg(test)]
mod tests;
pub use blocks::{CtrisScene, Piece};
pub use cards::{Card, CardPile, SolitaireVariant, SolitermScene};
pub use grids::{GreedScene, MineCell, MinePreset, NimesweeperScene, SudokuScene, TermwalkScene};
pub use invaders::{Invader, NinvadersScene};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalKind {
    Soliterm,
    Termwalk,
    Ctris,
    Sudoku,
    Nimesweeper,
    Ninvaders,
    Greed,
}
impl TerminalKind {
    pub const ALL: [Self; 7] = [
        Self::Soliterm,
        Self::Termwalk,
        Self::Ctris,
        Self::Sudoku,
        Self::Nimesweeper,
        Self::Ninvaders,
        Self::Greed,
    ];
    pub fn selector(self) -> &'static str {
        match self {
            Self::Soliterm => "soliterm",
            Self::Termwalk => "termwalk",
            Self::Ctris => "ctris",
            Self::Sudoku => "sudoku",
            Self::Nimesweeper => "nimesweeper",
            Self::Ninvaders => "ninvaders",
            Self::Greed => "greed",
        }
    }
    pub fn help(self) -> &'static str {
        match self {
            Self::Soliterm => {
                "Select a source pile and destination. Build alternating colours descending in tableau; suit foundations ascend Ace to King. Freecell has four reserves. Klondike uses draw-one stock and King-only empty columns. Undo/reset available."
            }
            Self::Termwalk => {
                "Rotate tiles clockwise or anticlockwise to connect every server to the central router. No wrapping; all pipe ends must match. Undo/reset available."
            }
            Self::Ctris => {
                "Left/right move, up rotates clockwise, down soft-drops, Space hard-drops. P pauses. Complete horizontal rows to clear them; blocked spawn loses. Independent conventional controls."
            }
            Self::Sudoku => {
                "Digits 1–9 fill each row, column and 3×3 box once. 0 erases. Starting digits are fixed. Undo/reset available."
            }
            Self::Nimesweeper => {
                "Reveal every safe square. F toggles flag; Space reveals; open numbered squares chord when adjacent flags match. P pauses. Undo/reset available."
            }
            Self::Ninvaders => {
                "Space starts/fires. Left/right move. Tick advances one simulation step. P pauses. Destroy the wave; enemy shots cost lives, enemies reaching the player lose. Independent conventional controls."
            }
            Self::Greed => {
                "Move in eight directions (hjklyubn/keypad). The first digit sets distance. Erase that many squares; crossing erased squares or leaving the board is illegal. No legal moves ends the game; clear all digits to win."
            }
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Playing,
    Won,
    Lost,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalInput {
    Rotate {
        cell: usize,
        clockwise: bool,
    },
    Digit {
        cell: usize,
        value: u8,
    },
    Reveal {
        cell: usize,
    },
    Flag {
        cell: usize,
    },
    Chord {
        cell: usize,
    },
    Move {
        dx: i8,
        dy: i8,
    },
    Tick,
    Drop,
    Fire,
    Start,
    Pause,
    Draw,
    Transfer {
        from: CardPile,
        to: CardPile,
        count: usize,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminalScene {
    Soliterm(SolitermScene),
    Termwalk(TermwalkScene),
    Ctris(CtrisScene),
    Sudoku(SudokuScene),
    Nimesweeper(NimesweeperScene),
    Ninvaders(NinvadersScene),
    Greed(GreedScene),
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum State {
    Cards(cards::Cards),
    Walk(grids::Walk),
    Blocks(blocks::Blocks),
    Sudoku(grids::Sudoku),
    Mines(grids::Mines),
    Invaders(invaders::Invaders),
    Greed(grids::Greed),
}
#[derive(Clone, Debug)]
pub struct TerminalSession {
    kind: TerminalKind,
    seed: u64,
    state: State,
    initial: State,
    history: Vec<State>,
}
impl TerminalSession {
    pub const UNDO_LIMIT: usize = 128;
    pub fn new(kind: TerminalKind, seed: u64) -> Self {
        let mut rng = Rng(seed);
        let state = match kind {
            TerminalKind::Soliterm => {
                State::Cards(cards::Cards::new(&mut rng, SolitaireVariant::Freecell, 3))
            }
            TerminalKind::Termwalk => State::Walk(grids::Walk::new(&mut rng)),
            TerminalKind::Ctris => State::Blocks(blocks::Blocks::new(&mut rng)),
            TerminalKind::Sudoku => State::Sudoku(grids::Sudoku::new(&mut rng)),
            TerminalKind::Nimesweeper => {
                State::Mines(grids::Mines::new(&mut rng, MinePreset::Easy))
            }
            TerminalKind::Ninvaders => State::Invaders(invaders::Invaders::new()),
            TerminalKind::Greed => State::Greed(grids::Greed::new(&mut rng)),
        };
        Self {
            kind,
            seed,
            initial: state.clone(),
            state,
            history: Vec::new(),
        }
    }
    pub fn new_solitaire(seed: u64, variant: SolitaireVariant, passes: u8) -> Self {
        let mut s = Self::new(TerminalKind::Soliterm, seed);
        s.state = State::Cards(cards::Cards::new(
            &mut Rng(seed),
            variant,
            passes.clamp(1, 20),
        ));
        s.initial = s.state.clone();
        s
    }
    /// Only odd sizes 3 through 15 are accepted; generation is bounded.
    pub fn new_termwalk(seed: u64, size: usize) -> Result<Self, &'static str> {
        if !(3..=15).contains(&size) || size % 2 == 0 {
            return Err("Grid size must be odd, between 3 and 15");
        }
        let mut s = Self::new(TerminalKind::Termwalk, seed);
        s.state = State::Walk(grids::Walk::new_size(&mut Rng(seed), size));
        s.initial = s.state.clone();
        Ok(s)
    }
    pub fn new_mines(seed: u64, preset: MinePreset) -> Self {
        let mut s = Self::new(TerminalKind::Nimesweeper, seed);
        s.state = State::Mines(grids::Mines::new(&mut Rng(seed), preset));
        s.initial = s.state.clone();
        s
    }
    pub fn kind(&self) -> TerminalKind {
        self.kind
    }
    pub fn seed(&self) -> u64 {
        self.seed
    }
    pub fn outcome(&self) -> Outcome {
        match &self.state {
            State::Cards(s) => s.outcome(),
            State::Walk(s) => s.outcome(),
            State::Blocks(s) => s.outcome,
            State::Sudoku(s) => s.outcome(),
            State::Mines(s) => s.outcome,
            State::Invaders(s) => s.outcome,
            State::Greed(s) => s.outcome(),
        }
    }
    pub fn scene(&self) -> TerminalScene {
        match &self.state {
            State::Cards(s) => TerminalScene::Soliterm(s.scene()),
            State::Walk(s) => TerminalScene::Termwalk(s.scene()),
            State::Blocks(s) => TerminalScene::Ctris(s.scene()),
            State::Sudoku(s) => TerminalScene::Sudoku(s.scene()),
            State::Mines(s) => TerminalScene::Nimesweeper(s.scene()),
            State::Invaders(s) => TerminalScene::Ninvaders(s.scene()),
            State::Greed(s) => TerminalScene::Greed(s.scene()),
        }
    }
    pub fn apply(&mut self, input: TerminalInput) -> Result<bool, &'static str> {
        if self.outcome() != Outcome::Playing {
            return Err("Game finished; undo or reset to continue");
        }
        let before = self.state.clone();
        let result = match &mut self.state {
            State::Cards(s) => s.apply(input),
            State::Walk(s) => s.apply(input),
            State::Blocks(s) => s.apply(input),
            State::Sudoku(s) => s.apply(input),
            State::Mines(s) => s.apply(input),
            State::Invaders(s) => s.apply(input),
            State::Greed(s) => s.apply(input),
        };
        if let Err(e) = result {
            self.state = before;
            return Err(e);
        }
        if before == self.state {
            return Ok(false);
        }
        if self.history.len() == Self::UNDO_LIMIT {
            self.history.remove(0);
        }
        self.history.push(before);
        Ok(true)
    }
    pub fn input(&mut self, input: TerminalInput) -> Result<bool, &'static str> {
        self.apply(input)
    }
    pub fn undo(&mut self) -> bool {
        if let Some(s) = self.history.pop() {
            self.state = s;
            true
        } else {
            false
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }
    pub fn reset(&mut self) {
        self.state = self.initial.clone();
        self.history.clear();
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j)
        }
    }
}
fn neighbours(c: usize, w: usize, h: usize) -> Vec<usize> {
    let (x, y) = (c % w, c / w);
    let mut v = Vec::new();
    for dy in -1..=1i32 {
        for dx in -1..=1i32 {
            let (a, b) = (x as i32 + dx, y as i32 + dy);
            if (dx != 0 || dy != 0) && a >= 0 && b >= 0 && a < w as i32 && b < h as i32 {
                v.push(b as usize * w + a as usize)
            }
        }
    }
    v
}
