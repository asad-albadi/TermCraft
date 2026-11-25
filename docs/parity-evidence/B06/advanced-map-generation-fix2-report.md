# Advanced Map generation FIX2

Private repair against the accepted integrated advanced core; no MAIN, UI or persistence-engine edits. The frozen source snapshot matches every source-file hash from the accepted94-file advanced FIX1 manifest. Pure-component identity is `b06-advanced-fix918` version0.1.918, edition2024. All Cargo invocations use jobs2. Root will perform fresh review and integration; this is owner implementation/test evidence, not an independent review.

## Failure and diagnosis

The root and independent pure-games gates failed the unchanged stock sampler because Default20×15/30-region Hard generated0 successful boards for local seeds0–7. The focused baseline reproduction checks the same ordinary eight seeds for all six advanced Map stock configurations, with the original success criterion; it also exposed0/8 Default30 Unreasonable. Both are actual behavioral failures after valid compilation (`baseline-diagnostic-red.log`, `baseline-timing-red.log`, exit101).

This is candidate selection, not incorrect grading or exhausted initial-colouring search. Among120 fully graded Default30 candidates per requested-grade matrix,108 were Easy and12 were Normal, with no Hard/Unreasonable result. Other candidates were rejected before final grading. The initial generator reused deterministic solution-count search: lowest-index MRV tie and ascending colour order. That biased colour partitions and often left an underused fourth colour. Source `map.c:688–812` instead randomizes tied MRV vertices and available colour order. Global colour-name permutation alone cannot repair that bias.

Pinned authority remains `f306b9db55453f7d7fc824bbbf89ca6ac7d2d273`/historical20150612. `verify-source.py` compares `map.c` bytes with the exact Git object and retains colourer, removal and final-grade slices. No original executable or runtime oracle was used. Upstream copyright/licence remains retained.

## Correction and scope

Advanced Map generation now has a separate bounded randomized MRV colourer, with random vertex ties and colour choices as in the source. Deterministic solution counting remains separate and unchanged. Independent candidate selection removes higher-degree clue anchors first, retaining random order among equal-degree regions, so influential anchors are tested before peripheral clues. This priority is an independently chosen heuristic, not claimed upstream shuffle/distribution compatibility.

During removal, source generation only needs completion at the permitted tier. FIX2 therefore avoids rerunning lower tiers after every accepted removal; the final certificate still computes the easiest achieved grade and rejects easier boards. This reduces redundant work without changing acceptance conditions. Candidate experiments support the choice: randomized colouring alone produced24/48 ordinary successes; prioritizing low-degree clues reduced that to17/48 and was discarded; prioritizing high-degree anchors produced39/48. The held-out8–15 seeds were then checked without retuning this heuristic.

No retry-bound increase, cached/fixed-board fallback, easier substitution or sampler weakening was made. `normal_pairs`, `forcing_chains`, `grade_solve`, `deduce_grade`, `deduce`, graph/options helpers, deterministic `solve` and `search` remain byte-for-byte unchanged; their hashes are retained in `unchanged-solver-and-sampler.json`. The entire existing `tests.rs`, including the formerly failing sampler, is unchanged. Easy/Normal initial-colouring and removal paths remain unchanged. No public scene/config/certificate API changed.

## Ordinary and held-out seed outcomes

Every entry uses the same16-candidate bound. Counts below include all eight seeds, including explicit failures; there are no specially selected successful seed substitutions.

| Stock advanced Map | Baseline seeds0–7 | FIX2 seeds0–7 | FIX2 held-out8–15 |
|---|---:|---:|---:|
|20×15,30 regions,Hard|0/8|5/8|2/8|
|20×15,30 regions,Unreasonable|0/8|6/8|5/8|
|30×25,75 regions,Hard|5/8|8/8|8/8|
|16×18,30 regions,Hard|1/8|5/8|4/8|
|16×18,30 regions,Unreasonable|2/8|8/8|4/8|
|25×30,75 regions,Hard|5/8|7/8|8/8|
|Total|13/48|39/48|31/48|

