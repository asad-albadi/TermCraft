# B06 puzzle options component handoff

**Status: ready for fresh root review; not integrated, not full B06, not native/original equivalence.** No MAIN source was modified. The private component uses edition 2024, no dependencies, and its own target directory. Root must review and integrate the candidate before the UI options addon consumes it. Existing default UI remains a separate tested boundary.

## Exact boundaries and hashes

Private component: `target/agent-work/b06-puzzle-options`. Source ownership: `src/games/puzzles/**`; evidence ownership: `docs/parity-evidence/B06/puzzle-options*` only. No terminal/UI files were edited. Exact MAIN source base was checked again before packaging and remains unchanged.

- Base source-tree SHA-256: `b7b4007cb9702475669172b61285b6050b4382fff878768a5e4c31639288ab32`
- Candidate source-tree SHA-256: `298ffd006e3f22f5cfdebda2285fa34e9309ff0c21567cb78fc3190cb50596f7`
- Tree digests hash sorted `sha256  relative/path\n` records; per-file original/candidate SHA-256 and sizes are in `handoff-manifest.json`.
- Historical primary revision: `f306b9db55453f7d7fc824bbbf89ca6ac7d2d273` from `https://git.tartarus.org/simon/puzzles.git`.
- Exact source raw bytes for all newly selected slices were verified using `git show` against the retained primary Git database. The candidate provenance manifest/slices record source line ranges and hashes. The original binaries/artwork were never executed or reused.

## Concrete extension

- Fixed default Solo generation: atomic half-turn clue orbits, complete **DIFF_BLOCK box-position-only replay**. This is algorithmic grading, not clue-count inference. Both independent rotational-mask and block-only replay tests fail against unchanged MAIN baseline and pass here.
- Net: real dimensions/wrapping/hard reciprocal barriers, loop/leak-aware completion, all ten stock presets, typed bounded custom settings. Barrier rate uses the source fixed fraction of unused edges after jumbling and nested sets on the same candidate; a behavioral regression caught the earlier per-edge approximation and it was corrected before this handoff. Unique custom search exhaustion is explicit; legacy default fallback is separately tested unique.
- Solo: physical boxes through 4×4, Basic deductions, 9-symbol X rules, all eight source symmetries, constructor certificate with requested/achieved grade and explicit easier fallback after at most eight Basic candidates. Source c×r stock labels map to physical box width r/height c (`solo.c:3611`), verified by a separate RED/GREEN catalog regression. Digits through16 use u32 pencil masks.
- Mines: all six stock dimensions/counts and bounded custom settings, mandatory full no-guess replay whenever ensure-solvable is selected, disclosed certified structured fallbacks and explicit exhaustion. First reveal remains safe. Explicit Solve supports normal completion and source-defined post-death corrections; hidden layout metadata is absent before Solve.
- Guess: Standard/Super, dynamic peg vectors, bounded custom colours/pegs/attempts, source blank and distinct-colour rules, duplicate-aware marking. Hidden codes never contain blank pegs. Solve gives up/reveals as the source does.
- Map: Easy singles and existing Normal proof rules, both 30-region orientations and both 75-region Normal stock dimensions. Large generation failure is explicit, never substituted with the small board. Existing Normal fallback proof remains; Easy fallback requires its own complete Easy replay.
- Untangle: all five stock counts and bounded4–25 custom counts, preserving exact crossing/touch/overlap/shared-point rules and disclosed outerplanar generator subset.
- Common controls: bounded redo, invalid/no-op branch preservation, atomic explicit Solve, assistance tracked through undo/redo, reset to captured initial seed/configuration. Configured help and exact stock metadata expose **36 selectable / 15 disabled** entries, including both conditional Map orientations. Unsupported variants cannot silently select another difficulty.

## API for the later UI addon

Retained constructors and methods: `new`, `new_map`, `input`/`apply`, `undo`, `reset`, `scene`, `outcome`, `kind`, `seed`, `can_undo` and kind selectors/default help. Added:

