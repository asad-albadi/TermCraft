# Protocol completeness, packages, plugins and releases Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver checklist outcomes P01, P02, P03, P04, P05, P06, P07, P08 with the user-visible behavior and evidence stated below.

**Architecture:** Add structured protocol transports behind existing session types and a versioned runtime manifest for necessary packaged tools. Shared tool discovery must distinguish app-bundled, system and unavailable providers. Every package/plugin capability gets its own visible lifecycle and evidence.

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

## Task P01: Bundled protocol client runtime

**Files:** Create `src/runtime/{mod,manifest,discovery}.rs`, `packaging/runtime-manifest.json`, `tests/runtime_manifest.rs`; modify `src/connection.rs`, `src/ui/pages.rs`.

**Dependencies:** F01 engine; D/X protocols consume this catalog as introduced

**Interfaces / deliverable:** `RuntimeCatalog::resolve(tool: ToolId) -> Result<ToolProvider, String>` returns Embedded, Bundled(path,version) or System(path,version); all bundled paths resolve inside the package. `RuntimeManifest` pins artifacts, hashes, platforms, licenses and source locations.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``packaged_runtime_works_with_empty_path`, `manifest_rejects_wrong_architecture_or_hash``. Required observations: Required packaged workflows resolve with PATH cleared. A modified helper hash or unsupported architecture causes a clear error before execution. No provider silently downloads or executes an unpinned artifact.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert!(required_workflows_resolve_with_empty_path);
assert!(wrong_hash_result.is_err());
assert!(wrong_architecture_result.is_err());
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test runtime_manifest -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Inventory every session/tool dependency, prefer embedded engines, and package remaining required helpers. Validate checksums, architecture, executable permissions and library closure on a clean installation. PATH fallback remains visible and does not conceal missing bundled functionality.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test runtime_manifest -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task P02: FTP/FTPS graphical transport

**Files:** Create `src/protocols/{mod,ftp}.rs`, `tests/ftp_ftps.rs`; modify `src/model.rs`, `src/ui/remote_files.rs`.

**Dependencies:** F01/F02/F04 shared remote-files APIs

**Interfaces / deliverable:** `FtpBackend` implements remote list/transfer operations; `FtpOptions` includes TLS mode, data encryption, passive/active mode and binary/ASCII conversion. `connect(options) -> Result<Self, String>` verifies server identity for TLS.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``ftps_validates_certificate_and_encrypts_selected_channels`, `binary_transfer_is_byte_exact`, `ascii_mode_converts_only_selected_text``. Required observations: Known binary bytes round-trip unchanged; explicit text conversion yields documented newline transformations. Bad TLS identity fails; encrypted-data mode never falls back to clear data. Passive/active tests inspect actual negotiated connections.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(binary_download, binary_upload);
assert_eq!(converted_text, b"one\r\ntwo\r\n");
assert!(!invalid_certificate_connected);
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test ftp_ftps -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Replace terminal-only FTP for graphical workflows. Implement encrypted control/data, passive/active negotiation, text conversion selected only for text transfers, timeouts/cancel and useful server errors. Integrate F02 transfer conflicts and F04 remote editing.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test ftp_ftps -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task P03: Telnet, Rlogin and Rsh transport options

**Files:** Create `src/protocols/{telnet,rlogin,rsh}.rs`, `tests/legacy_protocols.rs`; modify `src/connection.rs`, `src/model.rs`, `src/ui/pages.rs`.

**Dependencies:** T01 terminal transport; D04 for gateways when selected

