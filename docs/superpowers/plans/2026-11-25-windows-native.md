# Windows Native Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox syntax for tracking.

**Goal:** Qualify locally actionable Windows issue #1 workflows and fix defects exposed by native verification.

**Architecture:** Keep the GUI and adjacent RDP engine independently locked. Use existing packaging and runtime probes, with source-bound evidence and isolated fixtures.

**Tech Stack:** Rust, MSVC, PowerShell, Python and existing Windows runtime providers.

**Spec:** [Approved Windows design](../specs/2026-11-25-windows-native-implementation-design.md).

## Global Constraints

- Use Windows x86-64 MSVC, Rust 1.96.1 and locked Cargo dependencies with two build jobs.
- Use disposable workspaces, profiles, listeners and installation fixtures with synthetic data.
- Qualify exact artifacts. Keep disabled providers disabled until their pinned output qualifies.
- Preserve Harbor data and identity-safe handle operations. Keep physical-device and original-product requirements open until exercised.

## Review Focus

- Checkout paths with spaces must work for portable startup and shortcuts (task 2).
- Modified installation files and integration references must cause safe refusal (task 2).
- Missing, occupied or unauthenticated X listeners must fail safely (task 3).
- Cancelled sessions must release owned processes and leave unrelated processes alone (tasks 3–5).
- Corrupt workspace and vault inputs must retain their original bytes (tasks 2 and 5).

### Task 1: Native baseline

- [x] Capture OS, compiler, SDK and Python versions, commit ID and SHA-256 for `Cargo.lock` and `engines/rdp/Cargo.lock`.
- [x] Run `cargo +1.96.1 fmt --all -- --check`, `cargo +1.96.1 test --locked --all-targets --no-fail-fast` and `cargo +1.96.1 test --locked --manifest-path engines/rdp/Cargo.toml`. Record actual counts, failures and ignored tests separately.
- [x] Build both release binaries with `cargo +1.96.1 build --locked --release` and its separate RDP manifest invocation via the package builder. Record hashes and native architecture.
- [ ] For any product defect, first pin its expected behavior in the nearest existing test, observe failure, fix and repeat the affected full suite. Record any toolchain or fixture blockers in `docs/parity-evidence/execution/2026-11-25-windows-native.md`.

### Task 2: Package and installer

- [x] Run `cargo +1.96.1 test --locked --test package_manifest`. Build a candidate with `scripts/package-release.py` and run `scripts/package-check-startup.py` on the ZIP, using the exact built binaries.
- [x] Observe the adjacent Windows RDP child image and digest during packaged loopback negotiation. Exercise two distinct installed GUI revisions against one isolated workspace.
- [x] Reinstall an exact removed version and confirm modified-file and foreign-file refusal. Reopen the preserved synthetic encrypted vault after uninstall.
- [x] Install real per-user protocol registrations for a staged executable, test Windows shell routing and disposable workspace dispatch, and verify the uninstall guard before and after cleanup.
- [x] Stage the pinned BusyBox-w32 archive and exercise its installed shell in native ConPTY.
- [x] Replay portable GUI startup and the per-user installer on a clean Windows runner. Separately install two real development package versions with original manifests and verify shortcuts, modified-file refusal, reinstall and encrypted vault preservation.
- [ ] Replay the upgrade from a previously published version and the interactive fresh-user recovery workflow.
- [ ] Fix confirmed defects in `packaging/windows/install-per-user.ps1`, the package scripts or the owning Rust module with a regression that fails first. Record package and installed executable hashes and the exercised lifecycle.

### Task 3: Private X engine and workflows

- [x] Verify exact Windows source and patch using `packaging/x11/native/prepare-source.py`.
- [x] Build the private VcXsrv release executable and stage adjacent DLLs, font and XKB resources. Confirm a real native candidate binds loopback, refuses occupied displays and invalid cookies, and removes its listener on exit.
- [x] Exercise embedded SSH X11 forwarding and cookie rejection against the locally built private engine using the owned provider test.
- [x] Exercise native Direct and Indirect XDMCP to an isolated peer through MANAGE and cookie-authenticated X setup. Select the peer in the candidate's Win32 chooser for Indirect.
- [x] Verify the owned Windows XDMCP listener on an explicit second loopback address and run authenticated Git for Windows X forwarding.
- [x] Verify a German keyboard map, monitor 1 fullscreen geometry and authenticated software Composite pixel readback on the native candidate. Record live GLX and RANDR versions separately.
- [x] Exchange Unicode text through the native clipboard in both directions on a private Windows window station and retain the alternate-address regression for the owned provider.
- [x] Rebuild the pinned Windows engine on a clean hosted runner and inventory runtime files, fonts, XKB and corresponding source. Verify the runtime and installer with an unpublished exact pin.
- [ ] Exercise owned-job/PID cleanup, occupied-display refusal, cookie checks and readiness, then SSH forwarding, Direct/Indirect XDMCP, clipboard, keyboard and monitor/graphics workflows with appropriate peers. Record any unavailable extensions and dependencies.
- [ ] Update `packaging/x11/native/source-contract.json` only after the exact staged output passes qualification and inventory review.

