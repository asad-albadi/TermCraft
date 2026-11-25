# B06 advanced puzzle preset amendment

Status: private proposed amendment for fresh review; no MAIN or other-agent source edits. This enables the remaining15 recovered source presets on the accepted36-preset FIX1 component. All51 stock entries now have configurations. This is source-defined puzzle-core coverage, not native original parity; strict native/original count remains0.

Base: `handoff-puzzle-options-fix1.tar.gz`, SHA-256 `f9923a046d53cbac31f08841a5a6dbbb705dc69c49b63cdca32d1f0dde386ef9`. The complete accepted50-file payload was extracted into immutable `b06-advanced/base` and copied into `work` before edits. The amendment patch is computed against that frozen base, never against changing MAIN. See handoff manifest for every base/proposed file hash and exact archive/patch hashes.

Primary historical authority: Simon Tatham Puzzles Git `f306b9db55453f7d7fc824bbbf89ca6ac7d2d273`, companion historical version20150612. `verify-source.py` compares `solo.c`, `map.c`, `puzzles.but`, and `LICENCE` byte-for-byte against exact Git objects, then retains source/manual slices with hashes in `provenance/advanced-source-manifest.json`. No binary/art was used, executed, ported, or repackaged. Source-derived safe Rust rules retain upstream MIT-style copyright/licence in `provenance/UPSTREAM-LICENCE`.

## Exact remaining stock coverage

Solo names/settings come from `solo.c:304–322`; the ordinary block width/height interpretation remains as accepted FIX1. The source Jigsaw c9/r1 settings are represented as side9 with explicit irregular regions, without claiming encoded-parameter compatibility. Map names/settings come from `map.c:116–132`, including both conditional orientations.

| Source stock entry | Rules and exact grade | Successful local seed |
|---|---|---:|
| 3x3 Intermediate | DIFF_INTERSECT; half-turn clues; block/row/column intersections |0|
| 3x3 Advanced | DIFF_SET; all unit Hall subsets/hidden complements |0|
| 3x3 Advanced X | DIFF_SET plus both long diagonals |0|
| 3x3 Extreme | DIFF_EXTREME; single-number row/column sets and bivalue peer-chain BFS |0|
| 3x3 Unreasonable | DIFF_RECURSIVE; Extreme stalls; bounded count to two yields exactly one |0|
| 3x3 Killer | Normal DIFF_BLOCK, killer DIFF_KINTERSECT, SYMM_NONE; no given digits; connected distinct-digit sum cages, no singleton cages |0|
| 9 Jigsaw Basic | DIFF_SIMPLE; nine connected nine-cell irregular regions, half-turn clues |0|
| 9 Jigsaw Basic X | DIFF_SIMPLE plus both long diagonals |0|
| 9 Jigsaw Advanced | DIFF_SET with real irregular regions |0|
| 20x15,30 regions,Hard | Source forcing-chain BFS; Normal stalls |10|
| 20x15,30 regions,Unreasonable | Hard stalls; bounded unique recursion |10|
| 30x25,75 regions,Hard | Source forcing-chain BFS; Normal stalls |1|
| 16x18,30 regions,Hard | Source forcing-chain BFS; Normal stalls |1|
| 16x18,30 regions,Unreasonable | Hard stalls; bounded unique recursion |0|
| 25x30,75 regions,Hard | Source forcing-chain BFS; Normal stalls |2|

Local seeds are reproducibility inputs for this independent generator, not original game IDs. All advanced constructors either produce the exact requested logical grade/topology or report explicit bounded exhaustion. There is no clue-count difficulty classifier and no advanced easier fallback. In particular the Map Default Hard/Unreasonable samples need seed10: seeds failing the16-candidate bound return an error rather than a mislabeled easier puzzle.

Killer source routines implemented: remove filled cells/reduce sums and enforce cage distinctness; singleton sums; min/max candidate sums; source2/3/4 distinct-number sum-set unions;45 minus filled cells and whole cages within a row/column/box; derived residual cages and splitting an enclosing cage. The Killer stock replay remains normal Trivial (box hidden singles only), never ordinary Basic row/column/numeric singles. A fully valid Sudoku solution with an incorrect cage total cannot win.

Generation is independent and deliberately restricted: Jigsaw uses equal-digit boundary swaps preserving connected equal-size regions and the full solution; Killer uses connected small cages with singleton cages merged away before replay. These are real topology/constraint puzzles, not claimed replicas of upstream generation distributions. The pre-existing default Trivial/Basic constructors and their honestly disclosed Basic easier fallback remain unchanged. Map Normal/Easy fallback and accepted Net Solve full locks remain unchanged.

## API and renderer bridge

