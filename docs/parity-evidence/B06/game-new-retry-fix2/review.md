# B06 Save/Load UI FIX2 scoped independent re-review

Verdict: native New-game P1 ADDRESSED; updated Map fixtures VALID. No actionable findings in the scoped FIX2 diff.

Review scope: only the new ephemeral attempt cursor, the two updated fixture tests and potential new breakage in those changes. Previous Save/Load/overwrite review and accepted game generators/codecs were not reopened. No MAIN edits and no review subagent.

## Exact input verification

All four base hashes match base-manifest.json; all four candidate payload hashes match base-vs-fix-maps.json. Applying fix2.patch to the copied four-file base reproduces every final payload byte. Only play.rs differs; save.rs, games.rs and session_confirmations.rs remain byte-identical to this frozen current-MAIN base. The complete FIX2 work snapshot was copied into the independent private review work directory, preserving packaging/tests/resources/vendor. All game-dependency-hashes.json entries were verified against the review snapshot, including Map d5b5a94c3f8d371b0d2b5d8271dd9eee1cfe3a51a4585de43f2da952c7a2d10e. No dependency source was edited. Private Cargo package/lock identity is0.1.921 and normal bin termcraft-save-ui-review921. lib.rs, main.rs and play.rs mtimes were explicitly refreshed before Cargo so old copy mtimes could not hide source changes. cursor3.log records actual recompilation of the921 private work crate.

## Code and test assessment

The next-seed cursor advances only inside the confirmed New action with an active session. It advances using wrapping arithmetic before attempting construction, so a bounded failure can retain the active session while the next explicit attempt progresses. A canceled discard prompt never executes that action. Successful start/configured-start and adoption initialize the cursor from the new session seed; reset returns it to the active session's initial seed plus one, and close resets it. No persistent game state, file generation/revision/clean marker or constructor fallback/grade/error policy changes accompany a failed attempt. Existing start_config failure returns after setting its readable message; it does not alter session/history, selection, settings or queued input.

The two Map fixture repairs remove obsolete assumptions about specific successful/failed seeds, with bounded independent core discovery (failure0..16; success0..32). The failure configuration validates first and its real constructor failure is still exercised through the UI adapter, with prior scene/selection/configuration/assistance/error-preservation assertions retained. The rendering case still requires all51 presets, actually constructs and paints all15 specified samples, checks exact configuration and now explicitly requires achieved difficulty equal requested difficulty for both Solo and Map. Discovery cannot skip an unavailable sample: it fails at the fixed bound.

## Independent verification

Every Cargo command used CARGO_TARGET_DIR=<app>/target, --offline --locked -j2 and --bin termcraft-save-ui-review921 against target/agent-work/b06-game-save-ui-review/work/Cargo.toml. All commands ran sequentially; every job was awaited/reaped.

- `cargo test [common flags] fix2_ -- --nocapture --test-threads=2`: PASS3/0,217 filtered,9.15s (cursor3.log). Actual Solo Advanced Rotate2 and Map30Unreasonable independently identify valid seed3 after failed seed2. Actual New/discard/cancel controls verify seed progression, exact expected seed3 saved state, old complete state/history/dirty/generation/revision/settings preservation after failure, and cancel-no-consume. Other cases cover all13 constructor routes, wrap, reset/adopt/configured-start initialization and failed-New queued events/keys/modifiers preservation.
- `cargo test [common flags] advanced_ui_killer_cages_sums_arithmetic_and_history -- --nocapture`: PASS1/0,219 filtered,0.26s (killer-fixture.log).
- `cargo test [common flags] advanced_ui_all_51_presets_available_and_15_samples_paint_exact_grade -- --nocapture`: PASS1/0,219 filtered,5.23s (presets-fixture.log).
- `rustfmt --edition2024 --check <private>/src/ui/games/play.rs`: PASS (format.log).
- `cargo build [common flags]`: PASS8.14s with3 preexisting dead-code warnings (normal-build.log).

Exact fixture command arrays are in fixture-commands.json; raw logs, verified-maps.json, verified-dependency-hashes.json and patch-check.log are retained alongside this report. Source hashes were verified again after all commands completed.

No unchanged64/full parent gate, broad generator sampler, or prior overwrite/core re-review was performed. Parent owns the combined final gate.

Normal binary SHA256: d03716fcfd6b4d4625c5c63bf06dff64a37ca5e739a08f28c237cf3a9db52365; bytes: 611497368.

Final exact maps:

{
  "src/ui/games/play.rs": {
    "base": "8019865e83ea1864e055fa9349356bd85511acc52d30693791b383a5c2954b18",
    "candidate": "c449a915bf89c0aad71d1b8d4091c1252ad1c44a0ac7d476391b01da18d2d105"
  },
  "src/ui/games/save.rs": {
    "base": "a319751542ea82a4993bece94fef92a676cb73abce477bc5d3f8e7b43a34d9af",
    "candidate": "a319751542ea82a4993bece94fef92a676cb73abce477bc5d3f8e7b43a34d9af"
  },
  "src/ui/games.rs": {
    "base": "cc1d84d3bdad894cbe347d3ee725d4241122a4ec38f2cbee68033e6351a51bac",
    "candidate": "cc1d84d3bdad894cbe347d3ee725d4241122a4ec38f2cbee68033e6351a51bac"
  },
  "src/ui/session_confirmations.rs": {
    "base": "7394c8213179d507700236564e50493a1ef7774c1dfb5cc50c0fd12c635918de",
    "candidate": "7394c8213179d507700236564e50493a1ef7774c1dfb5cc50c0fd12c635918de"
  }
}
