# P01 optional packaged Chromium candidate

Implementation progress remains 66.7% (40/60). This private candidate adds no checkpoint and makes no full native/original parity claim.

## Implemented contract

- New Browser sessions choose packaged Chromium explicitly. Installed compatibility remains selectable. Missing serde fields and existing `Session::start(Config)` preserve System Chromium; the Config literal shape is unchanged.
- Compiled resource pin is Chromium 158.0.8127.88-1 (Arch Linux x86_64, 2026-11-24) plus privately supplied signed simdutf 9.3.1-1. The 243 pinned file members are 448,692,683 bytes (about428MiB). No host packages or browser profiles are used.
- Input archives/signatures/notices/complete distribution recipe are checked offline. Recipe PKGBUILD digest matches signed BUILDINFO; upstream exact source archive URL/SHA and distro notices/build inventory are retained. Full upstream multi-gigabyte source is referenced rather than copied.
- Runtime validates manifest byte identity against compiled pin, all resource sizes/SHA256, native executable headers, no symlink path components, and executable privilege bits. Launch copies the same verified bytes into a mode0700 private tree (engine0500/data0400), retains it until child cleanup, and uses adjacent private simdutf. Source path mutation cannot replace the snapshot after verification.
- Normal Chromium namespace/seccomp sandbox, TLS verification, private profile, anonymous CDP pipes and owned process-group cleanup remain. No no-sandbox, ignored certificate errors, debug TCP port, setuid host installation or installed-provider fallback.
- Browser worker checks cancellation before/after every64KiB verification read and between files. Existing Unix Session Drop joins an already-cancelled worker; actual whole-bundle RED blocked11.96s, GREEN returned0.01s. Provider inventory verification runs outside painting; repeated refresh while a probe is pending reuses that probe.
- Windows bundle is explicitly unavailable before a pinned secure helper/payload contract; the existing fixed known-folder helper never accepts an arbitrary bundle engine path. macOS bundle likewise explicitly unavailable. Installed native provider source candidates are preserved.

## Qualification

Durable evidence copies are in browser-bundle/logs beside this report; original qualification paths below are relative to the preserved private tree. Final outcome rows are recorded in handoff/qualification.json. No commands remain running at handoff.

- Signature logs: signature-current.log (Arch fingerprint1DF226B6A2E7FB85B2E6C8F1F8BB3DD85903D982), signature-simdutf.log (C5D2A6E0ED2D11C66B9FA2A306313911057DD5A8).
- Offline real-input package/stage contracts: qualification/packaging-real-inputs-final.log,2passed. Complete inventory, six assets, notices/recipe/BUILDINFO, mutation and no-overwrite publication. Existing release/Iperf regression logs retained.
- Relocated binary and resource directory contain spaces. Every native qualification invocation has PATH empty. Runtime snapshot still resolves all private resources with no installed Chromium dependency.
- Sandbox qualification: qualification/relocated-158-snapshot-sandbox.log,1passed: actual renderer uid_map differs from caller, Seccomp2, NoNewPrivs1, forbidden flags absent, process/profile/runtime tree removed.
- Whole-resource cancellation: qualification/verification-cancel-red.log versus verification-cancel-final.log,1RED then1GREEN using the actual session and verifier progress.
- Source replacement: qualification/snapshot-source-mutation.log,1passed: original engine ELF byte changed after snapshot verification; private snapshot still executes version158; restored source and removed snapshot.
- Earlier resource/manifest/symlink checks: qualification/relocated-158-mutation.log,1passed. Resources restored before subsequent provider runs.
- Provider workflow: qualification/relocated-158-snapshot-provider.log. 3passed/0failed. Local HTTP redirect/RGBA, Unicode form/pointer, history with persisted BFCache,300x200/460x320 responsive paint/click/DPR1, file URL rejection; consent-controlled proxy auth; site-auth cancellation; untrusted local TLS rejected before HTTP.
- GUI workflow: qualification/relocated-158-snapshot-ui-final.log. 4passed/0failed. Actual egui tab start/paint/pointer/text/close, redirect address reconciliation, full input queue release, resize/paint/input revision barrier.
- Native production component source checks: qualification/windows-source-lib-check.log and macos-source-lib-check.log. Compile-only with actual shared runtime/browser/discovery sources and enclosing-service facade. Windows test-target build separately fails on missing host lib.exe while compiling ring dev TLS fixture; no native Windows/macOS execution is claimed.

## Diagnostic failures retained

Initial rendering fixture18s expired because unoptimized SHA verification of the entire bundle took about16s before Chromium startup; the diagnostic60s run showed correct640x480 red frame, input/BFCache/resize and click behavior. The opt-in bundled test fixture budget is60s; installed fixture budgets18/15/5s and all production timeouts remain unchanged. One initial proxy case overlapped a deliberate locale mutation and is invalid qualification; serial reruns are authoritative. One first GUI attempt hit its separate5s first-request fixture deadline before the bundle worker finished hashing; corrected opt-in startup fixture follows the same60s budget. No production browser timeout was relaxed.

## Integration and remaining limits

handoff/browser-bundle.patch is a narrow shared-file patch; handoff/files.json supplies exact MAIN base SHA256, patch SHA256 and six new owned files. Do not copy private Cargo identity or stale unrelated service/UI source. Inputs for a fresh actual release are browser-inputs via --browser-inputs; prior packaged snapshots do not contain this candidate. Fresh independent review, MAIN integration/full gate and a new actual application release qualification remain parent-owned steps.

This is an Arch ABI bundle, not static clean-Linux dependency closure. Host shared libraries/fonts/trust store and unprivileged namespace policy remain declared. Clean OS matrix, Windows/macOS signed native payload/sandbox execution, native TLS trust and full original-target parity remain unqualified. Browser update/signing responsibility and deliberate release pin refresh are described in packaging/browser/README.md.
