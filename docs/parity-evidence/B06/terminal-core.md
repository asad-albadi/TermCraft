# B06 independent terminal cores — handoff 2026-11-25

Seven visible original declarations implemented independently: Soliterm (Freecell and Klondike), Termwalk, Ctris, Sudoku, Nimesweeper, nInvaders and Greed. Typespeed remains excluded because the exact menu declares it hidden. Component implementation is ready for independent review and common registry/UI integration. This report does not qualify original runtime parity, native behavior, or B06 completion.

Owned source: `target/agent-work/b06-terminal/component/src/games/terminal/**`; frozen integration contract in `API.md`, per-game rules/provenance and explicit historical gaps in `PROVENANCE.md`. Main product source was not edited. Only this report and `terminal-core-manifest.json` were added to the main evidence directory. Each owned target path was absent from main at handoff; SHA-256s and exact reviewed evidence input hashes are in the manifest. No original binary/assets are included.

## Verification

Package `b06-terminal-component` v0.6.27, no dependencies. Every cargo invocation used `CARGO_BUILD_JOBS=2` and the component's separate target folder. Commands run from app parent:

```
CARGO_BUILD_JOBS=2 cargo fmt --check --manifest-path app/target/agent-work/b06-terminal/component/Cargo.toml
CARGO_BUILD_JOBS=2 cargo test --offline --lib --manifest-path app/target/agent-work/b06-terminal/component/Cargo.toml
python app/target/agent-work/b06-terminal/run_mutations.py
```

- Initial RED: catalog assertion failed (0 expected 7). Subsequent behavioral RED (`red.log`): 11 failures from transition stubs with two baseline tests passing; genuine legal rule assertions failed for every game family. Grid-size RED (`red-size.log`): requested 3 returned default 5 before variable odd-size support.
- Final GREEN (`green.log`): **30 passed, 0 failed**, no compiler warnings; final suite finished in 0.82s. Formatting check passed. These are component tests, not the application's full suite.
- Seven distinct source mutations were each rejected by the relevant focused rule test: fixed Sudoku clue editing; reversed Termwalk rotation; no mine detonation loss; wrong-suit foundation acceptance; disabled Ctris full-row clearing; no life lost from enemy shot; Greed ignored first digit's distance. Each mutation run compiled, then failed a behavioral assertion. Restored source was tested again. `mutations.json` plus seven per-mutation logs retained in the private handoff directory.
- Generator/property checks cover 64 Sudoku seeds (valid givens, unique count using the same production solver with a larger budget, at least 39 clues); 64 Termwalk seeds (48 ports, legal masks, nontrivial initial state, rotation cycle); 64 Freecell and 64 Klondike deals (all 52 unique cards and face-up counts), all four recovered mine presets, odd Termwalk sizes 3/7/9/15, adversarial max indices/deltas, safe first mine click, wrong/correct chords, and exact undo/reset including RNG. History is capped at 128 states, shooter shots bounded, Sudoku search capped at 50,000 nodes per attempted removal.
- Explicit fixtures establish full Sudoku completion, all-25 connected Termwalk win, Greed full clear/walled-in loss, mine clear/death, Ctris row clear/spawn loss, card foundation win and nInvaders last-enemy win/zero-life loss.

## Remaining scope and evidence limits

Public typed scenes/input/state transitions are ready; UI, registry, native smoke, idle/screensaver and the full application's tests belong to root integration. No original executable, extracted game, checksum-bad stgames payload, or original artwork was executed, linked, distributed or used as a runtime oracle. No measured outcomes were invented. No commit: workspace is not a Git checkout.

The local reviewed evidence establishes Sudoku/Greed rules, named variants, Termwalk objective/odd sizes, mine presets and some controls, but not exact original Ctris/nInvaders gameplay. Those cores deliberately implement documented conventional mechanics. Exact historical defaults/timing/scoring/deals/skill curves, original Ctris keys/wall kicks, shooter shields/UFO/multiple waves, Soliterm stalemate detection/hints, Sudoku solve/hint/save/clear/fix commands, mine score/times and Termwalk solve-grid UI remain gaps as detailed in PROVENANCE.md. These independently playable cores must not be reported as measured original-game parity.

All compiler/mutation/test subprocesses exited synchronously. No spawned agents, persistent processes or background jobs remain owned by this lane.

## Scoped review amendments

The owner uniqueness test reuses production `count_solutions`; it is a consistency check, not an independent solver oracle. The fresh independent reviewer separately certified seeds 64–127 using a separately written bitmask MRV solver (64 additional seeds, all exactly one solution), as recorded in `terminal-core-review.md`. Its verification is distinct from the owner's original 64-seed tests.

Post-review private amendments make public `Piece::cells()` total by saturating coordinate offsets, document ignored paused gameplay inputs, and expose nInvaders `player_y` from the same constant used for collision/shot origin. The original eight-file manifest/tar remain the reviewed baseline. Scoped patch, amended hashes and fresh focused checks are recorded in `terminal-core-amendments.md`; application integration remains separate.
