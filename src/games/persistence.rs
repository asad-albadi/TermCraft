//! TermCraft-owned versioned replay format. No filesystem or executable operations.
//! Engine source identity and full private-state/history SHA-256 are required.
use super::{library::*, puzzles as p};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
pub const MAX_BYTES: usize = 262_144;
pub const MAX_ACTIONS: usize = 4096;
pub const MAX_STEPS: usize = 32768;
const MAX_DEPTH: usize = 16;
const MAX_REPLAY_WORK: usize = 64_000_000;
/// Exact compiled pure-engine source identity, updated automatically on source changes.
pub fn engine_identity() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(|| {
        let mut h = Sha256::new();
        h.update(b"library.rs");
        h.update(b"\0");
        h.update(include_bytes!("library.rs"));
        h.update(b"\0");
        h.update(b"mod.rs");
        h.update(b"\0");
        h.update(include_bytes!("mod.rs"));
        h.update(b"\0");
        h.update(b"persistence.rs");
        h.update(b"\0");
        h.update(include_bytes!("persistence.rs"));
        h.update(b"\0");
        h.update(b"puzzles/config.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/config.rs"));
        h.update(b"\0");
        h.update(b"puzzles/guess.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/guess.rs"));
        h.update(b"\0");
        h.update(b"puzzles/map.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/map.rs"));
        h.update(b"\0");
        h.update(b"puzzles/mines.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/mines.rs"));
        h.update(b"\0");
        h.update(b"puzzles/mod.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/mod.rs"));
        h.update(b"\0");
        h.update(b"puzzles/net.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/net.rs"));
        h.update(b"\0");
        h.update(b"puzzles/solo.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/solo.rs"));
        h.update(b"\0");
        h.update(b"puzzles/solo_advanced.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/solo_advanced.rs"));
        h.update(b"\0");
        h.update(b"puzzles/tests.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/tests.rs"));
        h.update(b"\0");
        h.update(b"puzzles/untangle.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/untangle.rs"));
        h.update(b"\0");
        h.update(b"terminal/blocks.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/blocks.rs"));
        h.update(b"\0");
        h.update(b"terminal/cards.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/cards.rs"));
        h.update(b"\0");
        h.update(b"terminal/grids.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/grids.rs"));
        h.update(b"\0");
        h.update(b"terminal/invaders.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/invaders.rs"));
        h.update(b"\0");
        h.update(b"terminal/mod.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/mod.rs"));
        h.update(b"\0");
        h.update(b"terminal/tests.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/tests.rs"));
        h.update(b"\0");
        let hash: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        format!("termcraft-pure-v1:{hash}")
    })
    .as_str()
}
mod dto {
    use serde::{Deserialize, Serialize};
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
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
    impl From<super::super::library::GameId> for GameId {
        fn from(v: super::super::library::GameId) -> Self {
            match v {
                super::super::library::GameId::Soliterm => Self::Soliterm,
                super::super::library::GameId::Termwalk => Self::Termwalk,
                super::super::library::GameId::Ctris => Self::Ctris,
                super::super::library::GameId::Sudoku => Self::Sudoku,
                super::super::library::GameId::Nimesweeper => Self::Nimesweeper,
                super::super::library::GameId::Ninvaders => Self::Ninvaders,
                super::super::library::GameId::Greed => Self::Greed,
                super::super::library::GameId::Netwalk => Self::Netwalk,
                super::super::library::GameId::Mines => Self::Mines,
                super::super::library::GameId::Solo => Self::Solo,
                super::super::library::GameId::Guess => Self::Guess,
                super::super::library::GameId::Map => Self::Map,
                super::super::library::GameId::Untangle => Self::Untangle,
            }
        }
    }
    impl From<GameId> for super::super::library::GameId {
        fn from(v: GameId) -> Self {
            match v {
                GameId::Soliterm => Self::Soliterm,
                GameId::Termwalk => Self::Termwalk,
                GameId::Ctris => Self::Ctris,
                GameId::Sudoku => Self::Sudoku,
                GameId::Nimesweeper => Self::Nimesweeper,
                GameId::Ninvaders => Self::Ninvaders,
                GameId::Greed => Self::Greed,
                GameId::Netwalk => Self::Netwalk,
                GameId::Mines => Self::Mines,
                GameId::Solo => Self::Solo,
                GameId::Guess => Self::Guess,
                GameId::Map => Self::Map,
                GameId::Untangle => Self::Untangle,
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum GameInput {
        Puzzle(PuzzleInput),
        Terminal(TerminalInput),
    }
    impl From<super::super::library::GameInput> for GameInput {
        fn from(v: super::super::library::GameInput) -> Self {
            match v {
                super::super::library::GameInput::Puzzle(x) => Self::Puzzle(x.into()),
                super::super::library::GameInput::Terminal(x) => Self::Terminal(x.into()),
            }
        }
    }
    impl From<GameInput> for super::super::library::GameInput {
        fn from(v: GameInput) -> Self {
            match v {
                GameInput::Puzzle(x) => Self::Puzzle(x.into()),
                GameInput::Terminal(x) => Self::Terminal(x.into()),
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum Direction {
        North,
        East,
        South,
        West,
    }
    impl From<super::super::puzzles::Direction> for Direction {
        fn from(v: super::super::puzzles::Direction) -> Self {
            match v {
                super::super::puzzles::Direction::North => Self::North,
                super::super::puzzles::Direction::East => Self::East,
                super::super::puzzles::Direction::South => Self::South,
                super::super::puzzles::Direction::West => Self::West,
            }
        }
    }
    impl From<Direction> for super::super::puzzles::Direction {
        fn from(v: Direction) -> Self {
            match v {
                Direction::North => Self::North,
                Direction::East => Self::East,
                Direction::South => Self::South,
                Direction::West => Self::West,
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum PuzzleInput {
        HalfTurn { cell: usize },
        NetSource { direction: Direction },
        Jumble { seed: u64 },
        Rotate { cell: usize, clockwise: bool },
        Lock { cell: usize },
        Reveal { cell: usize },
        Flag { cell: usize },
        Chord { cell: usize },
        Digit { cell: usize, value: u8 },
        Pencil { cell: usize, value: u8 },
        Peg { slot: usize, colour: u8 },
        Hold { slot: usize },
        Submit,
        Colour { region: usize, colour: u8 },
        MapPencil { region: usize, colour: u8 },
        Move { vertex: usize, x: i32, y: i32 },
    }
    impl From<super::super::puzzles::PuzzleInput> for PuzzleInput {
        fn from(v: super::super::puzzles::PuzzleInput) -> Self {
            match v {
                super::super::puzzles::PuzzleInput::HalfTurn { cell } => {
                    Self::HalfTurn { cell: cell.into() }
                }
                super::super::puzzles::PuzzleInput::NetSource { direction } => Self::NetSource {
                    direction: direction.into(),
                },
                super::super::puzzles::PuzzleInput::Jumble { seed } => {
                    Self::Jumble { seed: seed.into() }
                }
                super::super::puzzles::PuzzleInput::Rotate { cell, clockwise } => Self::Rotate {
                    cell: cell.into(),
                    clockwise: clockwise.into(),
                },
                super::super::puzzles::PuzzleInput::Lock { cell } => {
                    Self::Lock { cell: cell.into() }
                }
                super::super::puzzles::PuzzleInput::Reveal { cell } => {
                    Self::Reveal { cell: cell.into() }
                }
                super::super::puzzles::PuzzleInput::Flag { cell } => {
                    Self::Flag { cell: cell.into() }
                }
                super::super::puzzles::PuzzleInput::Chord { cell } => {
                    Self::Chord { cell: cell.into() }
                }
                super::super::puzzles::PuzzleInput::Digit { cell, value } => Self::Digit {
                    cell: cell.into(),
                    value: value.into(),
                },
                super::super::puzzles::PuzzleInput::Pencil { cell, value } => Self::Pencil {
                    cell: cell.into(),
                    value: value.into(),
                },
                super::super::puzzles::PuzzleInput::Peg { slot, colour } => Self::Peg {
                    slot: slot.into(),
                    colour: colour.into(),
                },
                super::super::puzzles::PuzzleInput::Hold { slot } => {
                    Self::Hold { slot: slot.into() }
                }
                super::super::puzzles::PuzzleInput::Submit => Self::Submit,
                super::super::puzzles::PuzzleInput::Colour { region, colour } => Self::Colour {
                    region: region.into(),
                    colour: colour.into(),
                },
                super::super::puzzles::PuzzleInput::MapPencil { region, colour } => {
                    Self::MapPencil {
                        region: region.into(),
                        colour: colour.into(),
                    }
                }
                super::super::puzzles::PuzzleInput::Move { vertex, x, y } => Self::Move {
                    vertex: vertex.into(),
                    x: x.into(),
                    y: y.into(),
                },
            }
        }
    }
    impl From<PuzzleInput> for super::super::puzzles::PuzzleInput {
        fn from(v: PuzzleInput) -> Self {
            match v {
                PuzzleInput::HalfTurn { cell } => Self::HalfTurn { cell: cell.into() },
                PuzzleInput::NetSource { direction } => Self::NetSource {
                    direction: direction.into(),
                },
                PuzzleInput::Jumble { seed } => Self::Jumble { seed: seed.into() },
                PuzzleInput::Rotate { cell, clockwise } => Self::Rotate {
                    cell: cell.into(),
                    clockwise: clockwise.into(),
                },
                PuzzleInput::Lock { cell } => Self::Lock { cell: cell.into() },
                PuzzleInput::Reveal { cell } => Self::Reveal { cell: cell.into() },
                PuzzleInput::Flag { cell } => Self::Flag { cell: cell.into() },
                PuzzleInput::Chord { cell } => Self::Chord { cell: cell.into() },
                PuzzleInput::Digit { cell, value } => Self::Digit {
                    cell: cell.into(),
                    value: value.into(),
                },
                PuzzleInput::Pencil { cell, value } => Self::Pencil {
                    cell: cell.into(),
                    value: value.into(),
                },
                PuzzleInput::Peg { slot, colour } => Self::Peg {
                    slot: slot.into(),
                    colour: colour.into(),
                },
                PuzzleInput::Hold { slot } => Self::Hold { slot: slot.into() },
                PuzzleInput::Submit => Self::Submit,
                PuzzleInput::Colour { region, colour } => Self::Colour {
                    region: region.into(),
                    colour: colour.into(),
                },
                PuzzleInput::MapPencil { region, colour } => Self::MapPencil {
                    region: region.into(),
                    colour: colour.into(),
                },
                PuzzleInput::Move { vertex, x, y } => Self::Move {
                    vertex: vertex.into(),
                    x: x.into(),
                    y: y.into(),
                },
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
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
    impl From<super::super::terminal::TerminalInput> for TerminalInput {
        fn from(v: super::super::terminal::TerminalInput) -> Self {
            match v {
                super::super::terminal::TerminalInput::Rotate { cell, clockwise } => Self::Rotate {
                    cell: cell.into(),
                    clockwise: clockwise.into(),
                },
                super::super::terminal::TerminalInput::Digit { cell, value } => Self::Digit {
                    cell: cell.into(),
                    value: value.into(),
                },
                super::super::terminal::TerminalInput::Reveal { cell } => {
                    Self::Reveal { cell: cell.into() }
                }
                super::super::terminal::TerminalInput::Flag { cell } => {
                    Self::Flag { cell: cell.into() }
                }
                super::super::terminal::TerminalInput::Chord { cell } => {
                    Self::Chord { cell: cell.into() }
                }
                super::super::terminal::TerminalInput::Move { dx, dy } => Self::Move {
                    dx: dx.into(),
                    dy: dy.into(),
                },
                super::super::terminal::TerminalInput::Tick => Self::Tick,
                super::super::terminal::TerminalInput::Drop => Self::Drop,
                super::super::terminal::TerminalInput::Fire => Self::Fire,
                super::super::terminal::TerminalInput::Start => Self::Start,
                super::super::terminal::TerminalInput::Pause => Self::Pause,
                super::super::terminal::TerminalInput::Draw => Self::Draw,
                super::super::terminal::TerminalInput::Transfer { from, to, count } => {
                    Self::Transfer {
                        from: from.into(),
                        to: to.into(),
                        count: count.into(),
                    }
                }
            }
        }
    }
    impl From<TerminalInput> for super::super::terminal::TerminalInput {
        fn from(v: TerminalInput) -> Self {
            match v {
                TerminalInput::Rotate { cell, clockwise } => Self::Rotate {
                    cell: cell.into(),
                    clockwise: clockwise.into(),
                },
                TerminalInput::Digit { cell, value } => Self::Digit {
                    cell: cell.into(),
                    value: value.into(),
                },
                TerminalInput::Reveal { cell } => Self::Reveal { cell: cell.into() },
                TerminalInput::Flag { cell } => Self::Flag { cell: cell.into() },
                TerminalInput::Chord { cell } => Self::Chord { cell: cell.into() },
                TerminalInput::Move { dx, dy } => Self::Move {
                    dx: dx.into(),
                    dy: dy.into(),
                },
                TerminalInput::Tick => Self::Tick,
                TerminalInput::Drop => Self::Drop,
                TerminalInput::Fire => Self::Fire,
                TerminalInput::Start => Self::Start,
                TerminalInput::Pause => Self::Pause,
                TerminalInput::Draw => Self::Draw,
                TerminalInput::Transfer { from, to, count } => Self::Transfer {
                    from: from.into(),
                    to: to.into(),
                    count: count.into(),
                },
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum CardPile {
        Tableau(usize),
        Reserve(usize),
        Foundation(usize),
        Waste,
    }
    impl From<super::super::terminal::CardPile> for CardPile {
        fn from(v: super::super::terminal::CardPile) -> Self {
            match v {
                super::super::terminal::CardPile::Tableau(x) => Self::Tableau(x.into()),
                super::super::terminal::CardPile::Reserve(x) => Self::Reserve(x.into()),
                super::super::terminal::CardPile::Foundation(x) => Self::Foundation(x.into()),
                super::super::terminal::CardPile::Waste => Self::Waste,
            }
        }
    }
    impl From<CardPile> for super::super::terminal::CardPile {
        fn from(v: CardPile) -> Self {
            match v {
                CardPile::Tableau(x) => Self::Tableau(x.into()),
                CardPile::Reserve(x) => Self::Reserve(x.into()),
                CardPile::Foundation(x) => Self::Foundation(x.into()),
                CardPile::Waste => Self::Waste,
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum SolitaireVariant {
        Freecell,
        Klondike,
    }
    impl From<super::super::terminal::SolitaireVariant> for SolitaireVariant {
        fn from(v: super::super::terminal::SolitaireVariant) -> Self {
            match v {
                super::super::terminal::SolitaireVariant::Freecell => Self::Freecell,
                super::super::terminal::SolitaireVariant::Klondike => Self::Klondike,
            }
        }
    }
    impl From<SolitaireVariant> for super::super::terminal::SolitaireVariant {
        fn from(v: SolitaireVariant) -> Self {
            match v {
                SolitaireVariant::Freecell => Self::Freecell,
                SolitaireVariant::Klondike => Self::Klondike,
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum MinePreset {
        Newbie,
        Easy,
        Medium,
        Hard,
    }
    impl From<super::super::terminal::MinePreset> for MinePreset {
        fn from(v: super::super::terminal::MinePreset) -> Self {
            match v {
                super::super::terminal::MinePreset::Newbie => Self::Newbie,
                super::super::terminal::MinePreset::Easy => Self::Easy,
                super::super::terminal::MinePreset::Medium => Self::Medium,
                super::super::terminal::MinePreset::Hard => Self::Hard,
            }
        }
    }
    impl From<MinePreset> for super::super::terminal::MinePreset {
        fn from(v: MinePreset) -> Self {
            match v {
                MinePreset::Newbie => Self::Newbie,
                MinePreset::Easy => Self::Easy,
                MinePreset::Medium => Self::Medium,
                MinePreset::Hard => Self::Hard,
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum PuzzleConfig {
        Net(NetConfig),
        Mines(MinesConfig),
        Solo(SoloConfig),
        SoloVariant(SoloVariantConfig),
        Guess(GuessConfig),
        Map(MapConfig),
        Untangle { points: usize },
    }
    impl From<super::super::puzzles::PuzzleConfig> for PuzzleConfig {
        fn from(v: super::super::puzzles::PuzzleConfig) -> Self {
            match v {
                super::super::puzzles::PuzzleConfig::Net(x) => Self::Net(x.into()),
                super::super::puzzles::PuzzleConfig::Mines(x) => Self::Mines(x.into()),
                super::super::puzzles::PuzzleConfig::Solo(x) => Self::Solo(x.into()),
                super::super::puzzles::PuzzleConfig::SoloVariant(x) => Self::SoloVariant(x.into()),
                super::super::puzzles::PuzzleConfig::Guess(x) => Self::Guess(x.into()),
                super::super::puzzles::PuzzleConfig::Map(x) => Self::Map(x.into()),
                super::super::puzzles::PuzzleConfig::Untangle { points } => Self::Untangle {
                    points: points.into(),
                },
            }
        }
    }
    impl From<PuzzleConfig> for super::super::puzzles::PuzzleConfig {
        fn from(v: PuzzleConfig) -> Self {
            match v {
                PuzzleConfig::Net(x) => Self::Net(x.into()),
                PuzzleConfig::Mines(x) => Self::Mines(x.into()),
                PuzzleConfig::Solo(x) => Self::Solo(x.into()),
                PuzzleConfig::SoloVariant(x) => Self::SoloVariant(x.into()),
                PuzzleConfig::Guess(x) => Self::Guess(x.into()),
                PuzzleConfig::Map(x) => Self::Map(x.into()),
                PuzzleConfig::Untangle { points } => Self::Untangle {
                    points: points.into(),
                },
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct NetConfig {
        pub width: usize,
        pub height: usize,
        pub wrapping: bool,
        pub barrier_percent: u8,
        pub unique: bool,
    }
    impl From<super::super::puzzles::NetConfig> for NetConfig {
        fn from(v: super::super::puzzles::NetConfig) -> Self {
            Self {
                width: v.width.into(),
                height: v.height.into(),
                wrapping: v.wrapping.into(),
                barrier_percent: v.barrier_percent.into(),
                unique: v.unique.into(),
            }
        }
    }
    impl From<NetConfig> for super::super::puzzles::NetConfig {
        fn from(v: NetConfig) -> Self {
            Self {
                width: v.width.into(),
                height: v.height.into(),
                wrapping: v.wrapping.into(),
                barrier_percent: v.barrier_percent.into(),
                unique: v.unique.into(),
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct MinesConfig {
        pub width: usize,
        pub height: usize,
        pub mines: usize,
        pub solvable: bool,
    }
    impl From<super::super::puzzles::MinesConfig> for MinesConfig {
        fn from(v: super::super::puzzles::MinesConfig) -> Self {
            Self {
                width: v.width.into(),
                height: v.height.into(),
                mines: v.mines.into(),
                solvable: v.solvable.into(),
            }
        }
    }
    impl From<MinesConfig> for super::super::puzzles::MinesConfig {
        fn from(v: MinesConfig) -> Self {
            Self {
                width: v.width.into(),
                height: v.height.into(),
                mines: v.mines.into(),
                solvable: v.solvable.into(),
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SoloConfig {
        pub block_width: usize,
        pub block_height: usize,
        pub diagonal: bool,
        pub difficulty: SoloDifficulty,
        pub symmetry: SoloSymmetry,
    }
    impl From<super::super::puzzles::SoloConfig> for SoloConfig {
        fn from(v: super::super::puzzles::SoloConfig) -> Self {
            Self {
                block_width: v.block_width.into(),
                block_height: v.block_height.into(),
                diagonal: v.diagonal.into(),
                difficulty: v.difficulty.into(),
                symmetry: v.symmetry.into(),
            }
        }
    }
    impl From<SoloConfig> for super::super::puzzles::SoloConfig {
        fn from(v: SoloConfig) -> Self {
            Self {
                block_width: v.block_width.into(),
                block_height: v.block_height.into(),
                diagonal: v.diagonal.into(),
                difficulty: v.difficulty.into(),
                symmetry: v.symmetry.into(),
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum SoloDifficulty {
        Trivial,
        Basic,
        Intermediate,
        Advanced,
        Extreme,
        Unreasonable,
    }
    impl From<super::super::puzzles::SoloDifficulty> for SoloDifficulty {
        fn from(v: super::super::puzzles::SoloDifficulty) -> Self {
            match v {
                super::super::puzzles::SoloDifficulty::Trivial => Self::Trivial,
                super::super::puzzles::SoloDifficulty::Basic => Self::Basic,
                super::super::puzzles::SoloDifficulty::Intermediate => Self::Intermediate,
                super::super::puzzles::SoloDifficulty::Advanced => Self::Advanced,
                super::super::puzzles::SoloDifficulty::Extreme => Self::Extreme,
                super::super::puzzles::SoloDifficulty::Unreasonable => Self::Unreasonable,
            }
        }
    }
    impl From<SoloDifficulty> for super::super::puzzles::SoloDifficulty {
        fn from(v: SoloDifficulty) -> Self {
            match v {
                SoloDifficulty::Trivial => Self::Trivial,
                SoloDifficulty::Basic => Self::Basic,
                SoloDifficulty::Intermediate => Self::Intermediate,
                SoloDifficulty::Advanced => Self::Advanced,
                SoloDifficulty::Extreme => Self::Extreme,
                SoloDifficulty::Unreasonable => Self::Unreasonable,
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum SoloSymmetry {
        None,
        Rotate2,
        Rotate4,
        Reflect2,
        ReflectDiagonal,
        Reflect4,
        Reflect4Diagonal,
        Reflect8,
    }
    impl From<super::super::puzzles::SoloSymmetry> for SoloSymmetry {
        fn from(v: super::super::puzzles::SoloSymmetry) -> Self {
            match v {
                super::super::puzzles::SoloSymmetry::None => Self::None,
                super::super::puzzles::SoloSymmetry::Rotate2 => Self::Rotate2,
                super::super::puzzles::SoloSymmetry::Rotate4 => Self::Rotate4,
                super::super::puzzles::SoloSymmetry::Reflect2 => Self::Reflect2,
                super::super::puzzles::SoloSymmetry::ReflectDiagonal => Self::ReflectDiagonal,
                super::super::puzzles::SoloSymmetry::Reflect4 => Self::Reflect4,
                super::super::puzzles::SoloSymmetry::Reflect4Diagonal => Self::Reflect4Diagonal,
                super::super::puzzles::SoloSymmetry::Reflect8 => Self::Reflect8,
            }
        }
    }
    impl From<SoloSymmetry> for super::super::puzzles::SoloSymmetry {
        fn from(v: SoloSymmetry) -> Self {
            match v {
                SoloSymmetry::None => Self::None,
                SoloSymmetry::Rotate2 => Self::Rotate2,
                SoloSymmetry::Rotate4 => Self::Rotate4,
                SoloSymmetry::Reflect2 => Self::Reflect2,
                SoloSymmetry::ReflectDiagonal => Self::ReflectDiagonal,
                SoloSymmetry::Reflect4 => Self::Reflect4,
                SoloSymmetry::Reflect4Diagonal => Self::Reflect4Diagonal,
                SoloSymmetry::Reflect8 => Self::Reflect8,
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct SoloVariantConfig {
        pub base: SoloConfig,
        pub jigsaw: bool,
        pub killer: bool,
    }
    impl From<super::super::puzzles::SoloVariantConfig> for SoloVariantConfig {
        fn from(v: super::super::puzzles::SoloVariantConfig) -> Self {
            Self {
                base: v.base.into(),
                jigsaw: v.jigsaw.into(),
                killer: v.killer.into(),
            }
        }
    }
    impl From<SoloVariantConfig> for super::super::puzzles::SoloVariantConfig {
        fn from(v: SoloVariantConfig) -> Self {
            Self {
                base: v.base.into(),
                jigsaw: v.jigsaw.into(),
                killer: v.killer.into(),
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct GuessConfig {
        pub colours: u8,
        pub pegs: usize,
        pub max_guesses: usize,
        pub allow_blank: bool,
        pub allow_multiple: bool,
    }
    impl From<super::super::puzzles::GuessConfig> for GuessConfig {
        fn from(v: super::super::puzzles::GuessConfig) -> Self {
            Self {
                colours: v.colours.into(),
                pegs: v.pegs.into(),
                max_guesses: v.max_guesses.into(),
                allow_blank: v.allow_blank.into(),
                allow_multiple: v.allow_multiple.into(),
            }
        }
    }
    impl From<GuessConfig> for super::super::puzzles::GuessConfig {
        fn from(v: GuessConfig) -> Self {
            Self {
                colours: v.colours.into(),
                pegs: v.pegs.into(),
                max_guesses: v.max_guesses.into(),
                allow_blank: v.allow_blank.into(),
                allow_multiple: v.allow_multiple.into(),
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub struct MapConfig {
        pub preset: MapPreset,
        pub regions: usize,
        pub difficulty: MapDifficulty,
    }
    impl From<super::super::puzzles::MapConfig> for MapConfig {
        fn from(v: super::super::puzzles::MapConfig) -> Self {
            Self {
                preset: v.preset.into(),
                regions: v.regions.into(),
                difficulty: v.difficulty.into(),
            }
        }
    }
    impl From<MapConfig> for super::super::puzzles::MapConfig {
        fn from(v: MapConfig) -> Self {
            Self {
                preset: v.preset.into(),
                regions: v.regions.into(),
                difficulty: v.difficulty.into(),
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum MapPreset {
        Default,
        Portrait,
    }
    impl From<super::super::puzzles::MapPreset> for MapPreset {
        fn from(v: super::super::puzzles::MapPreset) -> Self {
            match v {
                super::super::puzzles::MapPreset::Default => Self::Default,
                super::super::puzzles::MapPreset::Portrait => Self::Portrait,
            }
        }
    }
    impl From<MapPreset> for super::super::puzzles::MapPreset {
        fn from(v: MapPreset) -> Self {
            match v {
                MapPreset::Default => Self::Default,
                MapPreset::Portrait => Self::Portrait,
            }
        }
    }
    #[derive(Clone, Copy, Debug, Serialize, Deserialize)]
    #[serde(deny_unknown_fields)]
    pub enum MapDifficulty {
        Easy,
        Normal,
        Hard,
        Unreasonable,
    }
    impl From<super::super::puzzles::MapDifficulty> for MapDifficulty {
        fn from(v: super::super::puzzles::MapDifficulty) -> Self {
            match v {
                super::super::puzzles::MapDifficulty::Easy => Self::Easy,
                super::super::puzzles::MapDifficulty::Normal => Self::Normal,
                super::super::puzzles::MapDifficulty::Hard => Self::Hard,
                super::super::puzzles::MapDifficulty::Unreasonable => Self::Unreasonable,
            }
        }
    }
    impl From<MapDifficulty> for super::super::puzzles::MapDifficulty {
        fn from(v: MapDifficulty) -> Self {
            match v {
                MapDifficulty::Easy => Self::Easy,
                MapDifficulty::Normal => Self::Normal,
                MapDifficulty::Hard => Self::Hard,
                MapDifficulty::Unreasonable => Self::Unreasonable,
            }
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum Origin {
    Default {
        id: dto::GameId,
    },
    Puzzle {
        config: dto::PuzzleConfig,
    },
    Solitaire {
        variant: dto::SolitaireVariant,
        passes: u8,
    },
    Mines {
        preset: dto::MinePreset,
    },
    Termwalk {
        size: usize,
    },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum Operation {
    Input(dto::GameInput),
    Undo,
    Redo,
    Solve,
    Ticks(u16),
}
impl Operation {
    pub(super) fn input(i: GameInput) -> Self {
        if matches!(i, GameInput::Terminal(super::terminal::TerminalInput::Tick)) {
            Self::Ticks(1)
        } else {
            Self::Input(i.into())
        }
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    format: String,
    version: u32,
    engine: String,
    seed: u64,
    origin: Origin,
    actions: Vec<Operation>,
    integrity: String,
}
struct HashWriter(Sha256);
impl std::fmt::Write for HashWriter {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        self.0.update(s.as_bytes());
        Ok(())
    }
}
pub(super) fn state_hash(core: &impl std::fmt::Debug) -> String {
    let mut h = HashWriter(Sha256::new());
    write!(&mut h, "{core:?}").expect("hash writer");
    h.0.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
impl Origin {
    fn validate(&self) -> Result<usize, &'static str> {
        match self {
            Self::Default { .. } => Ok(1024),
            Self::Puzzle { config } => {
                let c: p::PuzzleConfig = (*config).into();
                c.validate()?;
                // Native validators cap dimensions, generator search nodes and attempts.
                // One generation per candidate; Reset is deliberately absent from the journal.
                Ok(1024)
            }
            Self::Solitaire { passes, .. } if !(1..=20).contains(passes) => {
                Err("Invalid solitaire passes")
            }
            Self::Termwalk { size } if !(3..=15).contains(size) || size % 2 == 0 => {
                Err("Invalid Termwalk size")
            }
            _ => Ok(1024),
        }
    }
    fn start(&self, seed: u64) -> Result<GameSession, &'static str> {
        match self {
            Self::Default { id } => Ok(GameSession::new((*id).into(), seed)),
            Self::Puzzle { config } => GameSession::new_puzzle(seed, (*config).into()),
            Self::Solitaire { variant, passes } => {
                Ok(GameSession::new_solitaire(seed, (*variant).into(), *passes))
            }
            Self::Mines { preset } => Ok(GameSession::new_mines(seed, (*preset).into())),
            Self::Termwalk { size } => GameSession::new_termwalk(seed, *size),
        }
    }
}
/// Initial choices for restoring menu drafts after a successful load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InitialSettings {
    Default(GameId),
    Puzzle(p::PuzzleConfig),
    Solitaire {
        variant: super::terminal::SolitaireVariant,
        passes: u8,
    },
    Mines(super::terminal::MinePreset),
    Termwalk {
        size: usize,
    },
}
impl GameSession {
    pub fn initial_settings(&self) -> InitialSettings {
        match self.origin {
            Origin::Default { id } => InitialSettings::Default(id.into()),
            Origin::Puzzle { config } => InitialSettings::Puzzle(config.into()),
            Origin::Solitaire { variant, passes } => InitialSettings::Solitaire {
                variant: variant.into(),
                passes,
            },
            Origin::Mines { preset } => InitialSettings::Mines(preset.into()),
            Origin::Termwalk { size } => InitialSettings::Termwalk { size },
        }
    }

    /// Encode a bounded own-format save, including successful undo/redo/assistance.
    pub fn save_bytes(&self) -> Result<Vec<u8>, &'static str> {
        let actions = self
            .journal
            .as_ref()
            .ok_or("Save journal limit reached; reset starts a fresh journal")?;
        preflight(&self.origin, actions)?;
        let file = File {
            format: "TermCraft game".into(),
            version: 1,
            engine: engine_identity().into(),
            seed: self.seed(),
            origin: self.origin.clone(),
            actions: actions.clone(),
            integrity: self.persistence_hash(),
        };
        let bytes = serde_json::to_vec(&file).map_err(|_| "Cannot encode save")?;
        if bytes.len() > MAX_BYTES {
            return Err("Save size limit exceeded");
        }
        Ok(bytes)
    }
    /// Decode and verify a separate candidate; the caller's active game is untouched.
    pub fn load_bytes(bytes: &[u8]) -> Result<Self, &'static str> {
        if bytes.len() > MAX_BYTES {
            return Err("Save size limit exceeded");
        }
        depth_check(bytes)?;
        let file: File =
            serde_json::from_slice(bytes).map_err(|_| "Malformed or unsupported save")?;
        if file.format != "TermCraft game" {
            return Err("Unknown save format");
        }
        if file.version != 1 {
            return Err("Unsupported save version");
        }
        if file.engine != engine_identity() {
            return Err("Save engine mismatch");
        }
        if file.integrity.len() != 64 || !file.integrity.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("Invalid integrity digest");
        }
        preflight(&file.origin, &file.actions)?;
        let mut candidate = file.origin.start(file.seed)?;
        for op in file.actions {
            let changed = match op {
                Operation::Input(i) => candidate.input(i.into())?,
                Operation::Undo => candidate.undo(),
                Operation::Redo => candidate.redo(),
                Operation::Solve => candidate.solve()?,
                Operation::Ticks(n) => {
                    let mut changed = true;
                    for _ in 0..n {
                        if candidate
                            .input(GameInput::Terminal(super::terminal::TerminalInput::Tick))?
                            != true
                        {
                            changed = false;
                            break;
                        }
                    }
                    changed
                }
            };
            if !changed {
                return Err("Save journal contains unsuccessful operation");
            }
        }
        if candidate.persistence_hash() != file.integrity {
            return Err("Save state/history integrity mismatch");
        }
        Ok(candidate)
    }
}
fn preflight(origin: &Origin, actions: &[Operation]) -> Result<(), &'static str> {
    let unit = origin.validate()?;
    for &op in actions {
        action_bounds(op)?;
    }
    if actions.len() > MAX_ACTIONS {
        return Err("Save action limit exceeded");
    }
    // Charge each action a full board scan; repeated first-reveal generation after
    // undo costs a native 64-attempt generator, and has a separate strict quota.
    let steps = actions
        .iter()
        .try_fold(0usize, |n, op| {
            n.checked_add(match op {
                Operation::Ticks(n) => *n as usize,
                _ => 1,
            })
        })
        .ok_or("Save step limit exceeded")?;
    if steps > MAX_STEPS || actions.iter().any(|op| matches!(op, Operation::Ticks(0))) {
        return Err("Save step limit exceeded");
    }
    let mines = matches!(
        origin,
        Origin::Default {
            id: dto::GameId::Mines
        } | Origin::Puzzle {
            config: dto::PuzzleConfig::Mines(_)
        }
    );
    let (mut placed, mut past, mut future, mut generations) =
        (false, Vec::new(), Vec::new(), 0usize);
    if mines {
        for op in actions {
            match op {
                Operation::Undo => {
                    if let Some(before) = past.pop() {
                        future.push(placed);
                        placed = before;
                    }
                }
                Operation::Redo => {
                    if let Some(after) = future.pop() {
                        past.push(placed);
                        placed = after;
                    }
                }
                Operation::Ticks(_) => {}
                Operation::Input(_) | Operation::Solve => {
                    if past.len() == p::PuzzleSession::UNDO_LIMIT {
                        past.remove(0);
                    }
                    past.push(placed);
                    future.clear();
                    if matches!(
                        op,
                        Operation::Input(dto::GameInput::Puzzle(dto::PuzzleInput::Reveal { .. }))
                    ) && !placed
                    {
                        placed = true;
                        generations += 1;
                    }
                }
            }
        }
    }
    // Each first-reveal no-guess generator has 2M primary deduction steps plus
    // eight 500k fallback budgets. Undo must not permit unbounded regeneration.
    let generation_work = generations
        .checked_mul(6_000_000)
        .ok_or("Save work limit exceeded")?;
    let work = steps
        .checked_mul(unit)
        .and_then(|n| n.checked_add(generation_work))
        .ok_or("Save work limit exceeded")?;
    if work.saturating_add(16_000_000) > MAX_REPLAY_WORK {
        return Err("Save work limit exceeded");
    }
    Ok(())
}
fn action_bounds(op: Operation) -> Result<(), &'static str> {
    use dto::{CardPile as C, GameInput as I, PuzzleInput as P, TerminalInput as T};
    let pile = |v: C| match v {
        C::Tableau(n) => n < 8,
        C::Reserve(n) | C::Foundation(n) => n < 4,
        C::Waste => true,
    };
    let valid = match op {
        Operation::Input(I::Puzzle(i)) => match i {
            P::HalfTurn { cell }
            | P::Rotate { cell, .. }
            | P::Lock { cell }
            | P::Reveal { cell }
            | P::Flag { cell }
            | P::Chord { cell } => cell < 1024,
            P::Digit { cell, value } | P::Pencil { cell, value } => cell < 1024 && value <= 16,
            P::Peg { slot, colour } => slot < 8 && colour <= 10,
            P::Hold { slot } => slot < 8,
            P::Colour { region, colour } | P::MapPencil { region, colour } => {
                region < 75 && colour <= 4
            }
            P::Move { vertex, x, y } => {
                vertex < 25 && (0..=1000).contains(&x) && (0..=1000).contains(&y)
            }
            _ => true,
        },
        Operation::Input(I::Terminal(i)) => match i {
            T::Rotate { cell, .. } | T::Reveal { cell } | T::Flag { cell } | T::Chord { cell } => {
                cell < 1024
            }
            T::Digit { cell, value } => cell < 81 && value <= 9,
            T::Move { dx, dy } => (-1..=1).contains(&dx) && (-1..=1).contains(&dy),
            T::Transfer { from, to, count } => pile(from) && pile(to) && (1..=52).contains(&count),
            _ => true,
        },
        _ => true,
    };
    if valid {
        Ok(())
    } else {
        Err("Save action operands exceed limits")
    }
}

fn depth_check(bytes: &[u8]) -> Result<(), &'static str> {
    let (mut depth, mut string, mut escape) = (0usize, false, false);
    for &b in bytes {
        if string {
            if escape {
                escape = false;
            } else if b == b'\\' {
                escape = true;
            } else if b == b'"' {
                string = false;
            }
        } else {
            match b {
                b'"' => string = true,
                b'{' | b'[' => {
                    depth += 1;
                    if depth > MAX_DEPTH {
                        return Err("Save nesting limit exceeded");
                    }
                }
                b'}' | b']' => {
                    depth = depth.checked_sub(1).ok_or("Malformed save nesting")?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::super::*;
    #[test]
    fn all_thirteen_roundtrip() {
        for spec in GameRegistry::catalog() {
            let s = GameSession::new(spec.id, 7);
            let bytes = s.save_bytes().unwrap();
            let restored = GameSession::load_bytes(&bytes).unwrap();
            assert_eq!(s.scene(), restored.scene());
            assert_eq!(bytes, restored.save_bytes().unwrap());
        }
    }
}
#[cfg(test)]
mod integrity_tests {
    use super::super::{GameRegistry, GameScene, terminal as t};
    use super::*;
    fn roundtrip(s: &GameSession) -> GameSession {
        let bytes = s.save_bytes().unwrap();
        let t = GameSession::load_bytes(&bytes).unwrap();
        assert_eq!(s.persistence_hash(), t.persistence_hash());
        assert_eq!(s.scene(), t.scene());
        assert_eq!(s.can_undo(), t.can_undo());
        assert_eq!(s.can_redo(), t.can_redo());
        assert_eq!(s.assisted(), t.assisted());
        t
    }
    fn edit(bytes: &[u8], f: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut v: serde_json::Value = serde_json::from_slice(bytes).unwrap();
        f(&mut v);
        serde_json::to_vec(&v).unwrap()
    }
    #[test]
    fn all_thirteen_successful_actions_and_undo_history() {
        use p::PuzzleInput as P;
        use t::TerminalInput as T;
        for spec in GameRegistry::catalog() {
            let mut s = GameSession::new(spec.id, 7);
            let actions = match spec.id {
                GameId::Soliterm => vec![GameInput::Terminal(T::Transfer {
                    from: t::CardPile::Tableau(0),
                    to: t::CardPile::Reserve(0),
                    count: 1,
                })],
                GameId::Termwalk => vec![GameInput::Terminal(T::Rotate {
                    cell: 0,
                    clockwise: true,
                })],
                GameId::Ctris => vec![GameInput::Terminal(T::Drop)],
                GameId::Sudoku => {
                    let GameScene::Terminal(t::TerminalScene::Sudoku(c)) = s.scene() else {
                        panic!()
                    };
                    let cell = c.fixed.iter().position(|&v| !v).unwrap();
                    (1..=9)
                        .map(|value| GameInput::Terminal(T::Digit { cell, value }))
                        .collect()
                }
                GameId::Nimesweeper => vec![GameInput::Terminal(T::Flag { cell: 0 })],
                GameId::Ninvaders => vec![
                    GameInput::Terminal(T::Start),
                    GameInput::Terminal(T::Fire),
                    GameInput::Terminal(T::Tick),
                ],
                GameId::Greed => {
                    let mut a = Vec::new();
                    for dy in -1..=1 {
                        for dx in -1..=1 {
                            a.push(GameInput::Terminal(T::Move { dx, dy }));
                        }
                    }
                    a
                }
                GameId::Netwalk => vec![GameInput::Puzzle(P::Lock { cell: 0 })],
                GameId::Mines => vec![GameInput::Puzzle(P::Flag { cell: 0 })],
                GameId::Solo => {
                    let GameScene::Puzzle(p::PuzzleScene::Solo(c)) = s.scene() else {
                        panic!()
                    };
                    let cell = c.givens.iter().position(|&v| !v).unwrap();
                    vec![GameInput::Puzzle(P::Pencil { cell, value: 1 })]
                }
                GameId::Guess => vec![GameInput::Puzzle(P::Peg { slot: 0, colour: 1 })],
                GameId::Map => {
                    let GameScene::Puzzle(p::PuzzleScene::Map(c)) = s.scene() else {
                        panic!()
                    };
                    let region = c.givens.iter().position(|&v| !v).unwrap();
                    vec![GameInput::Puzzle(P::MapPencil { region, colour: 1 })]
                }
                GameId::Untangle => vec![GameInput::Puzzle(P::Move {
                    vertex: 0,
                    x: 0,
                    y: 0,
                })],
            };
            let mut changed = false;
            for a in actions {
                if s.input(a) == Ok(true) {
                    changed = true;
                    break;
                }
            }
            assert!(changed, "{:?} must have a successful action", spec.id);
            let mut t = roundtrip(&s);
            assert!(t.undo());
            assert!(s.undo());
            roundtrip(&s);
            assert_eq!(s.persistence_hash(), t.persistence_hash());
            if s.can_redo() {
                assert!(s.redo());
                assert!(t.redo());
                assert_eq!(s.persistence_hash(), t.persistence_hash());
            }
        }
    }
    #[test]
    fn solve_assistance_future_and_branch_are_preserved() {
        let mut s = GameSession::new(GameId::Guess, 5);
        assert_eq!(s.solve(), Ok(true));
        assert!(s.assisted());
        roundtrip(&s);
        assert!(s.undo());
        let mut t = roundtrip(&s);
        assert!(t.redo());
        assert!(t.assisted());
        assert_eq!(
            s.input(GameInput::Puzzle(p::PuzzleInput::Peg {
                slot: 0,
                colour: 2
            })),
            Ok(true)
        );
        assert!(!s.can_redo());
        roundtrip(&s);
    }
    #[test]
    fn noops_errors_reset_and_overflow_keep_gameplay_semantics() {
        let mut s = GameSession::new(GameId::Guess, 5);
        let base = s.save_bytes().unwrap();
        assert!(!s.undo());
        assert!(!s.redo());
        assert_eq!(
            s.input(GameInput::Puzzle(p::PuzzleInput::Peg {
                slot: 0,
                colour: 0
            })),
            Ok(false)
        );
        assert!(
            s.input(GameInput::Puzzle(p::PuzzleInput::Peg {
                slot: 999,
                colour: 1
            }))
            .is_err()
        );
        assert_eq!(base, s.save_bytes().unwrap());
        for n in 0..=MAX_ACTIONS {
            assert_eq!(
                s.input(GameInput::Puzzle(p::PuzzleInput::Peg {
                    slot: 0,
                    colour: (n % 2 + 1) as u8
                })),
                Ok(true)
            );
        }
        assert!(s.save_bytes().is_err());
        s.reset();
        assert_eq!(base, s.save_bytes().unwrap());
    }
    #[test]
    fn settings_and_hidden_state_survive() {
        for preset in [
            t::MinePreset::Newbie,
            t::MinePreset::Easy,
            t::MinePreset::Medium,
            t::MinePreset::Hard,
        ] {
            roundtrip(&GameSession::new_mines(12, preset));
        }
        for variant in [t::SolitaireVariant::Freecell, t::SolitaireVariant::Klondike] {
            for passes in [1, 3, 20] {
                roundtrip(&GameSession::new_solitaire(12, variant, passes));
            }
        }
        for size in [3, 9, 15] {
            roundtrip(&GameSession::new_termwalk(12, size).unwrap());
        }
        let mut s = GameSession::new(GameId::Mines, 7);
        assert_eq!(
            s.input(GameInput::Puzzle(p::PuzzleInput::Reveal { cell: 40 })),
            Ok(true)
        );
        roundtrip(&s);
        assert!(s.undo());
        roundtrip(&s);
        for config in [
            p::PuzzleConfig::Net(p::NetConfig {
                width: 7,
                height: 5,
                wrapping: true,
                unique: false,
                barrier_percent: 30,
            }),
            p::PuzzleConfig::Solo(p::SoloConfig {
                block_width: 2,
                block_height: 2,
                ..Default::default()
            }),
            p::PuzzleConfig::Mines(p::MinesConfig {
                width: 7,
                height: 6,
                mines: 6,
                solvable: false,
            }),
            p::PuzzleConfig::Guess(p::GuessConfig {
                allow_blank: true,
                allow_multiple: false,
                ..Default::default()
            }),
            p::PuzzleConfig::Untangle { points: 25 },
        ] {
            roundtrip(&GameSession::new_puzzle(7, config).unwrap());
        }
    }
    #[test]
    fn malformed_unknown_corrupt_and_oversize_are_rejected_without_mutation() {
        let s = GameSession::new(GameId::Guess, 5);
        let bytes = s.save_bytes().unwrap();
        let before = s.persistence_hash();
        for bad in [
            Vec::new(),
            b"{}".to_vec(),
            vec![b' '; MAX_BYTES + 1],
            vec![b'['; 32],
            edit(&bytes, |v| v["version"] = 2.into()),
            edit(&bytes, |v| v["engine"] = "foreign".into()),
            edit(&bytes, |v| v["extra"] = true.into()),
            edit(&bytes, |v| v["seed"] = 6.into()),
            edit(&bytes, |v| v["integrity"] = "0".repeat(64).into()),
            edit(&bytes, |v| v["actions"] = serde_json::json!(["Reset"])),
            edit(&bytes, |v| v["actions"] = serde_json::json!(["Undo"])),
            edit(&bytes, |v| {
                v["actions"] = serde_json::json!(vec!["Solve"; MAX_ACTIONS + 1])
            }),
        ] {
            assert!(GameSession::load_bytes(&bad).is_err());
            assert_eq!(before, s.persistence_hash());
            assert_eq!(bytes, s.save_bytes().unwrap());
        }
    }
    #[test]
    fn generation_replay_and_operands_are_preflight_bounded() {
        let origin = Origin::Default {
            id: GameId::Mines.into(),
        };
        let reveal = Operation::input(GameInput::Puzzle(p::PuzzleInput::Reveal { cell: 40 }));
        let mut repeated = Vec::new();
        for _ in 0..10 {
            repeated.push(reveal);
            repeated.push(Operation::Undo);
        }
        assert!(preflight(&origin, &repeated).is_err());
        assert!(
            preflight(&origin, &vec![reveal; 100]).is_ok(),
            "ordinary reveals must not be counted as repeated generation"
        );
        assert!(
            preflight(
                &origin,
                &[Operation::input(GameInput::Puzzle(
                    p::PuzzleInput::Reveal { cell: usize::MAX }
                ))]
            )
            .is_err()
        );
        assert!(preflight(&Origin::Termwalk { size: usize::MAX }, &[]).is_err());
        let cfg = p::PuzzleConfig::Net(p::NetConfig {
            width: usize::MAX,
            ..Default::default()
        });
        assert!(preflight(&Origin::Puzzle { config: cfg.into() }, &[]).is_err());
    }
}

#[cfg(test)]
mod long_and_advanced_tests {
    use super::super::terminal as t;
    use super::*;
    #[test]
    fn advanced_solo_topology_and_history_roundtrip() {
        for (jigsaw, killer) in [(true, false), (false, true)] {
            let config = p::PuzzleConfig::SoloVariant(p::SoloVariantConfig {
                base: p::SoloConfig {
                    difficulty: if jigsaw {
                        p::SoloDifficulty::Basic
                    } else {
                        p::SoloDifficulty::Trivial
                    },
                    symmetry: if killer {
                        p::SoloSymmetry::None
                    } else {
                        p::SoloSymmetry::Rotate2
                    },
                    ..Default::default()
                },
                jigsaw,
                killer,
            });
            let mut s = GameSession::new_puzzle(0, config).unwrap();
            assert_eq!(s.configuration(), Some(config));
            assert_eq!(s.solve(), Ok(true));
            assert!(s.undo());
            let mut restored = GameSession::load_bytes(&s.save_bytes().unwrap()).unwrap();
            assert_eq!(s.persistence_hash(), restored.persistence_hash());
            assert_eq!(restored.configuration(), Some(config));
            assert!(restored.redo());
            assert!(restored.assisted());
            assert!(s.redo());
            assert_eq!(s.persistence_hash(), restored.persistence_hash());
        }
    }
    #[test]
    fn timed_games_compress_successful_ticks_and_ignore_idle_ticks() {
        for id in [GameId::Ctris, GameId::Ninvaders] {
            let mut s = GameSession::new(id, 7);
            if id == GameId::Ninvaders {
                s.input(GameInput::Terminal(t::TerminalInput::Start))
                    .unwrap();
            }
            let mut successful = 0;
            for _ in 0..10000 {
                if s.input(GameInput::Terminal(t::TerminalInput::Tick)) == Ok(true) {
                    successful += 1;
                }
            }
            assert!(successful > 10);
            assert!(s.journal.as_ref().unwrap().len() <= 2);
            let bytes = s.save_bytes().unwrap();
            assert!(bytes.len() < 1024);
            let restored = GameSession::load_bytes(&bytes).unwrap();
            assert_eq!(s.persistence_hash(), restored.persistence_hash());
            eprintln!(
                "{id:?}: 10000 timer attempts, {successful} successful ticks, {} bytes",
                bytes.len()
            );
        }
        let mut s = GameSession::new(GameId::Ctris, 7);
        s.input(GameInput::Terminal(t::TerminalInput::Pause))
            .unwrap();
        let bytes = s.save_bytes().unwrap();
        for _ in 0..10000 {
            assert_eq!(
                s.input(GameInput::Terminal(t::TerminalInput::Tick)),
                Ok(false)
            );
        }
        assert_eq!(bytes, s.save_bytes().unwrap());
    }
    #[test]
    fn compressed_tick_step_quota_precedes_replay() {
        let origin = Origin::Default {
            id: GameId::Ctris.into(),
        };
        assert!(preflight(&origin, &[Operation::Ticks(0)]).is_err());
        assert!(preflight(&origin, &[Operation::Ticks(u16::MAX)]).is_err());
        assert!(preflight(&origin, &[Operation::Ticks(32768)]).is_ok());
    }
    #[test]
    fn initial_settings_report_initial_passes_and_variants() {
        let mut s = GameSession::new_solitaire(7, t::SolitaireVariant::Klondike, 4);
        s.input(GameInput::Terminal(t::TerminalInput::Draw))
            .unwrap();
        let restored = GameSession::load_bytes(&s.save_bytes().unwrap()).unwrap();
        assert_eq!(
            restored.initial_settings(),
            InitialSettings::Solitaire {
                variant: t::SolitaireVariant::Klondike,
                passes: 4
            }
        );
        assert_eq!(
            GameSession::new_termwalk(7, 11).unwrap().initial_settings(),
            InitialSettings::Termwalk { size: 11 }
        );
        assert_eq!(
            GameSession::new_mines(7, t::MinePreset::Hard).initial_settings(),
            InitialSettings::Mines(t::MinePreset::Hard)
        );
    }
}

#[cfg(test)]
mod history_boundary_tests {
    use super::*;
    #[test]
    fn complete_bounded_history_future_and_branch_replay() {
        let mut s = GameSession::new(GameId::Guess, 7);
        for n in 0..600 {
            assert_eq!(
                s.input(GameInput::Puzzle(p::PuzzleInput::Peg {
                    slot: 0,
                    colour: (n % 2 + 1) as u8
                })),
                Ok(true)
            );
        }
        for _ in 0..128 {
            assert!(s.undo());
        }
        let mut t = GameSession::load_bytes(&s.save_bytes().unwrap()).unwrap();
        assert_eq!(s.persistence_hash(), t.persistence_hash());
        for _ in 0..128 {
            assert!(s.redo());
            assert!(t.redo());
            assert_eq!(s.persistence_hash(), t.persistence_hash());
        }
        for _ in 0..256 {
            assert!(s.undo());
            assert!(t.undo());
        }
        assert!(!s.undo());
        assert!(!t.undo());
        assert_eq!(
            s.input(GameInput::Puzzle(p::PuzzleInput::Peg {
                slot: 1,
                colour: 3
            })),
            Ok(true)
        );
        assert!(!s.can_redo());
        let t = GameSession::load_bytes(&s.save_bytes().unwrap()).unwrap();
        assert_eq!(s.persistence_hash(), t.persistence_hash());
    }
    #[test]
    fn equal_visible_state_with_different_future_fails_integrity() {
        let original = GameSession::new(GameId::Guess, 7);
        let mut modified = original.clone();
        modified
            .input(GameInput::Puzzle(p::PuzzleInput::Peg {
                slot: 0,
                colour: 1,
            }))
            .unwrap();
        modified.undo();
        assert_eq!(original.scene(), modified.scene());
        assert_ne!(original.persistence_hash(), modified.persistence_hash());
        let mut value: serde_json::Value =
            serde_json::from_slice(&modified.save_bytes().unwrap()).unwrap();
        value["integrity"] = original.persistence_hash().into();
        assert_eq!(
            GameSession::load_bytes(&serde_json::to_vec(&value).unwrap()).unwrap_err(),
            "Save state/history integrity mismatch"
        );
    }
}

#[cfg(test)]
mod engine_source_guard {
    use super::*;
    #[test]
    fn engine_identity_matches_every_exact_core_source() {
        let mut h = Sha256::new();
        h.update(b"library.rs");
        h.update(b"\0");
        h.update(include_bytes!("library.rs"));
        h.update(b"\0");
        h.update(b"mod.rs");
        h.update(b"\0");
        h.update(include_bytes!("mod.rs"));
        h.update(b"\0");
        h.update(b"persistence.rs");
        h.update(b"\0");
        h.update(include_bytes!("persistence.rs"));
        h.update(b"\0");
        h.update(b"puzzles/config.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/config.rs"));
        h.update(b"\0");
        h.update(b"puzzles/guess.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/guess.rs"));
        h.update(b"\0");
        h.update(b"puzzles/map.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/map.rs"));
        h.update(b"\0");
        h.update(b"puzzles/mines.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/mines.rs"));
        h.update(b"\0");
        h.update(b"puzzles/mod.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/mod.rs"));
        h.update(b"\0");
        h.update(b"puzzles/net.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/net.rs"));
        h.update(b"\0");
        h.update(b"puzzles/solo.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/solo.rs"));
        h.update(b"\0");
        h.update(b"puzzles/solo_advanced.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/solo_advanced.rs"));
        h.update(b"\0");
        h.update(b"puzzles/tests.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/tests.rs"));
        h.update(b"\0");
        h.update(b"puzzles/untangle.rs");
        h.update(b"\0");
        h.update(include_bytes!("puzzles/untangle.rs"));
        h.update(b"\0");
        h.update(b"terminal/blocks.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/blocks.rs"));
        h.update(b"\0");
        h.update(b"terminal/cards.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/cards.rs"));
        h.update(b"\0");
        h.update(b"terminal/grids.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/grids.rs"));
        h.update(b"\0");
        h.update(b"terminal/invaders.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/invaders.rs"));
        h.update(b"\0");
        h.update(b"terminal/mod.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/mod.rs"));
        h.update(b"\0");
        h.update(b"terminal/tests.rs");
        h.update(b"\0");
        h.update(include_bytes!("terminal/tests.rs"));
        h.update(b"\0");
        let hash: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(engine_identity(), format!("termcraft-pure-v1:{hash}"));
    }
}
