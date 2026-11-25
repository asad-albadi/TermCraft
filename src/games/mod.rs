//! Independent game implementations and workspace idle state.
//! Game-specific entries require recorded original catalog and rule evidence.
pub mod library;
pub mod puzzles;
pub mod screensaver;
pub mod terminal;
pub use library::{GameId, GameInput, GameOutcome, GameRegistry, GameScene, GameSession, GameSpec};

pub mod persistence;