Existing `SoloConfig` fields and old explicit struct literals remain valid. Additional grades: `SoloDifficulty::{Intermediate,Advanced,Extreme,Unreasonable}`, `MapDifficulty::{Hard,Unreasonable}`. Advanced normal Solo grades are bounded to side9; invalid larger advanced configurations fail validation before allocation.

Additive advanced topology: `PuzzleConfig::SoloVariant(SoloVariantConfig { base: SoloConfig, jigsaw: bool, killer: bool })`. `kind`, `validate`, `help`, source preset catalog and `PuzzleSession::new_config` handle this variant. Killer stock requires regular3×3 boxes, Trivial normal grade, no X and no clue symmetry. Jigsaw uses the base side9/difficulty/diagonal/symmetry.

`SoloScene.regions: Vec<usize>` has one canonical region ID per square, including ordinary rectangular boxes. Compare neighbouring IDs for region boundaries. `SoloScene.cages: Vec<SoloCage>` contains sorted cell indices and `sum: u16`; display the sum at the smallest cell and outline its cells. Existing diagonal flag, fixed givens, conflicts and pencil masks remain available. `SoloCertificate` adds intersection/set/Extreme/cage counters, recursive nodes and `killer_intersection_required`. Killer requires source KINTERSECT specifically: a lower-KSUMS replay must stall. `MapCertificate` adds achieved grade, forcing eliminations and recursive nodes.

The contract was sent to the separate games-options-UI lane early. No renderer/UI source belongs to this handoff. A compatible UI bridge must add exhaustive matches, advanced custom grade choices, real Jigsaw boundaries and cage outlines/sums before these new presets are exposed in the app. All existing new/new_config, options, history/redo, explicit assisted Solve and reset semantics are preserved and checked on successful samples of all15 new presets.

## Bounds and evidence

Finite operation bounds are encoded in source: Solo32 candidates for Intermediate/Advanced/Jigsaw,96 for Extreme/Unreasonable,256 for Killer; solved-grid randomized search30,000 nodes; Jigsaw2,000 possible exchanges per candidate; at most81 symmetry-group removal visits; at most810 monotone deduction iterations; recursive certificate search100,000 nodes per replay. Map retains16 candidates,10,000-node initial colour searches and50,000-node recursive certificate searches. Search exhaustion is never converted into a uniqueness certificate.

Targeted RED/GREEN: the same two availability probes fail behavioral assertions on isolated accepted FIX1 source (3x3 Intermediate and20x15 Hard unavailable), then pass unchanged on isolated proposed source. The retained probe checks all nine Solo and six Map entries. Missing Map variants/rule functions also failed compilation before implementation; callable unimplemented Solo advanced constructor then failed the actual remaining-preset generation assertion. GREEN covers actual exact-grade sample generation, strict lower-grade rejection, independent solution counting, topology and rule fixtures. The independent Solo checker builds candidates directly from scene row/column/region/diagonal/cage constraints without calling production deduction/search. The independent Map checker reconstructs edge adjacency from right/down cell contacts and counts solutions without production search/options. Region and cage connectivity/partition checks are explicit. Jigsaw tests distinguish real irregular peers from obsolete rectangular-box peers. Killer tests check zero given digits, no singleton cages and wrong-sum win rejection.

Final filtered regression:64 passed,0 failed,1 intentionally filtered. This includes all six base cores, accepted Net Solve-lock regressions, stock constructor/Solve smoke, the15 advanced samples, topology/grade/history/reset checks and independent solution counts. The existing broad8-seed-per-preset sampler is intentionally skipped; it was not needed for this amendment. Strict Clippy all targets and format check pass. Pure-component compile checks pass for Linux, aarch64 macOS and x86_64 MSVC Windows. These checks do not imply native GUI execution or packaging.

Nine isolated behavioral mutations are required to fail an assertion: Solo intersection, full set, fish, forcing-chain; Map forcing-chain; Killer sum table and45-residual intersection grade; Jigsaw region peer substitution with rectangular boxes; Killer completed-cage sum omission. `advanced-puzzles-mutation-results.json` and individual RED logs retain results. The immutable proposed source is never edited by those probes; separate mutant copies are used, with jobs2 for every Cargo build.

## Remaining gaps

No original runtime/seed/ID/save compatibility claim. Arbitrary larger advanced Solo sizes/custom Map dimensions, separate hint commands, full upstream generator distributions, source-complete Untangle graph distribution, renderer/UI bridge, native packaging and original-platform parity remain separate work. There is no remaining blocker for the15 source-defined advanced core presets in this amendment. Default/basic behavior remains subject to the accepted FIX1 limitations. Fresh parent review and compatible UI integration are still required.
