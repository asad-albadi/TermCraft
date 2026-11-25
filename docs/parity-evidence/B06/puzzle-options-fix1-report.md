# B06 puzzle options FIX1: Net solved-state locks

Status: **ready for fresh scoped root review; no MAIN implementation was changed**. This resolves the one Important P2 finding in `puzzle-options-review.md`; it does not complete B06 or authorize core integration without the compatible dynamic UI addon.

## Review boundary and exact source digests

- Exact unchanged MAIN source-tree base: `b7b4007cb9702475669172b61285b6050b4382fff878768a5e4c31639288ab32`.
- Reviewed round-0 candidate source tree: `298ffd006e3f22f5cfdebda2285fa34e9309ff0c21567cb78fc3190cb50596f7`.
- FIX1 candidate source tree: `255241f23f1a8920bace433935bf1a322f2676c2f4ce8378c7cd784e81fe0585`.
- Tree digests hash sorted `file_sha256  relative/path\n` records. The FIX1 manifest pins every original MAIN, reviewed candidate and amended payload byte stream.
- Preserved round-0 archive: `handoff-puzzle-options.tar.gz`, SHA-256 `a4714ca0d89dfef4173729b3090a89f05cbb492a1a02c0d2d7b14dbcee5b75de`. Its original manifest and patch remain intact; original patch SHA-256 `443175466131406a2604723f18a660a5dca13f27a282f945543fe779a17a22dd`.
- Amended full payload: `handoff-puzzle-options-fix1.tar.gz`; manifest `handoff-fix1-manifest.json`; reviewed-candidate delta `puzzle-options-fix1.patch`; complete MAIN-base delta `puzzle-options-fix1-from-main.patch`.

## Verified source correspondence and correction

Primary source remains `https://git.tartarus.org/simon/puzzles.git` revision `f306b9db55453f7d7fc824bbbf89ca6ac7d2d273`. Exact `net.c:1746–1825` shows the known auxiliary-answer Solve path sets LOCKED on every decoded answer tile (line1775), then constructs the unlock/rotate/relock transitions. This component likewise retains its complete generated answer. The private Net Solve previously changed orientations but preserved the player's partial lock vector; it now also fills the existing lock vector with true.

This is one bounded linear lock fill inside the existing single Solve state snapshot. No constructor, configuration, generator, ordinary lock input, API signature or other engine behavior changed. Existing session snapshots atomically undo both orientations and locks, redo both solved orientations and all solved locks, preserve assistance, and reset to the captured unlocked initial board. Known answers are still exposed only by explicit Solve; this does not introduce original IDs/saved-game/native behavior claims.

The existing options source manifest/excerpt file now additionally retains exact Net Solve lines1746–1825. Every selected raw source file and slice was rechecked against the exact retained primary Git revision; no original/native executable was run or reused.

## Actual RED/GREEN and targeted checks

Two new public-session regressions were added before the correction:

- Default Net seed9 with cell0 locked: RED explicitly reports **1 locked versus25 expected**.
- Custom Net7×5, wrapping,25% barriers, uniqueness disabled, seed9 with first/last cells locked: RED explicitly reports **2 locked versus35 expected**.

Both tests then pass after the correction. Each asserts Won, every solved tile locked, exactly one added undo snapshot, assisted=true, undo restoring the complete earlier partial-lock scene/assisted=false, redo restoring the complete solved scene/all locks/assisted=true, and reset restoring the original unlocked scene with empty undo/redo/assistance. The custom test also checks configuration retention.

Actual private edition2024 checks, all Cargo commands using `CARGO_BUILD_JOBS=2`:

- `cargo test ... net_solve_locks -- --nocapture`: expected RED,2 behavioral assertion failures before implementation.
- `cargo test ... solve -- --nocapture`: GREEN,6 passed (includes both new public regressions and existing common Solve coverage).
- `cargo test ... net:: -- --nocapture`: GREEN,7 passed.
- `cargo test ... redo_restores -- --nocapture`: GREEN,1 passed.
- `cargo fmt --check`: exit0.
- `cargo clippy --all-targets -- -D warnings`: exit0, no warnings.
- `cargo check --target x86_64-pc-windows-msvc`: exit0.
- `cargo check --target aarch64-apple-darwin`: exit0.

The suite now compiles50 tests. **No whole50-test suite or broad288-scene sampler was rerun for this narrow fix**; reviewed round-0 full48/288 evidence remains preserved. This amended evidence is14 distinct passing focused tests, formatting/static checks and pure cross-compilation, not native execution. All worker command jobs finished and were reaped before return.

## Retained scope and integration gaps

The reviewed36 selectable/15 explicitly disabled source preset catalog and previous bounded generator/difficulty disclosures are unchanged. Dynamic Guess vectors, Solo u32/digit ranges, configurable dimensions, preset/fallback metadata, redo/Solve controls and Mines corrections still need the compatible native UI addon before core integration. Advanced Solo/Map variants, broader source customization/generation, hints, original IDs/save interoperability, native/platform/package/original-runtime parity remain pending. MAIN/release source was not edited, no global completion checkpoint changed, and fresh scoped root review is required.
