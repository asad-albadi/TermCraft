# Terminal core review amendments

2026-11-25. Scoped follow-up to accepted `terminal-core-review.md`; independent review of these amendments and common UI integration remain root-owned. No main product source was edited.

The original eight-file source snapshot is preserved in `target/agent-work/b06-terminal/amendments/baseline/src/games/terminal/`. The original handoff tar remains byte-identical with SHA-256 `7a5f17d6f6003f4a55f25cf9b921669bb5ff0ef8609754ebcc6c86026b04c4d8`. The scoped five-file patch is `target/agent-work/b06-terminal/amendments/terminal-core-amendments.patch`, SHA-256 `79257d2559792f36d009e0fbe116702f5d05f06c1161e05a83cd6c099bb49acc`. Per-file baseline/amended hashes appear in `terminal-core-amendments-manifest.json`. Three of the eight source files are unchanged.

- `blocks.rs`: public `Piece::cells()` uses saturating additions for arbitrary i32 coordinates, with an explicit API contract. Extreme out-of-board coordinates can merge cells at the integer limits; valid session positions retain exact results. A RED regression first reproduced the debug overflow; the restored implementation passes all 256 public kinds × five rotation values × both coordinate extremes plus existing rotation/board fixtures.
- `invaders.rs`: public `NinvadersScene::player_y` uses the same `PLAYER_Y` constant as shot origin, enemy-shot damage and enemy ground contact. The UI owner was notified to render this explicit field. A focused contract fixture obtains the row from the public scene, checks shot origin at row−1, confirms no damage before the shot reaches the row, checks damage on that row, and ground-contact loss.
- `tests.rs` / `PROVENANCE.md` / `API.md`: paused Ctris, Nimesweeper and started nInvaders intentionally ignore ordinary gameplay inputs before validation, return `Ok(false)`, and add no history. Pause/start lifecycle controls remain separate. Tests cover malformed indices/deltas while paused, unchanged scenes/history, and active rejection after unpause. Documentation now accurately specifies this intended behavior.
- Original owner evidence wording in `terminal-core.md` now calls its first 64-seed Sudoku check a consistency check using the production solver with a larger budget. The separate fresh reviewer used a genuinely independent bitmask MRV solver for 64 additional seeds (64–127), all unique, as reported in `terminal-core-review.md`; that audit is not relabelled as owner work.

## Focused verification

All Cargo calls used `CARGO_BUILD_JOBS=2` and only the private component target. No broad application tests/builds ran in this lane.

```
CARGO_BUILD_JOBS=2 cargo test --offline --lib --manifest-path app/target/agent-work/b06-terminal/component/Cargo.toml arbitrary_public_piece_coordinates_saturate_instead_of_overflowing
CARGO_BUILD_JOBS=2 cargo test --offline --lib --manifest-path app/target/agent-work/b06-terminal/component/Cargo.toml
CARGO_BUILD_JOBS=2 cargo fmt --check --manifest-path app/target/agent-work/b06-terminal/component/Cargo.toml
patch --dry-run -p1 -d app/target/agent-work/b06-terminal/amendments/baseline < app/target/agent-work/b06-terminal/amendments/terminal-core-amendments.patch
```

First targeted run: expected RED, one failure with `attempt to add with overflow` (`amendments/red-piece.log`). Final covering suite: **33 passed, 0 failed**, no warnings, 0.84 seconds (`amendments/green.log`). Formatting passed; scoped patch dry-run matched all five preserved baseline files. No new game features, original execution, native parity claims, or B06 completion claims were introduced. All subprocesses exited synchronously; no owned background jobs remain.
