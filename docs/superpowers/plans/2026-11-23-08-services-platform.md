# Servers, desktop integration and games Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver checklist outcomes B01, B02, B03, B04, B05, B06 with the user-visible behavior and evidence stated below.

**Architecture:** Use an owned service supervisor with explicit bind address/root/credentials and bounded logs. Bundle or implement services without modifying system daemons. Platform integrations and games get independent source/assets and native verification.

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

## Task B01: FTP, TFTP and Telnet servers

**Files:** Create `src/services/{mod,supervisor,ftp,tftp,telnet}.rs`, `tests/services.rs`; modify `src/ui/servers.rs`, `src/model.rs`.

**Dependencies:** P01 package providers; E03 determines extra server requirements

**Interfaces / deliverable:** `ServiceSupervisor::start(config: ServiceConfig) -> Result<ServiceId, String>`, `stop(id)`, `status(id) -> ServiceStatus`; configs declare protocol, bind address, root, auth and runtime provider.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``tftp_cannot_escape_share_root`, `occupied_port_reports_start_failure`, `service_stop_closes_only_owned_listener``. Required observations: GET/PUT against fixture roots obey selected permissions and cannot traverse via encoded path/symlink. Starting on an occupied port fails without claiming Running. Stop releases the service port and leaves unrelated listeners alive.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert!(!outside_share_path.exists());
assert!(occupied_port_start.is_err());
assert!(unrelated_listener_remains_open);
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test services -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Add owned servers with configurable endpoint/root, useful start errors, stop/restart and bounded logs. FTP uses declared authentication; TFTP confines paths to its shared root; Telnet exposes only the explicitly selected shell/session. Default binding is loopback until the user changes it.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test services -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task B02: Complete HTTP file sharing

**Files:** Create `tests/http_sharing.rs`; modify `src/http_server.rs`, `src/ui/servers.rs`, `src/model.rs`.

**Dependencies:** E03/V07 original HTTP mode evidence; F02 conflict/recovery policy

**Interfaces / deliverable:** `HttpShareConfig` includes root, bind, timeout, auth policy and upload policy; `Server::start_with_config(config) -> Result<Self, String>` preserves the existing read-only `start` wrapper for old callers.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``range_returns_correct_bytes_and_headers`, `unauthenticated_upload_cannot_modify_share`, `cancelled_upload_keeps_existing_file``. Required observations: `Range: bytes=2-4` over `abcdef` yields 206, `cde`, and correct Content-Range/Length; unsatisfiable range yields 416. HEAD sends no body. Failed auth, traversal, oversize or partial upload does not replace an existing destination.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(response.status, 206);
assert_eq!(response.body, b"cde");
assert_eq!(response.header("Content-Range"), Some("bytes 2-4/6"));
assert_eq!(existing_file_after_failed_upload, b"keep");
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test http_sharing -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Implement original-supported authentication/upload/range behavior after measuring its modes. Keep default read-only compatibility. Constrain canonical paths, stage uploads with explicit conflicts, parse bounded headers/body, and implement valid GET/HEAD/single-range semantics before optional multipart ranges.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test http_sharing -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task B03: SSH/SFTP and Iperf service management

**Files:** Create `src/services/{sshd,iperf}.rs`, `tests/service_adapters.rs`; modify `src/ui/servers.rs`, `src/model.rs`.

**Dependencies:** B01 supervisor; F01 SSH primitives; P01 provider packaging

**Interfaces / deliverable:** `SshServiceConfig` declares host-key store, authorized users/keys, bind/port and SFTP capability; `IperfServiceConfig` declares version, bind/port and supported options. Both return owned ServiceId through B01 supervisor.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``ssh_service_auth_and_sftp_match_config`, `iperf_version_and_bind_are_applied`, `restart_preserves_service_host_key``. Required observations: An isolated client authenticates using allowed credentials and denied users cannot. Iperf2/3 clients reach the selected version/port. Restart preserves host fingerprint and stop leaves system daemons untouched.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert!(allowed_key_authenticated);
assert!(!denied_user_authenticated);
assert_eq!(host_fingerprint_before_restart, host_fingerprint_after_restart);
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test service_adapters -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Package providers or use an embedded SSH server where interoperability is verified. Expose supported auth/listen settings instead of fixed localhost/key-only behavior; preserve safe current defaults. Never reuse or edit system sshd configuration. Generate host keys asynchronously and persist them in app-owned service data.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test service_adapters -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task B04: Windows Explorer, paths and registry integration

