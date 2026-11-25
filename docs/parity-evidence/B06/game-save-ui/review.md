# Independent B06 Save/Load UI review

Disposition: CHANGES REQUIRED — one P1 finding. No production edits.

## P1: Overwrite consent is not bound to the destination file

Location: payload/src/ui/games/save.rs:90–113 and :252–260 (especially the exists-only check at :255).

The worker remembers only an `exists: bool` before showing the overwrite question. After consent, it checks only that the path is still a regular file, then unconditionally renames over it. Another producer can replace the destination while the user is considering the visible question. Accepting the question about the old file silently destroys this different replacement file. The current code also reports success and marks the revision clean.

Deterministic actual-control repro is saved in `overwrite-repro.rs`: create an existing file, click actual Save, and in the existing test-only overwrite callback rename the original aside (retaining its inode) and write distinct replacement bytes to the chosen path, then return true. Both operations finish before consent returns. Expected: reject changed target, preserve replacement bytes and dirty active state. Actual: replacement becomes game JSON; active board/history remains unchanged. `overwrite-repro.log` records exit101, exactly1 test failed; it is a desired-behavior regression and intentionally RED.

Required correction: obtain and hold regular-file identity before asking consent; revalidate the same identity, and relevant content-change information, after consent and immediately before publication. Reject changed/replaced/missing/linked targets and preserve existing bytes and dirty state. Add actual-control regressions for replacement and in-place modification during consent, plus unchanged-file acceptance. Unix device/inode and Windows volume/file-index support exist in the already bundled cap-primitives metadata extension interface (re-export usage should be verified by implementer); keep an opened original handle to avoid identifier reuse. This recommendation does not require an unavailable platform-specific atomic compare-and-rename primitive. Explicitly document the residual interval between the last identity check and portable rename; this demonstrated failure happens completely before the final check and is independently fixable.

## Verified package and scope

All four submitted candidate hashes match candidate-maps.json and parent-supplied exact hashes; base files match base-manifest.json. Applying ui.patch to packaged base reconstructs all four candidate bytes exactly. All tested-game-source-hashes.json entries match complete private snapshot, and all three accepted backend hashes match current MAIN. Complete work snapshot includes packaging, FEATURE_PARITY.md, tests, resources and vendor. Private identity changed only to package0.1.921 / normal bin termcraft-save-ui-review921, excluded from payload. Exact candidate files restored after review-only regression run and verified again before normal build.

Static review covered all13 shared actual Save/Load controls, chooser/codec/I/O worker placement, bounded no-follow regular reads, same-directory staging and sync, no-clobber hard-link publish, generation/revision stale results, timed-game progression while chooser runs, failed Load/Save preservation, adoption initial settings/transients/input quarantine, discard guards for Start/New/Reset/Close/Load, ROOT close with dirty/suspended game and no owned terminals, retained detached close, and existing owned-terminal confirmation ordering. No additional actionable UI findings found.

## Independent verification

All commands run from app root with CARGO_TARGET_DIR=<app>/target, --offline --locked and -j2. Every cargo session was awaited/reaped; no same-identity rebuild overlapped a running test.

1. `cargo test --manifest-path target/agent-work/b06-game-save-ui-review/work/Cargo.toml --offline --locked -j 2 --bin termcraft-save-ui-review921 ui::games -- --test-threads=2` — exact submitted candidate, PASS58/0,156 filtered,36.44s; native58.log. Includes native ROOT close and terminal isolation regressions.
2. After completion, appended review-only actual-button regression to private play.rs: `cargo test --manifest-path target/agent-work/b06-game-save-ui-review/work/Cargo.toml --offline --locked -j 2 --bin termcraft-save-ui-review921 review_save_ui_replacement_during_visible_overwrite_consent_is_preserved -- --exact ui::games::play::options_ui_tests::review_save_ui_replacement_during_visible_overwrite_consent_is_preserved --nocapture` — expected RED0/1,214 filtered,1.14s; overwrite-repro.log. Original candidate restored after process exit.
3. `cargo build --manifest-path target/agent-work/b06-game-save-ui-review/work/Cargo.toml --offline --locked -j 2 --bin termcraft-save-ui-review921` — PASS,10.80s,3 preexisting dead-code warnings; normal-build.log.

No full parent/all-target gate, original native format claim, or broad Map source sampler performed. The known original Map sampler baseline remains outside this UI review. Backend codec was used as accepted API context, not re-reviewed.

Normal binary SHA256: 348b36aaaec8487045cfa8e10144b068f9945c5bcf1b62264b5d477fc4174db8; bytes: 611306816.

Exact candidate hashes:

{
  "src/ui/games.rs": {
    "base": "52f57bffea9ed82867533b100a96ee4586cfd2492b992a3446fd9cfe5c2e4c3f",
    "candidate": "cc1d84d3bdad894cbe347d3ee725d4241122a4ec38f2cbee68033e6351a51bac"
  },
  "src/ui/games/play.rs": {
    "base": "9ed8b5ca22798480c98514b50881944e0b60243c997e4bc37eca9f78f0d5f18e",
    "candidate": "10568dc869262e703df24884de8585c2301601c0c61cc63a6a2a0dce1ed7167e"
  },
  "src/ui/games/save.rs": {
    "base": null,
    "candidate": "36083d6f5a5527fe8d2500539c05dc118f18097e5ad084c9efc3c61aec0754ec"
  },
  "src/ui/session_confirmations.rs": {
    "base": "5ffa70053f46c4c21e0f146398ec7a9150b482683314a416210f380b232ed67e",
    "candidate": "68f97410c75bd813b953391c69e649a25c2ae94dd70b7ee009b6ad2ea1e1e067"
  }
}
