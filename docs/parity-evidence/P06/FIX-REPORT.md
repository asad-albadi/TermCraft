# P06 premerge findings repair

Progress: 58.3% (35/60 implementation checkpoints). Private source only; no production source or Cargo identity change is part of this handoff. Parent performs the fresh scoped review.

## Changes

1. Linux verifies copied bytes into a sealable anonymous memfd, seals against writes/growth/shrink/seal changes, and executes that exact image. A real ELF regression hashes the original executable, overwrites its installed inode with a distinct ELF, and requires the original successful response. It also requires a write to the verified image to fail with EPERM.
2. Windows launches the application bootstrap with inheritance disabled into the invocation Job Object. All parent pipe ends remain non-inheritable. The bootstrap alone opens its parent's process for handle duplication, duplicates the three endpoint handles as inheritable within itself, and launches the plugin with an explicit handle list. Parent keeps the source endpoints until the helper exits (then closes them before checking stdout EOF), and holds the write/delete-denying verified-image handle for the whole invocation. The helper waits for the plugin, so the parent deadline/cancellation kills the invocation job. The native-only overlap test keeps an unrelated ordinary piped child alive while requiring plugin EOF and checks parent endpoint inheritance flags.
3. Plugin cleanup calls current P05 collect_file: identity-aware private quarantine with a second digest check, preserving a concurrent replacement instead of deleting its original filename.
4. Plugin collection uses its own cleanup lock, snapshots retirement under registry lock, then releases registry lock for cancellable hashing; completed records are removed from a fresh registry read. It scans the full bounded retirement snapshot, preserving P05's fairness fix. Cancellation defers cleanup; the existing running-plugin removal test now verifies later uncancelled collection after cancellation.

## Shared compatibility

Authoritative baseline is current MAIN after P05 pinned license digests, no-follow regular opening, private identity quarantine, cancellation and full-snapshot fairness, plus B01 services. Exact source hashes: p06-handoff/base-helper-compatibility.json. Apply p06-handoff/shared-after-p05.patch and copy src/plugins, src/ui/plugins.rs and tests/plugins.rs. The patch dry-runs against that MAIN. It adds only plugin hooks to MAIN shared UI/lib/main and exposes/reuses package helpers. The generalized archive API still verifies P05's compiled-in license pin. src/packages/index.rs and packaging/unix-runtime/manifest.json are exactly equal to baseline and require no P06 overwrite. Private Cargo version 0.1.6 is excluded from the patch.

## Verification

- RED: p06-review-red.log — concurrent registry enable fails Busy and the concurrently replaced filename is deleted.
- RED: p06-image-red.log — real ELF mutation changes execution after verification of the held installed inode.
- GREEN: p06-review-green.log — all four corresponding Linux regressions pass.
- Windows component: p06-windows-fixes-check.log — actual packages/plugins modules and native overlap test compile under x86_64-pc-windows-msvc. No native Windows runtime exists here; Windows runtime RED/GREEN remains unrun.
- Formatting: p06-fixes-fmt.log — cargo fmt --check passes.

The first package integration attempt revealed a stale private P05 manifest lacking license_sha256; synchronizing the authoritative pinned manifest resolves this input mismatch. A queued subsequent integration run was stopped before acquiring the build lock to avoid replacing MAIN's unversioned executable during the parent's whole-app gate. Final private integration passed after synchronizing all baseline source and forcing a fresh timestamp-based recompilation. Every omitted common source equals current MAIN (p06-handoff/common-source-compatibility.json); exactly the six files covered by the shared patch differ.

Final GREEN: p06-fixed-targeted.log — seven plugin cases and three actual Portable Unix cases pass, including active production PTY crash isolation. p06-package-helper-green.log — five archive/header/quarantine/cancellation/fairness cases pass. p06-review-green.log — four Linux plugin/image regressions pass, including a registry-level after-verification mutation. That registry-level case was separately shown RED by bypassing only frozen-image selection (p06-registry-image-red.log), then restoring it for GREEN. Final Windows component check and formatting both pass. The build emitted three existing dead-code warning groups in unchanged UI modules.

The private integration wrapper restored its saved shared executable byte-for-byte, but concurrent private writers mean the shared executable is not a qualified MAIN artifact. Parent will force a MAIN rebuild before its next whole-app gate. No broad private UI suite was repeated. Native Windows execution is unrun; request the parent's fresh scoped review before integration.