**Files:** Create `src/platform/{mod,windows_integration,path_map,registry_fs}.rs`, `tests/windows_integration.rs`; modify `src/main.rs`, `src/cli.rs`, `packaging/windows/`.

**Dependencies:** S04, P05, P08

**Interfaces / deliverable:** `WindowsIntegration::{install_user,remove_user,status}` owns named registry entries; `PathMap::to_native(remote_style: &str) -> Result<PathBuf, String>`; `RegistryFs` exposes typed read/enumerate/write operations with explicit target and confirmation.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``explorer_launch_preserves_working_directory`, `uninstall_removes_only_owned_registry_entries`, `drive_path_mapping_handles_unicode``. Required observations: Launching from a folder containing spaces opens that exact cwd. Removal leaves unrelated handlers/settings intact. Registry access denied is displayed rather than converted to an empty directory. Validate on native Windows.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(opened_cwd, requested_cwd_with_spaces);
assert!(unrelated_registry_handler_survives_uninstall);
assert!(denied_registry_access_is_reported);
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test windows_integration -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Add per-user Explorer launch/protocol handlers and drive/path mappings, plus the requested registry filesystem behavior. Use native APIs and structured launch arguments; install/remove only app-owned entries. Make privileged/unavailable operations explicit. Keep a record of original entries when modifying user-owned locations.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test windows_integration -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task B05: Linux and macOS desktop integration

**Files:** Create `src/platform/{linux_integration,macos_integration}.rs`, `tests/desktop_integration.rs`; modify `packaging/{linux,macos}/`, `src/cli.rs`, `README.md`.

**Dependencies:** S04, P08

**Interfaces / deliverable:** `DesktopIntegration::{install_user,remove_user,status}` manages .desktop/icon/protocol entries on Linux and bundle URL/file declarations on macOS; actions map to S04 CliAction without shell interpolation.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``desktop_launch_preserves_arguments_and_workspace`, `integration_removal_keeps_user_defaults``. Required observations: A launcher resolves the installed binary/icon after relocation and passes an encoded URL/path literally. Removing TermCraft integration leaves a pre-existing default application unchanged. Confirm on Linux desktops and native macOS.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(received_launch_arguments, requested_launch_arguments);
assert!(installed_icon_resolves_after_relocation);
assert_eq!(default_app_after_removal, original_default_app);
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test desktop_integration -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Package application-menu/icon, open-here and supported URL/file launch actions. Resolve installed paths at runtime, respect user default associations, and provide uninstall cleanup. Document native equivalents rather than inventing a registry filesystem on Unix.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test desktop_integration -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task B06: Original games/screensaver equivalence

**Files:** Create `src/games/{mod,library,screensaver}.rs`, `src/ui/games.rs`, `tests/games.rs`, `docs/parity-evidence/B06/`; modify `src/ui.rs` and add independently created assets under `assets/games/`.

**Dependencies:** E04/V07 original catalog required before game-specific implementation

**Interfaces / deliverable:** `GameRegistry::catalog() -> &[GameSpec]`, `start(id: GameId, seed: u64) -> GameSession`; each measured original game gets a defined state/update/input/outcome contract. `ScreensaverState::tick(dt)` supports idle start and input dismissal.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``game_rules_match_recorded_cases`, `screensaver_dismisses_without_losing_terminal_input``. Required observations: Every measured game has recorded rule/input fixtures and deterministic expected outcomes. Screensaver starts only after configured idle time and dismissal preserves running sessions. Missing original inventory leaves B06 pending, not complete.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(game_outcome, recorded_original_outcome);
assert!(screensaver_dismissed_on_input);
assert!(terminal_session_preserved);
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test games -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Inventory the exact Personal 26.6 games and screensaver first using E04/V07; record names, rules and controls. Implement each catalog entry with independent assets and deterministic logic, then add the menu and optional idle screensaver. Do not substitute a single sample game for the measured suite.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test games -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Handoff

The user approved the complete 60-point plan on 2026-11-23 and selected inline execution. Execute this subsystem under that approval; record routine implementation decisions and unresolved evidence in PARITY_PROGRESS.md. Keep external validation gaps explicit.
