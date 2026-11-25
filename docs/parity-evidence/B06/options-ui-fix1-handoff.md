# B06 options UI FIX1 — focused keyboard ownership

Private candidate only; pending a fresh independent scoped review. No MAIN product source, accepted core file, dependency, release or package changed. This amends the one Important P2 in `options-ui-review.md`; it does not target the separately prepared advanced-variant core. Private identity remains `harbor-workspace 0.1.609`, bin `termcraft-games-options609`. Every Cargo command used `-j 2` and the shared application target.

## Baseline and exact handoff

The original reviewed three-file payload, manifest, archive and full MAIN patch are preserved byte for byte under `target/agent-work/b06-options-ui/reviewed-ui-baseline`. The original archive and manifest also remain unchanged at their original private paths. Reviewed archive SHA-256 remains `c596fc406101d5ff08c862796f5b2ea953fe178bf3efdbb03f9658460c1f97dd`. Only `src/ui/games/play.rs` changes in this amendment; `src/games/library.rs` and `tests/games.rs` remain exact reviewed bytes.

New private archive: `handoff-puzzle-options-ui-fix1.tar.gz`, SHA-256 **`7aac5e6cdeff21b4b094435c1d55e866db35eb3f7e3910e9f16f33d0259d988c`**. It has precisely the same three regular relative paths, no extra members, and exact manifested bytes. Scoped patch against the reviewed UI candidate: `puzzle-options-ui-fix1.patch`, SHA-256 `5f60571124cac2bc7ced28580536a3da0bbe33b05a629706a63e090a55260e79`. Full amended patch against MAIN: `puzzle-options-ui-fix1-from-main.patch`, SHA-256 `5a1f2c868b717a25ad9443da16c9a782d8f4a734360eb987d20ea2e6f5063e6a`.

`handoff-ui-fix1-manifest.json` records original MAIN4 base hashes/sizes, reviewed UI3 hashes/sizes, amended UI3 hashes/sizes, all required accepted FIX1 core50 dependency hashes/sizes and the three new artifact hashes. Its exact copy is `options-ui-fix1-manifest.json` here. All four MAIN UI bases and all 50 required accepted FIX1 core files still match exact hashes/sizes. Core archive requirement remains `f9923a046d53cbac31f08841a5a6dbbb705dc69c49b63cdca32d1f0dde386ef9`, source tree `255241f23f1a8920bace433935bf1a322f2676c2f4ce8378c7cd784e81fe0585`.

## Correction

The reviewed route forwarded raw keys to the game before focused option widgets could consume them. Gameplay routing now runs after widget handling and reads the remaining events. It checks widget/popup ownership both before and after rendering: Enter can finish an edit and release focus without submitting a Guess in that same frame. This protects numeric TextEdits, sliders, checkbox/buttons and preset popups, rather than discarding all keys.

Board allocation records its actual egui focus id. Clicking or dragging the board requests that focus. Remaining board keys work when the board owns focus or the game surface is unfocused. Board arrows do not also move UI focus; Untangle keeps its Tab point-selection behavior, while Tab in the other games retains native widget navigation. The existing final event quarantine still clears all game/settings input before returning to underlying terminals; root/detached and Close ownership paths are unchanged.

## RED and GREEN evidence

Commands below used the absolute private manifest, `CARGO_TARGET_DIR` pointing at the application's existing target, `-j 2`, and the unique `termcraft-games-options609` bin:

- `cargo test --bin termcraft-games-options609 options_focused`: expected RED on the preserved reviewed code. Actual numeric-control digits changed the live row/history; focused Ctrl+Z also undid a live assisted solution. Both named regressions fail before the correction. Log: `options-ui-fix1-focused-red.log`.
- Initial amended `cargo test --bin termcraft-games-options609 options_`: 15 passed. Includes real numeric edit/digits, Ctrl+Z/Ctrl+Y, arrows, Enter, scene/selection/history/assistance preservation, board click recovery and ordinary board keys. Log: `options-ui-fix1-options-green.log`.
- Additional actual Tab-to-checkbox/preset-button test exposed an overly broad Tab lock in the initial correction. `options-ui-fix1-navigation-red.log` and `options-ui-fix1-navigation-green.log` both retain this failed regression; the first attempted textual amendment did not change the remaining `tab: true` filter. The corrected filter locks Tab only for Untangle, and `options-ui-fix1-navigation-green2.log` passes. This test uses native Tab navigation, Space to change the pending blank option, and Enter/arrows in the preset popup while a complete current Guess row remains unchanged.
- Final `cargo test --bin termcraft-games-options609 ui::games`: **42 passed**, 0 failed. Includes all 16 options UI tests, current default13-game input controls, owned-terminal isolation, root/detached/Close quarantine, idle behavior and coalesced Untangle drag. Log: `options-ui-fix1-final-ui.log`.
- Final `cargo test --test games`: **5 passed**, 0 failed. Log: `options-ui-fix1-final-registry.log`.
- `rustfmt --check --edition 2024` on the three manifested payload files: passed. Log: `options-ui-fix1-final-fmt.log`.
- Exact integrity and inactive-job checks: passed. Log: `options-ui-fix1-integrity.log`.

The focused numeric regression establishes actual egui keyboard focus by clicking the visible colours value after the options animation settles, then proves the draft changes to colours3 without New game. Its current row is already a valid complete guess, so Enter would cause an observable submit if misrouted. It preserves a session with both undo and redo entries and compares the complete undo/redo scene/assistance sequence on clones after each tested input. A separate assisted-game test confirms editor Ctrl+Z cannot undo the revealed solution. Board recovery is checked against independently applied expected moves, undo and redo.

The existing unused Solo label iterator variable warning inside this addon remains; integration compilation also emits the three previously reported outside-addon dead-code warnings. The original handoff's warning description was incomplete. Strict Clippy is not claimed. No broad 843-test repeat, original/native runtime, packaged launch, quota-exhaustion run, save/ID format or advanced-variant parity is claimed. All commands have finished and private Cargo/rustc/linker jobs were checked inactive and reaped before handoff.
