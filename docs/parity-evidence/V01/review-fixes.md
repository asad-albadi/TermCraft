# V01 scoped fix review — 2026-11-25

Verdict: both original Important findings ADDRESSED. No new Important issue found in the scoped changes. The current V01 source/evidence checkpoint is ready for ROOT integration and boundary verification. Strict native/original parity completion remains pending.

## Finding 1 — ADDRESSED

`matrix.json` now selects `encrypted_key_agent_authenticates_pinned_openssh_then_expires` in `tests/ssh_interop.rs:174`. Inspected source generates an encrypted identity, imports identical encrypted bytes/fingerprint, authorizes its public identity on the owned OpenSSH daemon, adds it to an owned agent, clears session file-key fallback and selects Owned agent mode. The connection uses the fixture's private known host pin; unexpected credential/trust prompts fail. It requires a successful connection and exactly one daemon authentication, waits until the actual agent reports the identity expired, then requires fresh authentication to fail with the daemon count still one. This closes the endpoint-observation gap. The old local lifecycle test is accurately classified outside the endpoint matrix in supplementary-coverage.json.

Inspected retained behavioral RED: daemon authentication count 0 versus required 1. Inspected GREEN: the exact new endpoint assertion passes, one actual test and zero ignores. The affected aggregate record/log matches the updated executable hash.

## Finding 2 — ADDRESSED

`prepare-dropbear-interop.sh` now verifies the pinned archive/hash/signature before extracting a newly allocated owned .stages directory on every run. Configure and make operate only in that fresh extracted tree. Successful publication atomically replaces the managed destination symlink; arbitrary environment-selected destinations outside checkout target/interop-tools, unmanaged existing directories and symlinks outside owned stages are refused. The previous source/object tree is neither executed nor removed.

The inspected regression seeds an old managed tree with a configure execution marker and tampered source/object files. Retained original RED reports the marker executed; fixed GREEN requires it never execute, preserves the old object, checks the fresh source and built binary, and observes a different published stage. The separate unmanaged-directory regression was independently rerun and passed. Fresh-stage provenance retains the same upstream archive SHA256 and signer fingerprint. Inspected refreshed independent Dropbear runtime log passes its exact shell/SFTP/one-auth assertion against the fresh staged endpoint; external OpenSSH SFTP delegation remains explicit.

## Independently performed checks

- All 150 candidate-owned file hashes and all 150 MAIN-base entries match their manifests; all eight retained test executable copies match.
- The updated patch dry-runs against current MAIN with exit 0, despite unrelated reviewed integrations.
- All final V01/V02 record log hashes and passed-record executable digests match their retained executables. V01 remains 24 passed / 2 unavailable; V02 remains 11 passed / 2 unavailable.
- Re-ran the four Python runner contracts: 4 passed.
- Re-ran FreshSignedStaging.test_unmanaged_destination_is_refused_without_changes: 1 passed.

The full signed-source staging build and endpoint runtime tests were source/log inspected, not independently rerun by this reviewer. No Cargo command, broad suite or host network modification was performed. This report changes no candidate-owned file, implementation count or checklist. The previous review.md describes the superseded first candidate; this scoped report controls disposition of its two findings. V02's prior source/evidence verdict remains unchanged; native Windows/macOS, rendered tab/app close and original Personal runtime observations remain open.
