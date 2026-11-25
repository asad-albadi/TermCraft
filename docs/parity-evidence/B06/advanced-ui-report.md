# B06 advanced puzzle UI bridge

Status: private proposal for independent review. All 51 recovered stock preset configurations are selectable through the existing catalog menu, including the accepted 15 advanced Solo/Map entries. This is a UI bridge to source-defined core behavior, not full B06 or native/original parity. Strict native/original parity count remains 0. Original generation distributions, IDs, saves, native packaging and native runtime parity remain open.

## Frozen inputs and scope

Frozen current MAIN after the options FIX1 merge and formatting into `target/agent-work/b06-advanced-ui/base`. Copied it into `work`, then overlaid the accepted advanced FIX1 archive `766f1716017e8b607b19f4e805781a8637b0b882c6139caa9dd8062d4db901cc`. Verified every one of its 94 proposed file hashes before editing; retained that paired snapshot as `paired-base`. Reverified all 94 accepted hashes after final checks. No advanced core, solver, generator, certificate, quota or search limit changed. MAIN was never edited.

The normalized proposal contains only `src/ui/games/play.rs` and `src/games/library.rs` plus this UI evidence. Private build identity is package 0.1.610 / bin `termcraft-advanced-ui610`; the private Cargo.toml/Cargo.lock changes are excluded from the normalized patch and archive. Both crate roots were touched before the first shared-target build. Every Cargo command used jobs 2. All launched jobs completed and were reaped.

The exact authoritative core API and limitations are the accepted `advanced-puzzles-report.md` and `advanced-core-fix1-review.md`. In particular, source Killer stock remains no givens, no singleton cages, regular 3×3 boxes, no X, no symmetry, Trivial normal logic plus the accepted KINTERSECT requirement. UI controls do not silently rewrite incompatible custom settings: validation explains the constraints, and failed creation retains the current board and history.

## Player behavior

- Every advanced Solo grade and both additional Map grades are editable. Jigsaw and Killer flags are explicit custom controls. Neither flag means Regular Solo; this remains valid and is labeled truthfully. Larger ordinary Solo sizes and 32-bit pencil masks through digit 16 remain supported.
- Solo region borders compare the actual canonical `SoloScene.regions` at each cell edge. This also draws ordinary rectangular regions correctly. Jigsaw no longer receives obsolete rectangular box lines. The existing X flag controls both long diagonal guides.
- Orange inset cage contours follow the actual cage cell sets. Concave corners close without gaps. Each cage's actual sum is drawn at its smallest-index square. Digits and notes leave space above for sum labels. Killer uses 38-pixel cells, with the existing two-way scrolling surface on narrow views.
- Current variant, actual achieved difficulty and meaningful fallback notices are shown. Map reads its actual achieved grade rather than reducing every board to Easy/Normal. Help explains outlined regions, distinct cage digits and sums in player language; no solver certificate counters or technical internals are displayed.
- Existing board focus ownership, keyboard digits/arrows, settings keyboard quarantine, failed New game atomicity, explicit assisted Solve, undo and redo remain in place.

## Verification

Commands ran from the private `work` directory with the shared app target and unique package/bin identity. Full logs are retained beside this report.

1. `cargo test --bin termcraft-advanced-ui610 advanced_ui_ -j 2 -- --test-threads=1`: clean behavioral RED, 0 passed / 5 failed on the minimally exhaustive bridge. It exposed missing region edges, sum glyphs, variant controls, actual preset selection and Map grade reporting. The initial compile RED independently exposed both missing SoloVariant UI matches. An earlier RED log also caught missing cage sums but encountered a texture-delta destructor panic; the test helper was corrected and the clean RED rerun is the retained primary RED evidence.
2. Additional contour-closure RED exposed concave corner gaps; additional compact-layout RED exposed unreadable small cage cells. These checks passed after the narrowly scoped rendering corrections.
3. `cargo test --bin termcraft-advanced-ui610 options_ui_tests -j 2 -- --test-threads=2`: 22 passed, 0 failed. The final source was subsequently covered by the larger games filter below.
4. `cargo test --bin termcraft-advanced-ui610 ui::games -j 2 -- --test-threads=2`: FINAL 48 passed, 0 failed, 156 filtered out. Includes all 22 options tests, six new advanced UI tests, lifecycle/input ownership and the existing real-board/owned-session input quarantine tests.
5. `cargo fmt --all -- --check`: FINAL exit 0. Targeted edition-2024 format check also exits 0. No formatter drift outside the owned files.
6. `cargo clippy --bin termcraft-advanced-ui610 -j 2 --message-format=json`: exit 0. Nine warnings in the owned UI file refer exclusively to primary source text already present in the frozen paired base; `advanced-ui-owned-file-lint-proof.json` records that comparison, with zero new owned-file diagnostics. This is not a claim of a warning-free whole application. Strict all-target Clippy fails on 91 pre-existing library lints. Default all-target Clippy is blocked by existing denied `absurd_extreme_comparisons` in `src/games/terminal/blocks.rs:285,287`. Those sources were not edited and lint rules were not suppressed.

The new checks instantiate successful bounded samples of all 15 advanced presets using the accepted report's seeds (Solo 0; Map Default 30-region Hard/Unreasonable 10, Default 75 Hard 1, Portrait 30 Hard 1 / Unreasonable 0, Portrait 75 Hard 2). They assert all 51 catalog entries have configurations, paint each actual grade, and use actual menu selection plus New game for Intermediate. The real Jigsaw test independently reconstructs every region boundary from neighbor IDs, including absence of obsolete regular-box boundaries, and counts both diagonal guides. The Killer test checks every exposed cage edge has exactly one painted inset segment, all contour endpoints have degree two, each displayed sum is at its actual anchor, and a revealed solution independently sums to each cage target with distinct cage digits. It also checks no givens/no singleton cages, real pointer hit coordinates, focused key input, actual Solve confirmation/Undo/Redo buttons, 32-bit pencil regression, and failure atomicity for both invalid settings and known bounded Map Hard seed-0 exhaustion. Custom no-flags Regular help and settings focus quarantine are exercised.

## Broader-check limits

A one-time broader bin run on an intermediate private build completed: 192 passed, 4 failed, 8 ignored. It was not repeated, and no unrelated source/test was changed. All four failures were ENOENT during test-process setup, rather than failed puzzle assertions:

- Three RDP tests fail at `tests/support/rdp_server.rs:97`, spawning the fixture executable. The private snapshot excludes target directories. A nested fixture build inherits the shared `CARGO_TARGET_DIR` and writes the executable to the shared target, while its helper returns the private `docs/parity-evidence/rdp-prototype/target/debug/termcraft-rdp-engine-evaluation` path. The latter is absent; the shared-target executable exists. Parent owns the whole-app gate and has its native fixture setup.
- `ui::xserver::tests::openssh_without_display_blocks_only_requested_forwarding` fails at `src/ui/xserver.rs:637`, restarting `current_exe()` with ENOENT. Another focused Cargo build replaced the same private test executable while the broader process was still running. That artifact-replacement explanation is an inference from the overlap and failing spawn; no Xserver source was edited and this task makes no Xserver correctness claim.

There is no native GUI screenshot/execution or cross-platform GUI compile claim in this handoff. The accepted pure-core cross-target checks remain separate. Fresh independent review and the parent's atomic advanced-core/UI merge are still required.
