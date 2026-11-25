# Embedded RDP, VNC and SSH gateways Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver checklist outcomes D01, D02, D03, D04 with the user-visible behavior and evidence stated below.

**Architecture:** Use a shared desktop-session model with decoded framebuffer updates, input events and clipboard capabilities, rendered inside egui. Evaluate IronRDP and vnc-rs against captured protocol requirements before pinning dependencies; unavailable device-redirection capabilities remain explicit gaps rather than falling back silently to external windows.

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

## Task D01: Embedded RDP view

**Files:** Create `src/desktop/{mod,framebuffer,rdp}.rs`, `src/ui/desktop.rs`, `tests/rdp_session.rs`; modify `src/ui.rs`, `src/model.rs`, `Cargo.toml`.

**Dependencies:** F01 challenge conventions; E04/V07 original configuration evidence

**Interfaces / deliverable:** `DesktopSession::connect(config: DesktopConfig) -> Result<Self, String>`; `send(input: DesktopInput) -> Result<(), String>`, `poll() -> Vec<DesktopEvent>`, `close()`. Events include Frame(rect,pixels), Cursor, AuthChallenge, Clipboard, Disconnected; `DesktopConfig` includes endpoint, dimensions and credential reference.

- [x] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``rdp_frame_updates_render_inside_workspace`, `rdp_certificate_change_requires_resolution`, `closing_rdp_releases_transport``. Required observations: A controlled RDP server draws known colored rectangles and receives click/key events at expected coordinates; no external desktop window opens. Resizing the view negotiates/letterboxes correctly. Changed certificate cannot be silently accepted. Close stops event delivery and releases sockets.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(observed_frame_pixel, [255, 0, 0, 255]);
assert_eq!(external_desktop_windows_started, 0);
assert!(!changed_certificate_was_accepted);
```

- [x] **Step 2 — Verify RED.** Run `cargo test --offline --test rdp_session -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [x] **Step 3 — Implement the declared behavior.** Create an in-app framebuffer surface using IronRDP as the first engine candidate. Implement certificate validation/trust, authentication, frame updates, resize, input focus, cancellation and session close. Prototype unsupported mandatory capabilities before committing to an engine. Existing external RDP remains visibly optional, never counted as embedded.

- [x] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test rdp_session -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [x] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task D02: RDP settings and device redirection

**Files:** Create `src/desktop/rdp_options.rs`, `tests/rdp_options.rs`; modify `src/model.rs`, `src/ui/pages.rs`, `src/desktop/rdp.rs`, `src/connection.rs`.

**Dependencies:** D01, S05

**Interfaces / deliverable:** `RdpOptions::validate() -> Result<(), String>` and `negotiate(capabilities: &RdpCapabilities) -> Result<NegotiatedRdpOptions, String>` cover administrative session, clipboard, keyboard routing, drive/port/printer/smartcard redirection and authentication/security choices.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``rdp_username_reaches_windows_profile`, `redirection_options_match_negotiated_channels`, `unsupported_smartcard_is_not_silently_enabled``. Required observations: A Windows profile contains the intended username and endpoint but no password. Disabled clipboard/drive settings open no corresponding channel; enabled settings work against native endpoints. Unsupported required redirection remains an open D02 gap.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(parsed_windows_profile_username, "alice");
assert_eq!(disabled_clipboard_channel_count, 0);
assert!(!unsupported_smartcard_was_reported_enabled);
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test rdp_options -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Expose documented options and implement each negotiated channel. Fix Windows external-adapter username propagation using a temporary protected .rdp profile or supported API, never unsupported mstsc flags. Report unavailable server/client capabilities per option; do not display a setting that is silently ignored.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test rdp_options -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task D03: Embedded VNC view

**Files:** Create `src/desktop/vnc.rs`, `tests/vnc_session.rs`; modify `src/ui/desktop.rs`, `src/model.rs`.

**Dependencies:** D01 common framebuffer/input interfaces

**Interfaces / deliverable:** Implement the D01 `DesktopSession` contract for `DesktopProtocol::Vnc`; `VncOptions` supplies view_only, fullscreen, clipboard policy and supported security types.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``vnc_known_frame_and_pointer_coordinates_match`, `view_only_sends_no_keyboard_or_pointer`, `vnc_resize_reinitializes_framebuffer``. Required observations: A known RFB rectangle yields exact expected pixels and translated coordinates. In view-only mode attempted input emits zero protocol input messages; clipboard follows its selected direction. Test disconnect during update and oversized dimensions before allocation.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(decoded_frame_pixel, [0, 255, 0, 255]);
assert_eq!(view_only_input_messages, 0);
assert_eq!(resized_frame_dimensions, (800, 600));
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test vnc_session -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Use vnc-rs as a candidate, verify required encodings/security, and integrate framebuffer updates/cursor/keyboard/clipboard with the common view. View-only must block input at the transport boundary; fullscreen remains a workspace view state. Explicitly report unsupported encodings/security rather than rendering corrupt frames.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test vnc_session -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task D04: SSH gateway for desktop and Telnet sessions

**Files:** Create `src/ssh/gateway.rs`, `tests/protocol_gateway.rs`; modify `src/desktop/{rdp,vnc}.rs`, `src/connection.rs`, `src/model.rs`.

**Dependencies:** F01 forwarding; D01/D03 or P03 target adapters

**Interfaces / deliverable:** `GatewayLease::open(config: GatewayConfig, target: Endpoint) -> Result<Self, String>` exposes a connected byte stream and owns its forwarding lifecycle. Reuse F01 host verification/challenges and per-hop identity checks.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``gateway_reaches_target_without_public_listener`, `target_failure_releases_gateway_channel`, `changed_gateway_host_key_blocks_desktop``. Required observations: Only the controlled gateway can reach the target; framebuffer/Telnet traffic works over that stream. Closing the tab closes its channel and leaves unrelated gateway users intact. A rotated gateway key prevents forwarding/authentication to the target.

The test must obtain the values below from the real fixture/production operations described above (imports/setup omitted here), then assert:

```rust
assert_eq!(received_through_gateway, b"GATEWAY_OK");
assert_eq!(unintended_public_listener_count, 0);
assert_eq!(remaining_channels_after_target_close, 0);
```

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test protocol_gateway -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Route RDP/VNC/Telnet streams through SSH direct-tcpip without opening an unintended public listener. Preserve target authentication separately from gateway authentication. On failed target connection, release the lease; reconnect verifies both hops. Local-listener compatibility adapters bind loopback only.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test protocol_gateway -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Targeted tests and the full Rust suite pass; native validation limitations remain recorded. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Handoff

The user approved the complete 60-point plan on 2026-11-23 and selected inline execution. Execute this subsystem under that approval; record routine implementation decisions and unresolved evidence in PARITY_PROGRESS.md. Keep external validation gaps explicit.
