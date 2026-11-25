# Session migration, automation and credentials Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver checklist outcomes S01, S02, S03, S04, S05, S06, S07 with the user-visible behavior and evidence stated below.

**Architecture:** Keep portable connection definitions separate from secrets, external shared sources and runtime handles. Add import reports instead of silently approximating settings. Vault and agent operations are explicit user actions.

**Tech Stack:** Rust 2024, eframe/egui, existing serde JSON persistence, subsystem libraries/providers explicitly selected and pinned by their introducing task.

**Spec:** [Authoritative checklist](../../../PARITY_CHECKLIST.md), [project design](../../../DESIGN.md), [approved F01 design](../specs/2026-11-23-remote-files-design.md). Only F01 has a separately approved subsystem design; the other sections are proposed scope/architecture for master-plan review.

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

## Task S01: MobaXterm and PuTTY import

**Files:** Create `src/sessions/{mod,import_moba,import_putty,report}.rs`, `src/ui/session_import.rs`, `tests/session_import.rs`, `tests/fixtures/imports/`; modify `src/model.rs`, `src/ui.rs`.

**Dependencies:** None beyond shared contracts

**Interfaces / deliverable:** `import_moba(bytes: &[u8]) -> Result<ImportReport, String>`, `import_putty(bytes: &[u8]) -> Result<ImportReport, String>`; `ImportReport` contains candidate Session values, unsupported fields and source locations. `apply(report, policy: DuplicatePolicy)` runs only after preview.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``moba_import_preserves_host_port_key_and_folder`, `unsupported_putty_option_is_reported`, `malformed_import_does_not_mutate_store``. Required observations: A fixture with port 2222, a key path containing spaces and a nested folder produces those exact values. Unknown settings appear in warnings; malformed input changes zero sessions. Test duplicate name, mixed encoding and malicious oversized input.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(imported.port, 2222);
assert_eq!(imported.key, "/keys/with spaces");
assert_eq!(store_after_failed_import, store_before_import);
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test session_import -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Obtain version-labeled export fixtures, parse without executing anything, map supported fields, preserve unknown fields in the report and offer duplicate policies. Do not import stored credentials as plain text or claim all formats supported. Failed imports leave the store unchanged.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test session_import -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task S02: Shared session sources

**Files:** Create `src/sessions/shared.rs`, `src/ui/shared_sessions.rs`, `tests/shared_sessions.rs`; modify `src/model.rs`, `src/ui.rs`.

**Dependencies:** S01, F01

**Interfaces / deliverable:** `SharedSource::refresh() -> Result<SharedSnapshot, String>` supports selected shared-file/HTTPS/SFTP sources; snapshots store source identity/revision and are read-only in the session tree. `copy_to_local(id)` creates an independent Session.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``shared_refresh_never_overwrites_local_session`, `offline_source_keeps_labeled_cached_snapshot``. Required observations: A shared session renamed upstream updates its shared node but leaves a local same-name session unchanged. Failed refresh retains cached entries with stale status. Unsupported transport is rejected in source setup.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(local_session_after_refresh, original_local_session);
assert!(cached_snapshot_is_labeled_stale);
assert!(!shared_entry_is_editable_in_place);
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test shared_sessions -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Add source configuration, refresh status, credential references and offline cached view. Keep remote IDs separate from local IDs and prevent remote definitions from silently overwriting local sessions. Handle unavailable sources and changes without losing user edits.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test shared_sessions -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task S03: Session groups and launch shortcuts

**Files:** Create `src/sessions/groups.rs`, `src/platform/shortcuts.rs`, `tests/session_groups.rs`; modify `src/ui.rs`, `src/model.rs`.

**Dependencies:** S01; S04 supplies command dispatch; implement group resolution first, finalize shortcuts after S04

**Interfaces / deliverable:** `SessionGroup::resolve(folder: &str, store: &Store) -> Vec<Session>` uses stable IDs; `ShortcutSpec::for_session(id: SessionId) -> Result<Self, String>` generates platform-specific argument-safe launch entries.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``group_launch_reports_per_session_outcomes`, `shortcut_preserves_space_and_quote_arguments``. Required observations: A folder containing two valid and one invalid session opens the two and reports the failure. Shortcut expansion preserves a path with spaces as one argument and cannot execute injected shell text.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(successfully_opened_session_count, 2);
assert_eq!(failed_session_count, 1);
assert_eq!(shortcut_argument_count_for_cwd, 1);
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test session_groups -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Add launch-folder context action, per-session/group shortcuts and visible launch results. Mixed invalid/unavailable sessions do not prevent valid peers launching. Quote filesystem paths using platform formats, never shell-concatenate endpoints.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test session_groups -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task S04: CLI commands and existing-instance dispatch

**Files:** Create `src/cli.rs`, `src/ipc.rs`, `tests/cli_dispatch.rs`; modify `src/main.rs`, `src/ui.rs`.

**Dependencies:** S01; shares group IDs from S03