Residual bounded exhaustion remains:26 of96 tested seed/configuration pairs fail explicitly, including9/16 Default30 Hard. The repair resolves the zero-success ordinary matrices and improves useful output, but does not promise every seed generates a board. Successful fingerprints are distinct. All successes remain exactly requested-grade, unique, connected and marked Randomized; no fallback is hidden in the totals.

## Work limits and synchronous timing

The bound remains16 region candidates. Each advanced initial colour search has10,000 nodes, with at most n clue-removal replays and one final grade request per candidate. A final grade request may run several lower-tier checks internally; those unchanged solvers retain finite5*n monotone deduction passes and50,000-node recursive-search limits. Initial colouring totals at most160,000 nodes; removal replays at most480 for30 regions or1200 for75. Region growth retains the existing4*grid-cells frontier bound. Exhaustion never becomes a certificate.

Test-only counters measure candidates, initial-colouring nodes, removal replays and final grade requests. Across96 pairs the observed maxima were16 candidates,1236 colour-search nodes,1200 removal replays and16 final grade requests. Tests assert the hard bounds for every success and exhaustion; a zero-budget colouring and zero-candidate generation request fail explicitly.

Constructor timings exclude independent audit work. Baseline and candidate release runs use single test threads on the same host; these are measurements, not portable real-time guarantees. Across the original48 pairs, mean constructor time decreases from15.28ms to8.79ms. The worst observed release constructor changes from62.91ms to66.73ms, both75-region cases. Across96 candidate pairs, mean is8.51ms and maximum66.73ms. Debug builds averaged79.69ms, maximum695.07ms for a75-region board. Synchronous UI construction can therefore still pause in debug builds; this patch adds no background-thread/UI lifecycle change and does not expand the operation budget. All per-seed values are retained in `timing-and-work.json`.

## Focused evidence

`map-regression-green.log`:13 Map tests passed,0 failed, including both ordinary and held-out six-configuration matrices, all existing Map grade/rule/default/fallback/geometry checks and the new randomized-colourer budget/diversity test. `debug-focused-green.log`: the three new focused tests also passed unoptimized. No broad parent gate, broad51-preset sampler or Solo15-preset matrix was rerun.

Every successful board in both matrices is audited independently: adjacency is reconstructed from right/down cell contacts; each region is flood-checked for connectivity; given colours and the complete solution are checked; an independent backtracker counts exactly one solution; separate test-only Easy/Normal/Hard deductions replay domains with different colour-bit encoding and data flow. Hard must complete independently while Normal stalls; Unreasonable must make independent Hard stall, then has exact independent solution count one. Certificate recursion/forcing metadata is checked. Distinct successful board fingerprints exclude a fixed-board rescue disguised as generation.

Three isolated mutations fail actual assertions: replacing randomized generation colouring with deterministic counting search fails non-equivalent-partition diversity; dropping the exact final-grade filter fails the independent lower-grade audit; removing the colouring budget guard fails the zero-budget assertion. Mutation results/logs are retained. Strict all-target Clippy, formatting and pure-component macOS aarch64/Windows x86_64 MSVC compile checks pass.

## Handoff and remaining work

The scoped patch is against the frozen accepted integrated source, never changing MAIN. The manifest pins baseline/candidate file hashes and an exact patch roundtrip. Only `map.rs` changes runtime behavior; `PROVENANCE.md` and two pinned-source provenance files document the repair. Scoped evidence is separate under `docs/parity-evidence/B06/advanced-map-generation-fix2-*`.

Advanced Map local seed layouts change. The persistence engine/core identity must pick up the accepted changed bytes before final qualification; this lane does not edit that engine. Root owns fresh review, integration and the eventual unchanged full sampler/gate. Original-runtime/seed-ID/save compatibility, full upstream generator distributions, UI execution and packaging remain outside this repair. Strict native/original parity count stays0.
