# Evidence and edition-sensitive scope decisions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver checklist outcomes E01, E02, E03, E04, E05 with the user-visible behavior and evidence stated below.

**Architecture:** Resolve uncertain requirements using the exact Personal 26.6 target and recorded source/runtime evidence. These are research deliverables with reviewable observations, not fabricated implementation tests or automatic exclusions.

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

## Task E01: Determine general-purpose embedded browser requirement

**Files:** Create `docs/parity-evidence/schema.json`, `docs/parity-evidence/E01-browser.md`; update `PARITY_CHECKLIST.md`, `FEATURE_PARITY.md` only after evidence review.

**Dependencies:** Original runtime or equivalent reliable original evidence

**Interfaces / deliverable:** Define EvidenceRecord using shared contracts. Classify the original Browser workflow as general-purpose browser, remote file browser, another feature, or unresolved; record exact UI/handler/capture evidence.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run `Original observation protocol, not a Rust test`. Required observations: Evidence contains version/edition, steps, capture/handler references and observed result. An unavailable original runtime produces unresolved status. Any confirmed browser subtask names its rendering backend, navigation/input tests and native platform requirements before execution.

- [ ] **Step 2 — Run the defined observation protocol.** Inspect original menus/session types and runtime behavior. Distinguish the documented SSH-browser from a web browser; do not infer either presence or absence from the term browser alone. If general browsing is confirmed, append a concrete implementation subtask using a sandboxed embedded browser with navigation, certificates, auth and workspace lifecycle tests; retain external URL opening separately.

- [ ] **Step 3 — Compare actual results with the required observations.** Evidence contains version/edition, steps, capture/handler references and observed result. An unavailable original runtime produces unresolved status. Any confirmed browser subtask names its rendering backend, navigation/input tests and native platform requirements before execution.

- [ ] **Step 4 — Resolve failures at their owning implementation task.** For product defects, write the reproducing test, observe RED, fix it and run the targeted and full Rust suites. For missing endpoints/source evidence, record unavailable/unresolved without manufacturing a passing result.

- [ ] **Step 5 — Record and review evidence.** Evidence record is complete and reviewed; resulting scope and any added implementation work are explicitly recorded. Link captures, artifact hashes, versions, commands and remaining differences from `PARITY_PROGRESS.md` and update the corresponding checklist item only after its evidence review.

## Task E02: Confirm macro timing, scheduling and graphical scope

**Files:** Create `docs/parity-evidence/E02-macros.md`; if confirmed create `src/macros/{mod,events,scheduler}.rs`, `tests/macro_events.rs`; modify `src/model.rs`, `src/ui/pages.rs`.

**Dependencies:** V07 or equivalent original evidence; F01 credential boundary

**Interfaces / deliverable:** Keep existing text Macro intact. If observed, define `MacroEvent::{Text,Key,Wait,Pointer}`, `MacroPlayer::tick(clock) -> Vec<MacroAction>` and versioned import/export; scheduling uses an owned app scheduler with explicit target context.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``macro_playback_obeys_recorded_timing` and `macro_never_records_auth_challenges` only when the original evidence confirms the corresponding capability`. Required observations: Evidence differentiates existing text replay from additional behavior. If timing is required, actions at 0/100/250 ms fire once in order under an injected clock and cancel stops future events; otherwise record why additional timing is outside observed scope.

- [ ] **Step 2 — Verify RED.** Run `cargo test --offline --test macro_events -- --nocapture`. Expected: the named behavioral assertions fail against the current product, or the new API is absent. Resolve fixture/setup errors before treating a failure as regression evidence.

- [ ] **Step 3 — Implement the declared behavior.** Record the original editor/recorder and whether it supports timing, schedules or pointer actions. Implement only confirmed required semantics and test a fake clock against real macro execution boundaries. Do not capture credential dialogs; preserve existing text macro files.

- [ ] **Step 4 — Verify GREEN and review.** Run `cargo test --offline --test macro_events -- --nocapture`, then `cargo fmt --check` and `cargo test --offline`. Expected: all implemented task cases and the whole suite pass; unavailable external cases are identified separately. Review the task against the five Review Focus classes above.

- [ ] **Step 5 — Record the verified result.** Evidence is reviewed; any confirmed additional macro implementation passes its targeted/full-suite tests. Unresolved scope stays open. Save command/output references in `PARITY_PROGRESS.md`; update the matching checklist/matrix row only with supported evidence. Commit only if a real repository is present; otherwise record file changes without claiming a commit.

## Task E03: Confirm additional server modes and controls

**Files:** Create `docs/parity-evidence/E03-servers.md`; update `src/services/` task specs and parity documents for confirmed FTPS/VNC/NFS/cron modes.

**Dependencies:** Original runtime/equivalent evidence; B01 supervisor for confirmed implementations

**Interfaces / deliverable:** Produce a `ServerScopeRecord` for each candidate with protocol, visible controls, lifecycle, auth/root defaults, privilege requirements and original evidence. Confirmed services extend B01/B03 ServiceConfig, not ad hoc global daemon changes.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run `Original service inventory plus per-confirmed-service endpoint probes`. Required observations: Every candidate is confirmed, disproved with evidence, or unresolved. A confirmed service has a concrete supervisor implementation/fixture assignment; an unverified handler name does not establish a complete service workflow.