**Interfaces / deliverable:** `Cli::parse(args: impl IntoIterator<Item=String>) -> Result<CliAction, String>`; `CliAction::{OpenSession,OpenGroup,RunCommand,RunMacro,OpenFolder,NewTab,Screenshot,Help}`. `InstanceBroker::send(action: CliAction) -> Result<DispatchReceipt, String>` uses same-user local IPC.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``cli_missing_value_returns_error_not_panic`, `same_workspace_dispatches_exactly_once`, `different_data_dirs_do_not_share_commands``. Required observations: `--screenshot` without a value is an error. A forwarded NewTab is opened once and acknowledged; malformed/oversized IPC is rejected. An instance using workspace A cannot receive session actions intended for workspace B.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert!(Cli::parse(["--screenshot".to_owned()]).is_err());
assert_eq!(new_tabs_opened_by_forwarded_action, 1);
assert_eq!(commands_received_by_other_workspace, 0);
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test cli_dispatch -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Preserve screenshot/data-dir/help flags and add stable saved-session IDs, group, macro, command, cwd and exit-when-done commands. Hand off only matching workspace actions to the existing instance; isolate different data directories. Validate IPC size/ownership and distinguish accepted from completed operations.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test cli_dispatch -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task S05: Opt-in encrypted credential vault

**Files:** Create `src/credentials/{mod,vault,format}.rs`, `src/ui/credentials.rs`, `tests/credential_vault.rs`; modify `src/model.rs`, `src/ssh/auth.rs`.

**Dependencies:** F01, F06

**Interfaces / deliverable:** `Vault::create(path: &Path, master: Secret) -> Result<Self, String>`, `unlock(path, master)`, `put(id: CredentialId, secret: Secret)`, `get(id) -> Result<Secret, String>`, `lock()`, `delete(id)`; sessions serialize opaque credential references only. Exact remaining signatures: `unlock(path: &Path, master: Secret) -> Result<Self, String>`, `put(&mut self, id: CredentialId, secret: Secret) -> Result<(), String>`, `get(&self, id: &CredentialId) -> Result<Secret, String>`, `lock(&mut self)`, `delete(&mut self, id: &CredentialId) -> Result<(), String>`.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``wrong_master_and_tamper_cannot_decrypt`, `session_export_contains_reference_not_password`, `failed_master_change_keeps_old_vault_usable``. Required observations: A sentinel credential is absent from vault bytes, workspace JSON and logs; wrong password and modified ciphertext fail without exposing partial plaintext. Interrupted rekey retains the old valid vault; locking invalidates access. Native file permissions are tested per OS.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert!(!vault_bytes.windows(b"SENTINEL_SECRET".len()).any(|w| w == b"SENTINEL_SECRET"));
assert!(wrong_password_unlock.is_err());
assert!(tampered_vault_unlock.is_err());
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test credential_vault -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Use a maintained authenticated-encryption/KDF implementation (Argon2id plus XChaCha20-Poly1305 candidate), random salt/nonce and versioned atomic vault format. Add optional save prompts, unlock/lock, change master password and delete. No secret or master password goes to ordinary JSON, logs or exports. Preserve existing no-save default.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test credential_vault -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task S06: SSH key and agent management

**Files:** Create `src/credentials/{keys,agent}.rs`, `src/ui/ssh_keys.rs`, `tests/ssh_agent.rs`; modify `src/ssh/auth.rs`, `src/ui/pages.rs`.

**Dependencies:** F01, S05

**Interfaces / deliverable:** `AgentManager::{list,add,remove,start_owned,stop_owned}` return typed identity/lifecycle results. `KeyManager::generate(destination: &Path, settings: KeySettings) -> Result<KeyPairPaths, String>` refuses unintended overwrite.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``removing_identity_updates_authentication`, `stop_owned_never_kills_system_agent`, `key_generation_never_overwrites_without_confirmation``. Required observations: An added fixture identity authenticates, removal makes it unavailable. Closing the app leaves a pre-existing system agent running. Existing key files retain their original bytes when generation conflicts.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert!(fixture_identity_authenticates_before_remove);
assert!(!fixture_identity_available_after_remove);
assert!(system_agent_running_after_stop_owned);
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test ssh_agent -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Provide GUI key creation/import, fingerprint inspection and agent identities. Prefer system agents; create an owned agent only on explicit request and stop only agents TermCraft owns. Respect passphrases, identity lifetime and platform named-pipe/socket differences.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test ssh_agent -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task S07: Advanced SSH and proxy settings

**Files:** Create `src/ssh/{config,proxy}.rs`, `tests/ssh_options.rs`; modify `src/model.rs`, `src/ui/pages.rs`, `src/connection.rs`.

**Dependencies:** F01, S05, S06

**Interfaces / deliverable:** `SshOptions::resolve(session: &Session, defaults: &SshDefaults) -> Result<ResolvedSshOptions, String>`; options include keepalive, timeout, HTTP CONNECT/SOCKS proxy, jump chain policy and declared OpenSSH directives. `ConfigReport` enumerates unsupported directives.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``keepalive_and_proxy_reach_the_transport`, `unsupported_directive_keeps_visible_compatibility_mode`, `proxy_secret_is_not_in_launch_args``. Required observations: A controlled proxy sees the requested destination and keepalive interval is driven by a test clock. Unknown directives produce an explicit report rather than connecting to a guessed endpoint. Blank/zero/invalid timeout fields are rejected.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(proxy_observed_destination, configured_destination);
assert_eq!(observed_keepalive_interval_secs, 60);
assert!(!launch_arguments_contain_proxy_secret);
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test ssh_options -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Expose settings, migrate fixed defaults, and test actual transport effects. Parse only supported OpenSSH config directives with source/precedence reporting; retain explicit compatibility mode for external programs/unknown semantics. Proxy credentials reference S05 and are never placed in process arguments.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test ssh_options -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Handoff

The user approved the complete 60-point plan on 2026-11-23 and selected inline execution. Execute this subsystem under that approval; record routine implementation decisions and unresolved evidence in PARITY_PROGRESS.md. Keep external validation gaps explicit.