**Interfaces / deliverable:** `LegacyProtocolConfig::validate() -> Result<(), String>` carries host, port, username, terminal size and gateway; protocol sessions expose input/output/resize/close through the terminal transport contract.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``telnet_iac_is_not_displayed_as_text`, `configured_legacy_port_and_user_reach_server`, `unsupported_privileged_port_requirement_is_explicit``. Required observations: Telnet WILL/DO/NAWS fixtures get correct negotiations and escaped 0xff payload. Rlogin/Rsh use the configured port and username; refusal due to unsupported privilege requirements is reported honestly. Close cleans all channels.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(displayed_telnet_payload, b"hello\xffworld");
assert_eq!(fixture_observed_port, configured_port);
assert_eq!(fixture_observed_user, "alice");
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test legacy_protocols -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Implement Telnet IAC negotiation, terminal/window size and login settings; implement Rlogin/Rsh handshake and custom endpoint behavior. Verify privileged-source-port requirements explicitly instead of inventing a successful unprivileged mode. Existing helpers may remain labeled compatibility providers.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test legacy_protocols -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task P04: Complete serial line settings

**Files:** Create `tests/serial_settings.rs`, `tests/support/serial.rs`; modify `src/serial.rs`, `src/model.rs`, `src/ui/pages.rs`.

**Dependencies:** None beyond shared contracts

**Interfaces / deliverable:** `SerialSettings { baud, data_bits, parity, stop_bits, flow_control }`; `Serial::open_with_settings(device: &str, settings: SerialSettings) -> Result<Self, String>`, plus cancellable close and explicit read/write error events.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``serial_settings_map_to_native_flags`, `old_serial_session_stays_9600_8n1`, `idle_serial_close_stops_reader``. Required observations: A 7E1/RTS-CTS configuration produces the corresponding termios/DCB values; unsupported values fail before open. Closing an idle PTY-backed transport completes promptly and unplug errors are visible. Physical settings need V04 hardware evidence.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(native_parity, Parity::Even);
assert_eq!(native_data_bits, 7);
assert_eq!(legacy_baud, 9600);
assert!(idle_reader_stopped_within_deadline);
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test serial_settings -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Add POSIX termios and Windows DCB mappings for supported settings, preserving old 9600/8N1 defaults. Validate combinations, check every OS call, and propagate device removal. Ensure read-thread shutdown does not rely on the device producing data.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test serial_settings -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task P05: Portable Unix environment and package manager

**Files:** Create `src/packages/{mod,index,install}.rs`, `src/ui/packages.rs`, `packaging/unix-runtime/manifest.json`, `tests/packages.rs`; modify `src/connection.rs`, `src/model.rs`.

**Dependencies:** P01; E04 package-format evidence

**Interfaces / deliverable:** `PackageManager::{list,install,remove,repair}` consume a versioned PackageIndex and return typed progress/outcomes; `UnixEnvironment::launch_config() -> Result<Launch, String>` supplies a private root/home/drive-path environment.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``failed_package_install_rolls_back`, `portable_unix_home_survives_restart`, `windows_drive_mapping_resolves_expected_file``. Required observations: Corrupt hash or interrupted extraction leaves the previous package usable. A user file in the portable home survives upgrade/restart. No package writes outside the app-owned root through archive traversal or symlinks.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(installed_version_after_failed_upgrade, previous_version);
assert_eq!(persistent_home_file, b"keep");
assert!(!archive_escape_path.exists());
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test packages -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Provide Windows Unix tooling through pinned redistributable runtime packages and app-owned persistent directories. Add an equivalent package UI on supported platforms. Stage verified installs atomically, detect conflicts and roll back failures. Document compatibility with original MobApt packages only when proven.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test packages -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task P06: Plugins and extension lifecycle

**Files:** Create `src/plugins/{mod,manifest,install,registry}.rs`, `src/ui/plugins.rs`, `tests/plugins.rs`; modify `src/ui.rs`, `src/model.rs`.

**Dependencies:** P05 runtime package primitives; E04 original plugin-format evidence