- [ ] **Step 2 — Run the defined observation protocol.** Inspect exact original services UI and captured handlers, then probe configured endpoints. Reconcile existing cron/FTP decompilation evidence with user-visible behavior. For each confirmed service add start/stop, bounded logs, configured endpoint and cleanup tests; NFS/cron must explicitly define OS/privilege support.

- [ ] **Step 3 — Compare actual results with the required observations.** Every candidate is confirmed, disproved with evidence, or unresolved. A confirmed service has a concrete supervisor implementation/fixture assignment; an unverified handler name does not establish a complete service workflow.

- [ ] **Step 4 — Resolve failures at their owning implementation task.** For product defects, write the reproducing test, observe RED, fix it and run the targeted and full Rust suites. For missing endpoints/source evidence, record unavailable/unresolved without manufacturing a passing result.

- [ ] **Step 5 — Record and review evidence.** Reviewed scope record and completed confirmed service additions; unavailable native privileges remain pending evidence. Link captures, artifact hashes, versions, commands and remaining differences from `PARITY_PROGRESS.md` and update the corresponding checklist item only after its evidence review.

## Task E04: Confirm original limits, defaults, imports and runtime formats

**Files:** Create `docs/parity-evidence/E04-defaults.md`, `tests/fixtures/original/`, `tests/original_defaults.rs`; update `MECHANICS.md`, `mechanics.json`, `src/model.rs` after measurement.

**Dependencies:** Original runtime or trustworthy recovered evidence; S01/P05/P06/B06 consume results

**Interfaces / deliverable:** `OriginalMeasurement` records field, target profile, action sequence, observed value, confidence and evidence reference. `DefaultsPolicy::for_profile(profile: ProtocolProfile)` applies only confirmed profile-specific values.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run ``confirmed_profile_defaults_match_measurements`, `personal_limit_boundary_matches_observed_behavior` after measurements exist`. Required observations: Each value in mechanics has measured/recovered/estimated provenance; test expectations derive from checked-in measurement records. No global cap is inferred from one handler or unrelated profile.

- [ ] **Step 2 — Run the defined observation protocol.** Measure tunnel/session limits, protocol defaults, SFTP behavior, plugin/package formats and game catalog. Separate Personal limits from Professional behavior and from TermCraft estimates. Replace estimates only when supported; use non-destructive boundary tests and distinguish persisted-entry limits from concurrent connection limits.

- [ ] **Step 3 — Compare actual results with the required observations.** Each value in mechanics has measured/recovered/estimated provenance; test expectations derive from checked-in measurement records. No global cap is inferred from one handler or unrelated profile.

- [ ] **Step 4 — Resolve failures at their owning implementation task.** For product defects, write the reproducing test, observe RED, fix it and run the targeted and full Rust suites. For missing endpoints/source evidence, record unavailable/unresolved without manufacturing a passing result.

- [ ] **Step 5 — Record and review evidence.** Measurement record reviewed and every confirmed behavior change tested; unmeasured values remain labeled estimated. Link captures, artifact hashes, versions, commands and remaining differences from `PARITY_PROGRESS.md` and update the corresponding checklist item only after its evidence review.

## Task E05: Personal versus Professional scope

**Files:** Create `docs/parity-evidence/E05-editions.md`; update master plan scope and parity documents only after review.

**Dependencies:** Official edition sources and exact target evidence

**Interfaces / deliverable:** Produce an edition matrix: feature, Personal26.6 evidence, Professional evidence, TermCraft requirement, disposition and approval reference.

- [ ] **Step 1 — Establish the failing behavior or missing evidence.** Add/run `Edition-source review, not a Rust test`. Required observations: Every proposed exclusion identifies supporting edition/version evidence. Not-applicable outcomes need review; no whole subsystem is silently dropped based on marketing wording or absent UI capture.

- [ ] **Step 2 — Run the defined observation protocol.** Check the Professional Customizer and edition-dependent limits against official edition documentation and the supplied executable. Retain baseline Personal scope; put requested additions beyond it into an explicit extension list. Do not remove a requirement merely because implementing it is difficult.

- [ ] **Step 3 — Compare actual results with the required observations.** Every proposed exclusion identifies supporting edition/version evidence. Not-applicable outcomes need review; no whole subsystem is silently dropped based on marketing wording or absent UI capture.

- [ ] **Step 4 — Resolve failures at their owning implementation task.** For product defects, write the reproducing test, observe RED, fix it and run the targeted and full Rust suites. For missing endpoints/source evidence, record unavailable/unresolved without manufacturing a passing result.

- [ ] **Step 5 — Record and review evidence.** Edition matrix reviewed; all resulting scope decisions and any user-requested extensions are recorded without false completion claims. Link captures, artifact hashes, versions, commands and remaining differences from `PARITY_PROGRESS.md` and update the corresponding checklist item only after its evidence review.

## Handoff

The user approved the complete 60-point plan on 2026-11-23 and selected inline execution. Execute this subsystem under that approval; record routine implementation decisions and unresolved evidence in PARITY_PROGRESS.md. Keep external validation gaps explicit.
