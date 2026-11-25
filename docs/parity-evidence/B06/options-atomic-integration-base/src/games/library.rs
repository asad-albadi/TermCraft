//! Typed common catalog over the independently implemented pure cores.
use super::{puzzles as p, terminal as t};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameId {
    Soliterm,
    Termwalk,
    Ctris,
    Sudoku,
    Nimesweeper,
    Ninvaders,
    Greed,
    Netwalk,
    Mines,
    Solo,
    Guess,
    Map,
    Untangle,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GameSpec {
    pub id: GameId,
    pub label: &'static str,
    pub selector: &'static str,
}
pub struct GameRegistry;
impl GameRegistry {
    pub fn catalog() -> &'static [GameSpec] {
        &[
            GameSpec {
                id: GameId::Soliterm,
                label: "Soliterm",
                selector: "soliterm",
            },
            GameSpec {
                id: GameId::Termwalk,
                label: "Termwalk",
                selector: "termwalk",
            },
            GameSpec {
                id: GameId::Ctris,
                label: "Ctris",
                selector: "ctris",
            },
            GameSpec {
                id: GameId::Sudoku,
                label: "Sudoku",
                selector: "sudoku",
            },
            GameSpec {
                id: GameId::Nimesweeper,
                label: "Nimesweeper",
                selector: "nimesweeper",
            },
            GameSpec {
                id: GameId::Ninvaders,
                label: "Ninvaders",
                selector: "ninvaders",
            },
            GameSpec {
                id: GameId::Greed,
                label: "Greed",
                selector: "greed",
            },
            GameSpec {
                id: GameId::Netwalk,
                label: "Netwalk",
                selector: "net",
            },
            GameSpec {
                id: GameId::Mines,
                label: "Mines",
                selector: "mines",
            },
            GameSpec {
                id: GameId::Solo,
                label: "Solo",
                selector: "solo",
            },
            GameSpec {
                id: GameId::Guess,
                label: "Guess",
                selector: "guess",
            },
            GameSpec {
                id: GameId::Map,
                label: "Map",
                selector: "map",
            },
            GameSpec {
                id: GameId::Untangle,
                label: "Untangle",
                selector: "untangle",
            },
        ]
    }
    pub fn start(id: GameId, seed: u64) -> GameSession {
        GameSession::new(id, seed)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameOutcome {
    Playing,
    Won,
    Lost,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameInput {
    Puzzle(p::PuzzleInput),
    Terminal(t::TerminalInput),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GameScene {
    Puzzle(p::PuzzleScene),
    Terminal(t::TerminalScene),
}
#[derive(Clone, Debug)]
enum Core {
    Puzzle(p::PuzzleSession),
    Terminal(t::TerminalSession),
}
#[derive(Clone, Debug)]
pub struct GameSession {
    id: GameId,
    core: Core,
}
impl GameSession {
    pub fn new(id: GameId, seed: u64) -> Self {
        use GameId::*;
        let core = match id {
            Soliterm => Core::Terminal(t::TerminalSession::new(t::TerminalKind::Soliterm, seed)),
            Termwalk => Core::Terminal(t::TerminalSession::new(t::TerminalKind::Termwalk, seed)),
            Ctris => Core::Terminal(t::TerminalSession::new(t::TerminalKind::Ctris, seed)),
            Sudoku => Core::Terminal(t::TerminalSession::new(t::TerminalKind::Sudoku, seed)),
            Nimesweeper => {
                Core::Terminal(t::TerminalSession::new(t::TerminalKind::Nimesweeper, seed))
            }
            Ninvaders => Core::Terminal(t::TerminalSession::new(t::TerminalKind::Ninvaders, seed)),
            Greed => Core::Terminal(t::TerminalSession::new(t::TerminalKind::Greed, seed)),
            Netwalk => Core::Puzzle(p::PuzzleSession::new(p::PuzzleKind::Net, seed)),
            Mines => Core::Puzzle(p::PuzzleSession::new(p::PuzzleKind::Mines, seed)),
            Solo => Core::Puzzle(p::PuzzleSession::new(p::PuzzleKind::Solo, seed)),
            Guess => Core::Puzzle(p::PuzzleSession::new(p::PuzzleKind::Guess, seed)),
            Map => Core::Puzzle(p::PuzzleSession::new(p::PuzzleKind::Map, seed)),
            Untangle => Core::Puzzle(p::PuzzleSession::new(p::PuzzleKind::Untangle, seed)),
        };
        Self { id, core }
    }
    pub fn new_solitaire(seed: u64, variant: t::SolitaireVariant, passes: u8) -> Self {
        Self {
            id: GameId::Soliterm,
            core: Core::Terminal(t::TerminalSession::new_solitaire(seed, variant, passes)),
        }
    }
    pub fn new_mines(seed: u64, preset: t::MinePreset) -> Self {
        Self {
            id: GameId::Nimesweeper,
            core: Core::Terminal(t::TerminalSession::new_mines(seed, preset)),
        }
    }
    pub fn new_termwalk(seed: u64, size: usize) -> Result<Self, &'static str> {
        Ok(Self {
            id: GameId::Termwalk,
            core: Core::Terminal(t::TerminalSession::new_termwalk(seed, size)?),
        })
    }
    pub fn new_map(seed: u64, preset: p::MapPreset) -> Self {
        Self {
            id: GameId::Map,
            core: Core::Puzzle(p::PuzzleSession::new_map(seed, preset)),
        }
    }
    pub fn id(&self) -> GameId {
        self.id
    }
    pub fn spec(&self) -> &'static GameSpec {
        GameRegistry::catalog()
            .iter()
            .find(|s| s.id == self.id)
            .unwrap()
    }
    pub fn seed(&self) -> u64 {
        match &self.core {
            Core::Puzzle(s) => s.seed(),
            Core::Terminal(s) => s.seed(),
        }
    }
    pub fn help(&self) -> &'static str {
        match &self.core {
            Core::Puzzle(s) => s.kind().help(),
            Core::Terminal(s) => s.kind().help(),
        }
    }
    pub fn scene(&self) -> GameScene {
        match &self.core {
            Core::Puzzle(s) => GameScene::Puzzle(s.scene()),
            Core::Terminal(s) => GameScene::Terminal(s.scene()),
        }
    }
    pub fn outcome(&self) -> GameOutcome {
        match &self.core {
            Core::Puzzle(s) => match s.outcome() {
                p::Outcome::Playing => GameOutcome::Playing,
                p::Outcome::Won => GameOutcome::Won,
                p::Outcome::Lost => GameOutcome::Lost,
            },
            Core::Terminal(s) => match s.outcome() {
                t::Outcome::Playing => GameOutcome::Playing,
                t::Outcome::Won => GameOutcome::Won,
                t::Outcome::Lost => GameOutcome::Lost,
            },
        }
    }
    pub fn input(&mut self, i: GameInput) -> Result<bool, &'static str> {
        match (&mut self.core, i) {
            (Core::Puzzle(s), GameInput::Puzzle(i)) => s.input(i),
            (Core::Terminal(s), GameInput::Terminal(i)) => s.input(i),
            _ => Err("Input belongs to another game family"),
        }
    }
    pub fn reset(&mut self) {
        match &mut self.core {
            Core::Puzzle(s) => s.reset(),
            Core::Terminal(s) => s.reset(),
        }
    }
    pub fn undo(&mut self) -> bool {
        match &mut self.core {
            Core::Puzzle(s) => s.undo(),
            Core::Terminal(s) => s.undo(),
        }
    }
    pub fn can_undo(&self) -> bool {
        match &self.core {
            Core::Puzzle(s) => s.can_undo(),
            Core::Terminal(s) => s.can_undo(),
        }
    }
}