### Task 4: Interactive terminal and native RDP

- [x] Verify live System, Light and Dark themes, Ctrl+Shift+T, split, detached native fullscreen, restoration, reattachment and shell cleanup in the relocated Windows package at the available scale.
- [x] Pass the native file/serial-control and smartcard-framing component tests and verify unknown-queue handling in the packaged Winspool worker.
- [ ] Run `workspace_layout`, `workspace_windows` and `rdp_options` integration tests as part of the root suite. On the actual desktop exercise ConPTY shells, full-screen program, Unicode/input, splits, detach/reattach, shortcuts, themes and 100/150/200 percent scaling.
- [x] Exchange Unicode clipboard text in both directions through the packaged helper against the independent TLS/NLA endpoint.
- [x] Exercise both mapped drive reads, writable creation, handle-relative rename and read-only refusal in a real xrdp desktop via the independently locked NLA release helper. Verify helper, FUSE and listener cleanup.
- [ ] Execute RDP helper and applicable native component tests. Exercise clipboard, drive handles, admin/fullscreen and device redirection with a controlled RDP endpoint. Record software fixture results and physical-device observations independently.

### Task 5: Browser, services and credentials

- [x] Replay installed Chromium compatibility against independent Windows loopback fixtures after fixing isolated startup and viewport/input handling.
- [x] Stage the exact Chromium ZIP inventory and replay native browser sessions through a private compile-time test pin, including TLS, proxy, Unicode, resize and cleanup.
- [x] Launch the privately pinned package from a relocated GUI and prove exact runtime startup, loopback navigation and profile/snapshot cleanup.
- [x] Build pinned upstream Chromium, record exact x64 installer and runtime hashes, and inventory the versioned resources and notices.
- [x] Reproduce the Chromium network AppContainer's launch failure, then verify the scoped runtime read and execute grants and the owner-only profile ACL.
- [x] Replay the exact source-built Chromium candidate through package, GUI and browser tests. Verify the network AppContainer, restricted renderers and exact registry, process and profile cleanup.
- [x] Exercise add/list/remove with the standalone Pageant process and a synthetic key. Verify ownership and cleanup.
- [x] Authenticate to native primary-display capture with view-only VNC, receive one raw pixel over loopback without recording it, then verify client and listener teardown.
- [x] Run native Npcap against two isolated loopback endpoints. Verify the filter, PCAP output, bounded capture, cancellation and destination preservation.
- [x] Run a real managed Cron job on Windows. Verify restart and clock-jump behavior, disabled jobs and corrupt store detection.
- [x] Verify native Eastern daylight-saving transitions with the timezone isolated to the hosted Windows runner.
- [x] Exercise explicit Windows Iperf2/3 compatibility with independent TCP and UDP clients and verify listener cleanup.
- [x] Use `packaging/browser/stage-runtime.py` and `tests/browser_session.rs` to identify and test the bundled runtime. Run native profile isolation, sandbox, resize/input, cancellation and cleanup checks against a private peer.
- [ ] Exercise locally available services and capture/control, and run `windows_integration`, `ssh_agent` and `session_import` as part of the root suite. Verify Pageant, vault ACLs and recovery, imports, shared definitions, gateways, proxies and reconnects with synthetic data.
- [x] Verify the supplied Personal 26.6 binary and run the isolated reference terminal and SSH bookmark replays. Compare the same synthetic inputs with the relocated TermCraft package and record the limits of the observations.
- [x] Record exact fixture status for physical serial/network/device and original Personal 26.6 comparisons.

### Task 6: Reconcile and verify

- [x] Commit verified checkpoints with evidence and reconcile `PARITY_EXECUTION_QUEUE.json`, `PARITY_CHECKLIST.md`, `FEATURE_PARITY.md`, Windows documentation and [issue #1](https://github.com/asad-albadi/TermCraft/issues/1).
- [x] Re-run the full affected Windows suites and record the verified branch and remaining fixtures and gates in [PR #6](https://github.com/asad-albadi/TermCraft/pull/6).
