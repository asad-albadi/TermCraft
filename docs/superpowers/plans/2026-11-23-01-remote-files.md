# Remote files and embedded SSH/SFTP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver checklist outcomes F01, F02, F03, F04, F05, F06 with the user-visible behavior and evidence stated below.

**Architecture:** Run an asynchronous embedded SSH engine outside egui, with separate shell/SFTP channels under one authenticated connection. The approved implementation uses `russh` plus `russh-sftp`: its async channels and X11 request hooks address limitations found in the initial ssh2 candidate. The full-plan approval includes this library choice. Preserve explicit compatibility mode for advanced external OpenSSH configurations.

**Tech Stack:** Rust 2024, eframe/egui, existing serde JSON persistence, subsystem libraries/providers explicitly selected and pinned by their introducing task.

**Spec:** [Authoritative checklist](../../../PARITY_CHECKLIST.md), [project design](../../../DESIGN.md), [approved F01 design](../specs/2026-11-23-remote-files-design.md). F01 has a separately approved design; remaining subsystem scope was approved through the complete master plan.

**Shared interfaces and fixtures:** [Parity contracts](2026-11-23-parity-contracts.md). New APIs named here are planned, not existing code.

## Global Constraints

- Deliverables stay inside `app/`; Rust edition 2024 with eframe/egui remains the application stack.
- Preserve old workspace JSON, penguin branding, emerald light/dark palettes and correct System-theme behavior.
- Treat installed-client adapters as partial until the required integrated behavior is implemented and verified.
- Credentials stay out of ordinary session JSON, logs and exports; never disable host/certificate checks to make a test pass.
- Network/protocol/file work stays off the egui render thread; cancellation and bounded queues are required.
- Keep full Personal 26.6 scope; Professional-only and uncertain requirements need evidence before inclusion/exclusion.
- No checklist outcome is completed by a plan, screenshot, cross-target compilation, or unavailable native/hardware test.

## Review Focus

- Host-key/certificate changes and credential responses must not authenticate the wrong endpoint or leak secrets (F01/F06/S05/D01).
- Remote/local filenames, archives and redirects must not become commands or escape their intended target root (F02/F05/P05/P06/B02).
- Disconnects, full queues and stale generations must preserve user data and cancel without freezing the interface (F01/F04/W02).
- Packaged/native workflows must use declared dependencies and preserve user settings across install/upgrade/remove (P08/B04/B05/V05/V08).
- Uncertain original behavior and absent hardware/machines must remain evidence gaps, never silently excluded scope or passing tests (E01-E05/V01-V08).

## Task F01: Automatic graphical remote directory pane

**Files:** Create `src/ssh/{mod,auth,host_keys,worker,forward}.rs`, `src/remote_files/{mod,path,state}.rs`, `src/ui/remote_files.rs`, `tests/ssh_browser.rs`, `tests/support/{ssh_server,connection_driver,gui}.rs`; modify `src/{lib,model,ui,connection}.rs`, `src/ui/pages.rs`, `Cargo.toml`, `Cargo.lock`.

**Dependencies:** Approved F01 design plus the shared contracts. F01 owns minimum jump/X11 option preservation; X01/X02 later add bundled-engine lifecycle rather than being a hidden prerequisite.

**Interfaces / deliverable:** Introduce `ConnectionConfig`, `ConnectionHandle`, `ConnectionCommand`, `ConnectionEvent`, `RemotePath`, `RemoteEntry`, and `DirectorySnapshot` exactly as defined in the shared contracts. `remote_files::BrowserState::apply(event: &ConnectionEvent) -> bool` rejects stale generations/requests. Add a serde-defaulted `SshEngine::{Embedded,OpenSsh}` preference; migrate existing sessions explicitly, preserving compatibility mode.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``embedded_shell_and_browser_share_one_authentication`, `changed_key_prevents_authentication`, `stalled_listing_keeps_shell_and_cancel_responsive`, `old_connection_cannot_replace_new_listing`, `automatic_pane_survives_collapsed_session_sidebar``. Required observations: With a real isolated OpenSSH fixture, accepted authentication count is 1; the shell returns `SSH_OK`; home listing contains a known fixture file. A rotated host key yields zero additional accepted authentications. Delayed SFTP does not prevent a shell response or cancellation within 2 seconds. A result from connection 41 cannot update connection 42. GUI tests assert the pane exists before data arrives, then shows entries; closing tabs releases both channels. Also test cancel during host/key/password/MFA prompts, full queues, denied listing, SFTP-disabled server, and old JSON migration.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(server.accepted_authentications(), 1);
assert!(shell_output.contains("SSH_OK"));
assert!(listing.entries.iter().any(|e| e.display_name == "fixture.txt"));
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test ssh_browser -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Build host trust and cancellable redacted prompts first, then the async transport and directory state, then the per-tab pane. Use `tokio::select!` across cancellation, controls, shell data and a separately polled listing future; never await a listing inside the terminal-input branch. Request a PTY, propagate resize/startup command/compression, and support the existing single jump host through an SSH direct-tcpip stream. Forward trusted X11 requests only to the explicitly configured local display with authorization; absence of a usable display is an actionable error, not a dropped option. Unknown hosts require trust; changed keys stop before auth. Read user known-hosts without rewriting them and write app trust under the selected data directory. Keep shell usable when SFTP fails. Render independent resizable/hideable remote pane in both palettes.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test ssh_browser -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task F02: Graphical transfers and remote file management

