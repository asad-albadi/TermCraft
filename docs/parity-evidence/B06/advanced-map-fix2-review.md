# B06 Advanced Map FIX2 independent review

Verdict: ACCEPTED for the narrow bounded-generation repair. No blocking findings.

Reviewed against `docs/parity-evidence/B06/advanced-map-generation-repair-brief.md` and the actual parentgate903/1 defect. This is a source/behavior review of FIX2, not a rereview of the previous core, UI, persistence codec, or a native/original parity claim. No MAIN source was modified. All builds used private pure-component `b06-advanced-map-review922` version `0.1.922`, offline/locked, jobs2.

## Artifact identity and scope

- Archive SHA256: `d7b254514bd492f3338d02da402561cbfae9f9a7bcb978105773b96d42f928b2`.
- Patch SHA256: `442452d2505a0d2a7f1cff2cec795c5538b7a1fc43d45934d0db506f994a2ac8`.
- Baseline map.rs: `48021e0fb77ad2e797e305242dd167751752a5559a39097f253571be240aea5c`.
- Candidate map.rs: `d5b5a94c3f8d371b0d2b5d8271dd9eee1cfe3a51a4585de43f2da952c7a2d10e`.

Independently checked every archive member against the manifest, including absence of extra members, and checked all manifest baseline files against current root. Applied the scoped patch to a fresh private baseline and matched every resulting proposed file hash. The accepted Save backend does not affect this Map identity check. `integrity.log`, `patch-roundtrip.log`, and `map-only.diff` retain evidence.

The only production behavior change is map.rs. Nine functions (`normal_pairs`, `forcing_chains`, `grade_solve`, `deduce_grade`, `deduce`, `graph`, `options`, `solve`, `search`) are byte-for-byte unchanged; hashes were derived independently in `independent-solver-hashes.json`. Entire root broad-sampler tests.rs remains unchanged. Pinned map.c Git object and all retained provenance slices were independently verified (`source-authority.log`). Randomized tied-MRV vertex and available-color choices agree with the retained source algorithm. Degree-descending clue removal is correctly disclosed as a local heuristic.

## Acceptance and work-limit review

Advanced candidates use randomized initial coloring within the same 10,000-node search budget and 16-candidate generator bound. Stable sorting retains shuffled order among equal-degree anchors. Changing removal probes from grade_solve(...).is_some() to deduce_grade(...).is_some() removes lower-tier classification work only: grade_solve previously called that same deduce_grade first and lower-grade checks cannot change Some to None. Final grade_solve, exact achieved-grade equality, answer equality and the original unique-search exhaustion rejection remain intact. Hard/Unreasonable exhaustion still returns the explicit generation error; no easier or fixed-board fallback was added. Easy/Normal behavior is unchanged.

Randomized recursion terminates at its node budget, restores unsuccessful assignments while unwinding, and does not certify exhausted coloring. Candidate/removal/final-grade loops remain bounded. Fresh focused tests assert every observed generation's 16 candidates, 160,000 coloring nodes, at most 16*regions removal replays, and 16 final certification requests, including failures, plus zero-budget and zero-candidate behavior.

## Fresh behavior evidence

`map-tests.log`: 13/13 Map tests passed in release, including all preexisting Map regressions and the three new focused tests. Ordinary seeds0..7 yielded 39/48 boards; held-out8..15 yielded31/48, exactly reproducing owner counts. All70 successes were audited for connected regions, reconstructed contact adjacency, valid clues/solution, independent solution count1, and independent lower-grade stalling; Hard also independently completes at Hard and Unreasonable requires unique recursion. Successful fingerprints are distinct. The 26/96 explicit failures remain counted.

Reviewer-authored public API integration tests in `work/tests/reviewer.rs` use a separate adjacency reconstruction, cell flood traversal and MRV backtracker. The baseline run genuinely fails exit101 with Default30 Hard0/8 and Unreasonable0/8, retaining results for every configuration (13/48 total), in `baseline-red.log`. The same test passes with FIX2 (39/48). Additional seeds32..39 across all six configurations yield33/48, all audited valid and unique, with15 explicit failures; every configuration has successful output. See `public-path-green.log`. These checks use PuzzleSession::new_config, the original sampler's construction path; inspection confirms it passes the same untransformed seed into Map generation.

`debug-focused.log`: all three new focused tests passed unoptimized. `fmt.log`: cargo fmt --check passed. `clippy.log`: strict library Clippy passed. `windows-check.log` and `macos-check.log`: pure-component x86_64-pc-windows-msvc and aarch64-apple-darwin checks passed. Owner mutation assertions were inspected but not rerun in this review.

Commands, from app root (all Cargo commands include --offline --locked -j 2):

```
cargo test --release --offline --locked -j 2 --manifest-path target/agent-work/b06-advanced-map-fix2-review/work/Cargo.toml map:: -- --test-threads=1 --nocapture
cargo test --release --offline --locked -j 2 --manifest-path target/agent-work/b06-advanced-map-fix2-review/baseline/Cargo.toml --test reviewer public_path_original_seeds -- --nocapture --test-threads=1
cargo test --release --offline --locked -j 2 --manifest-path target/agent-work/b06-advanced-map-fix2-review/work/Cargo.toml --test reviewer -- --nocapture --test-threads=1
cargo test --offline --locked -j 2 --manifest-path target/agent-work/b06-advanced-map-fix2-review/work/Cargo.toml fix2_generation_tests -- --test-threads=1 --nocapture
cargo fmt --manifest-path target/agent-work/b06-advanced-map-fix2-review/work/Cargo.toml --check
cargo clippy --offline --locked -j 2 --manifest-path target/agent-work/b06-advanced-map-fix2-review/work/Cargo.toml --lib -- -D warnings
cargo check --offline --locked -j 2 --manifest-path target/agent-work/b06-advanced-map-fix2-review/work/Cargo.toml --target x86_64-pc-windows-msvc
cargo check --offline --locked -j 2 --manifest-path target/agent-work/b06-advanced-map-fix2-review/work/Cargo.toml --target aarch64-apple-darwin
```

## Limits and integration consequence

This fixes the ordinary zero-success defect with a source-grounded coloring correction and a disclosed removal heuristic, without weakening grades or hiding failures. It does not guarantee every seed succeeds. Default30 Hard remains7/16 successful in the two requested matrices, and debug synchronous construction remains a documented latency limitation. Neither limitation is concealed, and the fixed operation budget is retained. No full51-preset sampler or broad parent gate was rerun here. Root must integrate the accepted exact bytes, refresh persistence core identity and complete final integrated qualification. All review-owned jobs have finished; no background review Cargo process remains.