**Interfaces / deliverable:** `PluginManifest` declares version, compatibility, entrypoints and permissions/capabilities; `PluginRegistry::{inspect,install,enable,disable,remove}` returns explicit results. Host/plugin protocol is versioned and process-isolated.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``plugin_crash_keeps_terminal_running`, `disabled_plugin_cannot_execute`, `malicious_archive_cannot_escape_plugin_root``. Required observations: A fixture plugin crash yields a plugin error while terminal output continues. Disabled plugins receive no commands. Traversal/absolute-path/symlink extraction entries are rejected before activation.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert!(terminal_still_running_after_plugin_crash);
assert_eq!(disabled_plugin_command_count, 0);
assert!(!archive_escape_path.exists());
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test plugins -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Implement inspect/install/update/enable/disable/remove with capability prompts and crash isolation; validate archives and dependencies. Define independently written extension compatibility; support original formats only after E04 evidence. A plugin failure must not terminate active terminal sessions.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test plugins -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task P07: Missing graphical tools

**Files:** Create `src/tools/{mod,diff,image,scan,capture}.rs`, `src/ui/tools.rs`, `tests/tools.rs`; modify `src/ui/pages.rs`, `src/ui.rs`.

**Dependencies:** P01; native capture providers require platform evidence

**Interfaces / deliverable:** `FileDiff::compare(left: &Path, right: &Path) -> Result<DiffResult, String>`, `FolderDiff::compare(left: &Path, right: &Path) -> Result<FolderDiffResult, String>`, `ImagePreview::open(path: &Path, max_pixels: u64) -> Result<Self, String>`, `PortScan::start(target: ScanTarget) -> Result<ScanHandle, String>`, `PacketCapture::start(config: CaptureConfig) -> Result<CaptureHandle, String>` each owns cancellation and bounded output.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``folder_diff_reports_added_removed_changed`, `oversized_image_is_rejected_before_allocation`, `scan_and_capture_cancel_release_owned_resources``. Required observations: A fixture folder yields exact added/removed/changed sets. Scan only the configured loopback ports and report open/closed results correctly. Capture tests use deterministic local traffic; unavailable native access is recorded as unavailable rather than empty success.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(added_names, vec!["new.txt"]);
assert_eq!(removed_names, vec!["old.txt"]);
assert_eq!(changed_names, vec!["changed.txt"]);
assert!(oversized_image_result.is_err());
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test tools -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Add file/folder comparison, bounded image preview, explicit-target port scanning and packet capture using native capture providers where available. Show required OS access/providers rather than silently invoking elevated operations. Keep scans/capture off UI thread and bounded in memory/disk.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test tools -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task P08: Distributable applications and release verification

**Files:** Create `.github/workflows/{ci,release}.yml`, `packaging/{linux,windows,macos}/`, `scripts/check-package-startup.*`, `tests/package_manifest.rs`, `docs/releases.md`; modify `Cargo.toml`, `README.md`, `CREDITS.md`.

**Dependencies:** P01-P07 runtime requirements; X01 artifact contract; B04/B05 launch integration finalized afterward

**Interfaces / deliverable:** `PackageManifest` enumerates executables, data, runtime libraries, engine/package versions, licenses and hashes; startup checker accepts artifact path, platform/display and evidence destination, returning nonzero on failure.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``package_manifest_has_complete_runtime_closure`, `packaged_startup_produces_nonblank_screenshot`, `upgrade_preserves_workspace``. Required observations: Artifacts launch without developer paths or undeclared tools; signatures/checksums match actual artifacts. Upgrade/uninstall leaves user sessions intact. Missing signing credentials mean unsigned evidence, never a signing claim. Save startup logs and screenshots for every target.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(undeclared_runtime_file_count, 0);
assert!(startup_screenshot_is_nonblank);
assert_eq!(workspace_after_upgrade, workspace_before_upgrade);
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test package_manifest -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Produce Linux AppImage/archive plus supported distro package, Windows per-user installer/portable app, and macOS bundles for supported architectures. Package pinned runtime/X engines, icon assets and notices; keep user data separate from install files. Run clean-machine startup/workflow checks; sign/notarize only when actual credentials are available. Publication remains a separate explicit action.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test package_manifest -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Handoff

The user approved the complete 60-point plan on 2026-11-23 and selected inline execution. Execute this subsystem under that approval; record routine implementation decisions and unresolved evidence in PARITY_PROGRESS.md. Keep external validation gaps explicit.