**Files:** Create `src/remote_files/{transfer,operations,drop_targets}.rs`, `tests/remote_transfers.rs`; modify `src/ui/remote_files.rs`.

**Dependencies:** F01

**Interfaces / deliverable:** `TransferManager::submit(op: TransferOperation) -> Result<JobId, TaskError>`, `cancel(id: JobId)`, `poll() -> Vec<TransferEvent>`; operations are Upload, Download, Mkdir, Rename, Remove and carry typed local/remote paths. `ConflictPolicy::{Ask,Skip,Overwrite}` is explicit; default Ask. `TransferEvent` contains bytes completed, outcome, and recoverable temporary path.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``recursive_transfer_preserves_bytes_and_names`, `cancelled_download_does_not_overwrite_destination`, `symlink_cycle_is_not_followed`, `remote_delete_requires_confirmed_target``. Required observations: Round-trip a tree with spaces, newline, Unicode, empty/binary files and a symlink; compare file bytes and relative names. Cancellation preserves pre-existing `keep` bytes. Inject disconnect, disk-full and permission errors; the job reports the actual failure and retains recoverable partial data. A drop aimed at the local side cannot accidentally invoke a remote delete.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(downloaded_bytes, uploaded_bytes);
assert_eq!(std::fs::read(&existing_destination).unwrap(), b"keep");
assert_eq!(outside_root_file_count, 0);
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test remote_transfers -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Add file/folder pickers, drag/drop targets, progress/cancel UI, and remote mkdir/rename/delete confirmation. Traverse recursively without following symlink cycles. Stage downloads beside destination and replace only after success and explicit overwrite; do not truncate an existing file on failed/cancelled download. Use protocol operations, never shell-joined filenames.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test remote_transfers -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

**Implementation checkpoint:** Steps above are verified on the Linux fixture/GUI/fault suites. Windows filesystem code cross-checks; native-platform and original-runtime acceptance remains pending, so the checklist outcome is still Partial. See PARITY_PROGRESS.md.

## Task F03: Follow the remote shell directory

**Files:** Create `src/ssh/shell_integration.rs`, `tests/shell_directory.rs`; modify `src/terminal.rs`, `src/remote_files/state.rs`, `src/ui/remote_files.rs`.

**Dependencies:** F01; F02 navigation state

**Interfaces / deliverable:** `ShellDirectoryTracker::feed(bytes: &[u8]) -> Vec<RemotePath>` parses bounded OSC 7 messages incrementally. `BrowserFollowMode::{Manual,FollowShell}` and `BrowserState::on_shell_directory(connection: ConnectionId, path: RemotePath) -> Option<ConnectionCommand>` guard origin and active connection.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``split_osc7_updates_only_matching_connection`, `manual_mode_ignores_shell_directory`, `unsupported_shell_keeps_manual_browser``. Required observations: Feeding split `OSC 7;file://host/home/a%20b` yields `/home/a b` once. A notification from a different connection does not navigate. Manual mode yields no list command; unsupported integration does not inject a startup command or pretend synchronization.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(notification.as_bytes(), b"/home/a b");
assert!(manual_mode_command.is_none());
assert!(other_connection_command.is_none());
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test shell_directory -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Offer reversible shell integration for supported shells without silently editing remote dotfiles. Accept directory notifications only from the associated SSH channel; decode URI paths safely. Follow only when enabled. For unsupported shells or absent notifications, show manual navigation and a reason; preserve the last successful path.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test shell_directory -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

**Checkpoint:** Linux implementation and one independent-review fix pass verified. Native GUI/platform and original-runtime evidence remain pending; F03 is Partial. Details and rulings: `PARITY_PROGRESS.md`.

## Task F04: Edit remote files and upload saved changes

**Files:** Create `src/remote_files/editor.rs`, `src/ui/remote_editor.rs`, `tests/remote_editor.rs`; modify `src/ui.rs`, `src/ui/remote_files.rs`.

**Dependencies:** F01, F02