- `PuzzleConfig::{Net(NetConfig),Mines(MinesConfig),Solo(SoloConfig),Guess(GuessConfig),Map(MapConfig),Untangle{points}}`, with `kind`, `validate`, configured `help`.
- `PuzzleSession::new_config(seed, config) -> Result<Self, &'static str>`, `configuration() -> PuzzleConfig`, session `help() -> String`.
- `redo()`, `can_redo()`, `solve() -> Result<bool, &'static str>`, `assisted()`.
- `PuzzleKind::default_configuration()` and `presets() -> Vec<PuzzlePreset>`; each preset has name, optional configuration, explanatory note.
- `NetConfig`: width, height, wrapping, integral barrier_percent, unique. Scene adds wrapping and barrier masks.
- `MinesConfig`: width, height, mines, solvable. Scene adds generation/solvable and explicit-Solve-only `solution: Option<Vec<bool>>`.
- `SoloConfig`: physical block_width/block_height, diagonal, difficulty Trivial/Basic, symmetry. Scene adds these dimensions/options and a logical certificate. Pencil masks become `Vec<u32>`.
- `GuessConfig`: colours, pegs, max_guesses, allow_blank, allow_multiple. Current/held/row pegs/terminal answer become vectors, retaining four-element default lengths and supporting five-element Super.
- `MapConfig`: preset orientation, regions30/75, difficulty Easy/Normal. Scene exposes configuration.

The UI must use scene lengths/dimensions, digit ranges and configured help, display fallback/grade metadata, disable unsupported presets, handle bounded construction/first-reveal errors, and render explicit Mines corrections. Current fixed4 Guess controls and 1–9 Solo selectors need adaptation. The core does not edit those UI files.

## Actual verification

All Cargo commands used `CARGO_BUILD_JOBS=2` and this private manifest/target. Final restored source:

- Whole component suite: **48 passed, 0 failed**, plus0 doc tests; Linux. Final log records16.08s on this machine, not a native timing claim.
- Edition2024 `cargo fmt --check`: exit0.
- `cargo clippy --all-targets -- -D warnings`: exit0, no warnings.
- `cargo check --target x86_64-pc-windows-msvc`: exit0.
- `cargo check --target aarch64-apple-darwin`: exit0.
- Eleven controlled mutations each produced a behavioral assertion failure: Solo symmetry and block grade; Net wrap/barriers/tree rule; redo no-op history; Solve assistance; Guess distinct and blank rules; Mines correction disclosure; Untangle point counts. All originals were restored before the final whole suite/checks. See results JSON, replay script and individual logs. An initial mutation of only the north seam revealed that the fixture checked only a horizontal seam; the fixture was strengthened to assert both north/south and east/west seams, then the mutation failed behaviorally.
- Separate unchanged-MAIN behavioral RED: both Solo default regressions fail as expected (symmetry seed0 and DIFF_BLOCK replay seed9). Separate Net barrier-rate and Solo preset-orientation regressions fail before their corrections and pass afterward. Early API declaration checks also produced compile RED locally; those are not represented as behavioral proof.
- Eight local seeds for each of36 selectable stock settings: **288 successful generations, 0 sampled explicit exhaustions**. Dense Mines9×9/35,16×16/99 and30×16/170 use certified fallback8/8 each; other Mines stock samples use randomized generation. Physical source2x3 Basic reports an easier Trivial fallback3/8 after bounded retries; all other sampled Solo entries achieve their requested grade. Map30-region Normal uses fallback2/8 in each orientation; other sampled Map entries are randomized. These are honest local samples, not distribution or original PRNG equivalence.

No full parent application test, native GUI/package or original binary execution was performed by this component worker. Fresh root review/integration is required. All local command jobs were finished/reaped before handoff.

## Remaining implementation, not silently approved

Fifteen source stock entries remain disabled: nine advanced Solo entries (Intermediate, Advanced, Advanced X, Extreme, Unreasonable, Killer, Jigsaw Basic, Jigsaw Basic X, Jigsaw Advanced) and six Map Hard/Unreasonable entries across both orientations. Full source customization beyond explicit local caps also remains: Solo cages/jigsaws/higher grades/broader dimensions, arbitrary Map dimensions/region counts, Net1/2 or over16 dimensions and fractional percent rates. Untangle still uses an outerplanar subset. Separate hints, copy/export/import/saved games/original generator IDs and independently verified interoperability remain. Native options/redo/Solve/correction UI and platform parity remain for their owners. **This candidate extends restricted B06; it does not finish B06 or advance any global completion checkpoint.**
