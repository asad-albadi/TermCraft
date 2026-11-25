# Advanced core FIX1: truthful unflagged Solo metadata

This private amendment resolves the one P2 finding in `b06-advanced-review/docs/parity-evidence/B06/advanced-core-review.md`. It does not touch MAIN, UI code or other agents' copies.

The reviewed public configuration `SoloVariantConfig { base: SoloConfig::default(), jigsaw:false, killer:false }` already validated and generated ordinary9×9 boxes without cages, but its help said `9×9 Killer Solo`. FIX1 preserves that valid ordinary configuration and labels it `9×9 Regular Solo`. The existing Jigsaw and Killer branches retain their names. The public type documentation now states that both flags false selects ordinary9×9 regions and no cages. There is no validation, generation, grading, topology, source-preset or lifecycle implementation change.

The new public-session regression checks the exact no-flags configuration: variant and wrapper validation succeed; help names Regular without Killer/Jigsaw; `configuration`, `kind` and session help agree; the live scene has9×9 dimensions, ordinary3×3 region IDs and no cages/diagonals; its requested/achieved grade is Trivial with no Killer certificate flag. It exercises explicit assisted Solve, undo and reset. It also verifies both existing default Solo entry points retain plain `PuzzleConfig::Solo(default)`, ordinary regions, no cages and truthful help.

## Frozen ancestry and packaging

The original reviewed archive `995be7c556d0a4472d2bdbb277d2dda3f0fb19e341394fdae02ef2916479a79d`, original amendment patch `1f22533583df5cd885d0ffc4d3fbc021f38588c7342854001fbffd7e31aba7d5`, original manifest, original accepted50-file base, original83-file proposal and all original payload files remain immutable. FIX1 uses a new `b06-advanced/fix1/base` copy that matches every original proposed-file hash, and a separate `fix1/work` copy. The manifest records every reviewed and amended file hash.

The FIX1 amendment patch is computed only against that frozen reviewed83-file base. The updated full archive contains the complete amended file map, including unchanged prior evidence. An independent patch application must reconstruct the exact proposed map. The only production file with changed behavior is `config.rs`; `solo.rs` changes one public documentation line, and `tests.rs` adds the focused public regression. Remaining changes are scoped FIX1 evidence.

## Actual RED/GREEN and checks

Before changing help, the new test compiled and failed its actual behavioral assertion, showing `9×9 Killer Solo` for the accepted no-flags ordinary configuration; exit101. The same test passed after the label correction, including its topology/configuration/default-API checks. This RED is not a missing-symbol or compilation failure.

Focused GREEN results: one new metadata/topology/default-API test; two default Solo symmetry/logical replay tests; five existing advanced topology/validation tests (including real Jigsaw peers and exact Killer KINTERSECT grading); one existing redo/history test; one existing atomic assisted Solve/undo test. Total10 targeted tests passed across the five invocations. Strict all-target Clippy, format check and pure-component aarch64 macOS/x86_64 MSVC Windows compile checks also pass. All Cargo builds use edition2024 and jobs2.

The broad15-preset sample matrix and legacy broad sampler were not rerun: the label-only behavior change does not alter any preset, generator, solver or topology. Prior independent review established those15 source-defined cores and their bounded exact-grade samples. The unchanged source-defined stock configurations and generator/solver bytes remain pinned in the ancestry; all51 presets stay selectable at the core API. This report adds no original-runtime, seed/game-ID/save or native UI claim.

Fresh scoped parent review is required before acceptance. Advanced UI bridging remains separate. Strict native/original parity count remains0.
