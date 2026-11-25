//! Independent, deterministic native implementations of six recovered puzzle rules.
//! See PROVENANCE.md. No original binary, C engine, assets, I/O or unsafe code.
mod config;
mod guess;
mod map;
mod mines;
mod net;
mod solo;
#[cfg(test)]
mod tests;
mod untangle;

pub use config::{PuzzleConfig, PuzzlePreset};
pub use guess::{GuessConfig, GuessRow, GuessScene};
pub use map::{MapCertificate, MapConfig, MapDifficulty, MapGeneration, MapPreset, MapScene};
pub use mines::{MineCell, MinesConfig, MinesGeneration, MinesScene};
pub use net::{NetConfig, NetScene};
pub use solo::{SoloCertificate, SoloConfig, SoloDifficulty, SoloScene, SoloSymmetry};
pub use untangle::UntangleScene;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PuzzleKind {
    Net,
    Mines,
    Solo,
    Guess,
    Map,
    Untangle,
}
impl PuzzleKind {
    pub const ALL: [Self; 6] = [
        Self::Net,
        Self::Mines,
        Self::Solo,
        Self::Guess,
        Self::Map,
        Self::Untangle,
    ];
    pub fn selector(self) -> &'static str {
        match self {
            Self::Net => "net",
            Self::Mines => "mines",
            Self::Solo => "solo",
            Self::Guess => "guess",
            Self::Map => "map",
            Self::Untangle => "untangle",
        }
    }
    pub fn help(self) -> &'static str {
        match self {
            Self::Net => {
                "Rotate every tile to connect all tiles without loops. Left/A: anticlockwise; right/D: clockwise; middle/S: lock; F: half-turn; Ctrl+arrows: highlight source; J: jumble unlocked tiles."
            }
            Self::Mines => {
                "Reveal every safe square. Right/Space: flag. Reveal an open number to clear unflagged neighbours when flags match. First reveal is safe."
            }
            Self::Solo => {
                "Fill each row, column and 3×3 box with digits 1–9 once. 0 erases. Starting digits cannot be changed."
            }
            Self::Guess => {
                "Find four hidden pegs using six colours within ten guesses. Black = correct position; white = correct colour elsewhere. Repeats allowed."
            }
            Self::Map => {
                "Colour every region with one of four colours. Regions sharing an edge must differ; corner contact does not count. Starting colours are fixed."
            }
            Self::Untangle => {
                "Drag the points until no edges cross. Edges sharing a graph point are ignored; unrelated edges must not cross, touch or overlap."
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
pub enum Direction {
    North,
    East,
    South,
    West,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PuzzleInput {
    HalfTurn {
        cell: usize,
    },
    NetSource {
        direction: Direction,
    },
    /// Same explicit seed and board produce the same jumble; one atomic undo.
    Jumble {
        seed: u64,
    },
    Rotate {
        cell: usize,
        clockwise: bool,
    },
    Lock {
        cell: usize,
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
    Digit {
        cell: usize,
        value: u8,
    },
    Pencil {
        cell: usize,
        value: u8,
    },
    Peg {
        slot: usize,
        colour: u8,
    },
    Hold {
        slot: usize,
    },
    Submit,
    Colour {
        region: usize,
        colour: u8,
    },
    MapPencil {
        region: usize,
        colour: u8,
    },
    /// Integer normalized coordinates, inclusive 0..=1000. No NaN or infinity.
    Move {
        vertex: usize,
        x: i32,
        y: i32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PuzzleScene {
    Net(NetScene),
    Mines(MinesScene),
    Solo(SoloScene),
    Guess(GuessScene),
    Map(MapScene),
    Untangle(UntangleScene),
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum State {
    Net(net::Net),
    Mines(mines::Mines),
    Solo(solo::Solo),
    Guess(guess::Guess),
    Map(map::Map),
    Untangle(untangle::Untangle),
}
#[derive(Clone, Debug)]
pub struct PuzzleSession {
    kind: PuzzleKind,
    seed: u64,
    state: State,
    initial: State,
    config: PuzzleConfig,
    history: Vec<(State, bool)>,
    future: Vec<(State, bool)>,
    assisted: bool,
    deaths: u32,
    net_source: usize,
}
impl PuzzleSession {
    pub const UNDO_LIMIT: usize = 256;
    pub fn new(kind: PuzzleKind, seed: u64) -> Self {
        let mut rng = Rng(seed);
        let state = match kind {
            PuzzleKind::Net => State::Net(net::Net::new(&mut rng)),
            PuzzleKind::Mines => State::Mines(mines::Mines::new(seed)),
            PuzzleKind::Solo => State::Solo(solo::Solo::new(&mut rng)),
            PuzzleKind::Guess => State::Guess(guess::Guess::new(&mut rng)),
            PuzzleKind::Map => State::Map(map::Map::new(&mut rng)),
            PuzzleKind::Untangle => State::Untangle(untangle::Untangle::new(&mut rng)),
        };
        Self::from_state(kind, seed, kind.default_configuration(), state)
    }
    fn from_state(kind: PuzzleKind, seed: u64, config: PuzzleConfig, state: State) -> Self {
        let net_source = match &state {
            State::Net(n) => {
                let (w, h) = n.dimensions();
                h / 2 * w + w / 2
            }
            _ => 0,
        };
        Self {
            kind,
            seed,
            initial: state.clone(),
            config,
            state,
            history: Vec::new(),
            future: Vec::new(),
            assisted: false,
            deaths: 0,
            net_source,
        }
    }
    pub fn new_config(seed: u64, config: PuzzleConfig) -> Result<Self, &'static str> {
        config.validate()?;
        let mut rng = Rng(seed);
        let state = match config {
            PuzzleConfig::Net(c) => State::Net(net::Net::new_config(&mut rng, c)?),
            PuzzleConfig::Mines(c) => State::Mines(mines::Mines::new_config(seed, c)?),
            PuzzleConfig::Solo(c) => State::Solo(solo::Solo::new_config(&mut rng, c)?),
            PuzzleConfig::Guess(c) => State::Guess(guess::Guess::new_config(&mut rng, c)?),
            PuzzleConfig::Map(c) => State::Map(map::Map::new_config(&mut rng, c)?),
            PuzzleConfig::Untangle { points } => {
                State::Untangle(untangle::Untangle::new_config(&mut rng, points)?)
            }
        };
        Ok(Self::from_state(config.kind(), seed, config, state))
    }
    pub fn configuration(&self) -> PuzzleConfig {
        self.config
    }
    pub fn help(&self) -> String {
        self.config.help()
    }
    pub fn new_map(seed: u64, preset: MapPreset) -> Self {
        Self::new_config(
            seed,
            PuzzleConfig::Map(MapConfig {
                preset,
                ..MapConfig::default()
            }),
        )
        .expect("supported legacy Map preset")
    }
    pub fn kind(&self) -> PuzzleKind {
        self.kind
    }
    pub fn seed(&self) -> u64 {
        self.seed
    }
    pub fn outcome(&self) -> Outcome {
        match &self.state {
            State::Net(s) => s.outcome(),
            State::Mines(s) => s.outcome(),
            State::Solo(s) => s.outcome(),
            State::Guess(s) => s.outcome(),
            State::Map(s) => s.outcome(),
            State::Untangle(s) => s.outcome(),
        }
    }
    /// Invalid/no-op moves never consume an undo slot. Terminal boards require undo/reset.
    pub fn apply(&mut self, input: PuzzleInput) -> Result<bool, &'static str> {
        if let PuzzleInput::NetSource { direction } = input {
            if self.kind != PuzzleKind::Net {
                return Err("Highlight source is only available in Net");
            }
            let State::Net(n) = &self.state else {
                unreachable!()
            };
            let (w, h) = n.dimensions();
            let (x, y) = (self.net_source % w, self.net_source / w);
            self.net_source = match direction {
                Direction::North => (y + h - 1) % h * w + x,
                Direction::South => (y + 1) % h * w + x,
                Direction::West => y * w + (x + w - 1) % w,
                Direction::East => y * w + (x + 1) % w,
            };
            return Ok(true);
        }
        if self.outcome() != Outcome::Playing {
            return Err("Game finished; undo or reset to continue");
        }
        let before = self.state.clone();
        let result = match &mut self.state {
            State::Net(s) => s.apply(input),
            State::Mines(s) => s.apply(input),
            State::Solo(s) => s.apply(input),
            State::Guess(s) => s.apply(input),
            State::Map(s) => s.apply(input),
            State::Untangle(s) => s.apply(input),
        };
        if let Err(e) = result {
            self.state = before;
            return Err(e);
        }
        if self.state == before {
            return Ok(false);
        }
        if self.outcome() == Outcome::Lost && self.kind == PuzzleKind::Mines {
            self.deaths = self.deaths.saturating_add(1);
        }
        if self.history.len() == Self::UNDO_LIMIT {
            self.history.remove(0);
        }
        self.history.push((before, self.assisted));
        self.future.clear();
        Ok(true)
    }
    pub fn input(&mut self, input: PuzzleInput) -> Result<bool, &'static str> {
        self.apply(input)
    }
    pub fn undo(&mut self) -> bool {
        if let Some((s, assisted)) = self.history.pop() {
            self.future
                .push((std::mem::replace(&mut self.state, s), self.assisted));
            self.assisted = assisted;
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self) -> bool {
        if let Some((s, assisted)) = self.future.pop() {
            self.history
                .push((std::mem::replace(&mut self.state, s), self.assisted));
            self.assisted = assisted;
            true
        } else {
            false
        }
    }
    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
    pub fn assisted(&self) -> bool {
        self.assisted
    }
    /// Explicit answer reveal, in one undoable step. Guess's source action gives
    /// up, and Mines requires an existing layout. No generation/solution IDs.
    pub fn solve(&mut self) -> Result<bool, &'static str> {
        if self.outcome() == Outcome::Won
            || (self.outcome() == Outcome::Lost && self.kind != PuzzleKind::Mines)
        {
            return Ok(false);
        }
        let before = self.state.clone();
        match &mut self.state {
            State::Net(s) => s.solve(),
            State::Solo(s) => s.solve(),
            State::Map(s) => s.solve(),
            State::Untangle(s) => s.solve(),
            State::Guess(s) => s.solve(),
            State::Mines(s) => s.solve()?,
        }
        if self.state == before {
            return Ok(false);
        }
        if self.history.len() == Self::UNDO_LIMIT {
            self.history.remove(0);
        }
        self.history.push((before, self.assisted));
        self.future.clear();
        self.assisted = true;
        Ok(true)
    }
    pub fn can_undo(&self) -> bool {
        !self.history.is_empty()
    }
    pub fn reset(&mut self) {
        self.state = self.initial.clone();
        self.history.clear();
        self.future.clear();
        self.assisted = false;
        self.deaths = 0;
        self.net_source = match &self.state {
            State::Net(n) => {
                let (w, h) = n.dimensions();
                h / 2 * w + w / 2
            }
            _ => 0,
        };
    }
    pub fn scene(&self) -> PuzzleScene {
        match &self.state {
            State::Net(s) => PuzzleScene::Net(s.scene_at(self.net_source)),
            State::Mines(s) => PuzzleScene::Mines(s.scene(self.deaths)),
            State::Solo(s) => PuzzleScene::Solo(s.scene()),
            State::Guess(s) => PuzzleScene::Guess(s.scene()),
            State::Map(s) => PuzzleScene::Map(s.scene()),
            State::Untangle(s) => PuzzleScene::Untangle(s.scene()),
        }
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
            v.swap(i, j);
        }
    }
}
fn neighbours(cell: usize, w: usize, h: usize, diagonal: bool) -> Vec<usize> {
    let (x, y) = (cell % w, cell / w);
    let mut out = Vec::new();
    for dy in -1i32..=1 {
        for dx in -1i32..=1 {
            if (dx == 0 && dy == 0) || (!diagonal && dx != 0 && dy != 0) {
                continue;
            }
            let (nx, ny) = (x as i32 + dx, y as i32 + dy);
            if nx >= 0 && ny >= 0 && nx < w as i32 && ny < h as i32 {
                out.push(ny as usize * w + nx as usize);
            }
        }
    }
    out
}
