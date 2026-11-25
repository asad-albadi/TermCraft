# Pure game persistence API

- `GameSession::save_bytes(&self) -> Result<Vec<u8>, &'static str>`
- `GameSession::load_bytes(bytes: &[u8]) -> Result<GameSession, &'static str>` returns a separate fully verified candidate. Does not mutate any existing session or supplied bytes.
- `games::persistence::MAX_BYTES = 262_144`, `MAX_ACTIONS = 4096`, `MAX_STEPS = 32768`; over-limit sessions continue playing but cannot save until Reset starts a fresh journal.
- `games::persistence::engine_identity()` binds replay to exact accepted pure-core source content.
- Common session input/undo/redo/Solve/reset hooks record successful operations only. Constructors preserve default-vs-configured generation paths, Solitaire passes, Termwalk size, terminal Mines preset, every PuzzleConfig including SoloVariant.
- Backend has no UI or filesystem; caller must enforce source type/byte limits while reading, atomic publication/overwrite consent, dirty-state handling, and load generation identity before adopting the candidate.
- Error strings are readable UI messages. Own format only; no original saved-game compatibility.

`GameSession::initial_settings() -> games::persistence::InitialSettings` is a read-only native-typed initial configuration accessor:

```rust
Default(GameId)
Puzzle(PuzzleConfig)
Solitaire { variant: SolitaireVariant, passes: u8 }
Mines(MinePreset)
Termwalk { size: usize }
```

Consecutive successful timer ticks use typed `Ticks(u16)` runs. Failed/no-op/paused ticks do not increase the journal. Every expanded tick remains subject to successful-operation and complete final-state/history verification. Quotas are preflighted before candidate construction; Undo-mediated Mines first-reveal generation is separately accounted against the work budget.