**Interfaces / deliverable:** `RemoteDocument::open(connection: ConnectionId, path: RemotePath) -> Result<Self, TaskError>`, `save(&mut self, bytes: &[u8], policy: SavePolicy) -> Result<SaveOutcome, TaskError>`; `SaveOutcome::{Saved,Conflict,Disconnected}`; document retains original remote metadata/hash and local recovery path.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``remote_save_conflict_preserves_both_versions`, `failed_save_keeps_recoverable_edits`, `close_cancel_retains_dirty_document``. Required observations: Remote `v1` changed independently to `v2` produces Conflict when local `mine` is saved; neither version is overwritten. Disconnect leaves local `mine` readable from recovery path. Cancelled close retains the dirty document and opening endpoint.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert!(matches!(save_outcome, SaveOutcome::Conflict));
assert_eq!(remote_bytes, b"v2");
assert_eq!(std::fs::read(recovery_path).unwrap(), b"mine");
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test remote_editor -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Double-click text files to download into an owned recovery workspace and open an editor. Save stages upload and atomic rename when supported. Detect remote changes before replacement; require a conflict decision. Closing with unsaved changes prompts Save/Discard/Cancel. Reconnect requires matching endpoint and fresh host verification; never upload to a different session.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test remote_editor -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

**Checkpoint:** Linux implementation and one independent-review fix pass verified. Native/original and documented interoperability evidence remain pending; F04 is Partial. Details and rulings: `PARITY_PROGRESS.md`.

## Task F05: SCP browser compatibility

**Files:** Create `src/remote_files/{scp,capabilities}.rs`, `tests/scp_browser.rs`; modify `src/ui/remote_files.rs`, `src/model.rs`.

**Dependencies:** F01, F02; E04/V07 original-profile evidence is a preflight requirement

**Interfaces / deliverable:** `RemoteBackendKind::{Sftp,Scp}` and `RemoteCapabilities` identify browsing/transfer/edit support. `ScpBackend::list(path: &RemotePath) -> Result<DirectorySnapshot, TaskError>` and `copy(op: TransferOperation) -> Result<SaveOutcome, TaskError>` implement the common remote-files contract.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``scp_names_cannot_be_interpreted_as_commands`, `sftp_disabled_endpoint_supports_explicit_scp`, `unsupported_enumerator_reports_capability_error``. Required observations: An SFTP-disabled server transfers a file in explicit SCP mode and correctly lists a supported directory helper response. A filename containing quotes, newline and `$()` remains literal and creates no command side effect. Unsupported remote environments cannot produce a fake empty listing.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(downloaded_bytes, b"SCP_OK");
assert!(!command_side_effect.exists());
assert!(unsupported_enumerator_result.is_err());
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test scp_browser -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Keep SFTP default. Offer explicit SCP mode only after verifying the original profiles in E04/V07. SCP itself supplies transfer, not enumeration: implement a length-delimited remote directory helper with documented supported remote environments; never parse `ls` output. Reject unsupported enumeration environments with an accurate message. Label transport in the pane; do not silently downgrade.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test scp_browser -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

**Checkpoint:** Implementation verified on Linux: 148 tests passed, 3 opt-in tests ignored; 24 SCP endpoint cases. Windows production staging/source/codec modules and native regression sources pass isolated target compilation, but native tests are unrun. Native/original/other-endpoint validation and stated Windows symlink/ACL limitations remain pending. F05 is Partial, not fully verified. See `PARITY_PROGRESS.md`.

## Task F06: Authentication reuse and reconnect coordination

**Files:** Create `src/ssh/lifecycle.rs`, `tests/ssh_lifecycle.rs`; modify `src/ssh/auth.rs`, `src/ui.rs`, `src/remote_files/state.rs`.

**Dependencies:** F01; F02/F04 pending actions

**Interfaces / deliverable:** `ConnectionLifecycle::reconnect(config: ConnectionConfig) -> ConnectionId`, `cancel_challenge(id: u64)`, and `attach_files(id: ConnectionId) -> Result<(), TaskError>` own shell/browser lifetime; no saved password field is introduced.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``browser_retry_does_not_reauthenticate`, `reconnect_discards_old_challenges_and_listings`, `cancel_auth_stops_the_entire_attempt``. Required observations: Retrying denied SFTP after server recovery keeps accepted auth count at 1. Reconnecting increments connection ID and rejects previous prompt IDs/results. Cancel during encrypted-key, password or MFA challenge prevents any subsequent prompt or credential submission.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(server.accepted_authentications(), 1);
assert_ne!(old_connection_id, new_connection_id);
assert!(!old_challenge_was_accepted);
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test ssh_lifecycle -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Exercise and finish coordinated prompts, per-hop authentication and retries after F01 foundations. Reconnect replaces the generation, wipes transient credentials and invalidates outstanding file actions. Browser retry reopens SFTP on the authenticated connection; it does not start another SSH login. Cancellation ends the attempt without launching another auth method.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test ssh_lifecycle -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

**Checkpoint:** 162 tests passed, 3 opt-in tests ignored; 14 lifecycle cases. Five Important review findings fixed in one RED→GREEN pass; no re-review. Native/original validation and initial-denial retry evidence remain pending. F06 remains Partial; detailed rulings/minors are in `PARITY_PROGRESS.md`.

## Handoff

The user approved the complete 60-point plan on 2026-11-23 and selected inline execution. Execute this subsystem under that approval; record routine implementation decisions and unresolved evidence in PARITY_PROGRESS.md. Keep external validation gaps explicit.
