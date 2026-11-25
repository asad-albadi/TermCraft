# B06 Save/Load UI FIX1 independent re-review

Verdict: P1 ADDRESSED. No new blocking findings in the scoped fix diff.

Scope is the sole original overwrite-consent identity finding and new breakage introduced by FIX1. The previously clean four-file UI review remains otherwise unchanged. No production edits or own review subagent.

## Evidence

Verified all four FIX1 base hashes against the independently reviewed original candidate map, all four final payload hashes against review-base-vs-fix-maps.json, and patch reconstruction from FIX1/base to exact final payload. games.rs and session_confirmations.rs are unchanged; production changes are confined to save.rs and appended regressions in play.rs. All exact FIX1 hashes are in verified-maps.json.

The worker holds the original no-follow regular-file handle while asking consent, captures device/inode, length, timestamp and bounded full contents, validates after consent and again after staging/sync immediately before publication, and refuses changed targets. Keeping the original handle avoids file-ID reuse. Both initial snapshot and subsequent reads remain bounded by MAX_BYTES+1; oversized originals fail rather than consuming unbounded memory. Read metadata comes from opened files, as required by the portable cap_fs_ext metadata API. Final directory-entry revalidation checks replacement after content reads. Error completion leaves dirty revision and board/history intact; staging cleanup retains existing destination bytes. The no-target no-clobber path is unchanged. The added Unix-specific test import is safely enclosed by the existing cfg(all(test, unix)) module.

Independent exact FIX1 focused run:
`CARGO_TARGET_DIR=<app>/target cargo test --manifest-path target/agent-work/b06-game-save-ui-review/work/Cargo.toml --offline --locked -j 2 --bin termcraft-save-ui-review921 save_ui -- --test-threads=2`
PASS13/0,204 filtered,8.31s (focused13.log). This independently reruns the exact original actual-Save replacement-during-consent reproduction, now GREEN, plus same-inode/same-size/restored-mtime edit preservation, final prepublication recheck and oversized-original rejection. Existing unchanged overwrite consent and decline, generation/revision dirty-state, Load failure, adoption/input quarantine and native close checks also pass.

Independent normal build:
`CARGO_TARGET_DIR=<app>/target cargo build --manifest-path target/agent-work/b06-game-save-ui-review/work/Cargo.toml --offline --locked -j 2 --bin termcraft-save-ui-review921`
PASS4.29s;3 preexisting dead-code warnings (normal-build.log).

Independent Windows metadata API typecheck PASS using bundled x86_64-pc-windows-msvc dependency artifacts. Verified the typechecked unchanged() body is byte-for-byte the production helper. Command and exit0 recorded in windows-metadata-api.log. This is API/type evidence only, with no Windows runtime claim.

## Limits

The fix accurately documents the residual interval between the last identity/content validation and portable rename; it does not claim atomic compare-and-swap. Deterministic consent-window replacement and in-place changes that complete before validation are rejected. No unsupported conditional-publication primitive is required for this finding's acceptance. No unchanged58 rerun, parent/all-target gate, original-format compatibility claim, or broad Map sampler was performed. Prior independent native58 evidence remains in the parent review directory. All own cargo sessions were sequential, awaited and reaped; no same-identity test/build overlap. Private version0.1.921 and bin termcraft-save-ui-review921 were reused only after original jobs completed.

Normal FIX1 binary SHA256: f4e247341c601db742acd2bca16bb669eea5b5f5960f930c0567f6ec8913dc93; bytes: 611366752.

Final reviewed hashes:

{
  "src/ui/games.rs": {
    "review_base": "cc1d84d3bdad894cbe347d3ee725d4241122a4ec38f2cbee68033e6351a51bac",
    "fix": "cc1d84d3bdad894cbe347d3ee725d4241122a4ec38f2cbee68033e6351a51bac"
  },
  "src/ui/games/play.rs": {
    "review_base": "10568dc869262e703df24884de8585c2301601c0c61cc63a6a2a0dce1ed7167e",
    "fix": "20d6dc46ecde31810f4b1fd81f7d5aaa9ff807463cef9e47eb57df5e041add24"
  },
  "src/ui/games/save.rs": {
    "review_base": "36083d6f5a5527fe8d2500539c05dc118f18097e5ad084c9efc3c61aec0754ec",
    "fix": "a319751542ea82a4993bece94fef92a676cb73abce477bc5d3f8e7b43a34d9af"
  },
  "src/ui/session_confirmations.rs": {
    "review_base": "68f97410c75bd813b953391c69e649a25c2ae94dd70b7ee009b6ad2ea1e1e067",
    "fix": "68f97410c75bd813b953391c69e649a25c2ae94dd70b7ee009b6ad2ea1e1e067"
  }
}
