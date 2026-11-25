# macOS Issue #2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete all locally actionable macOS implementation and delivery work for issue #2, with exact native evidence and explicit gates for unavailable environments.

**Architecture:** Extend the existing macOS modules and packaging scripts through contract tests and small focused helpers. Keep native artifact trust, process ownership, permission handling, and user-data lifecycle as separate boundaries; integrate them only through existing runtime manifests and platform adapters.

**Tech Stack:** Rust 1.96.1, Cargo locked dependencies, Python 3.11+ packaging checks, Objective-C/AppKit launcher, Xcode/macOS SDK, Meson/Ninja XQuartz build tools, Mach-O tooling, LaunchServices, ScreenCaptureKit, PCSC, CUPS, and existing evidence manifests.

**Spec:** `docs/superpowers/specs/2026-11-25-macos-issue-2-design.md`

## Global Constraints

- Do not mark native macOS support complete from Linux tests, source checks, cross-compilation, or synthetic GUI evidence.
- Do not install or modify system XQuartz, LaunchServices defaults, shell startup, or unrelated user handlers.
- Production native-engine gates accept only compiled, relocated, signed, hash-pinned output with a complete dependency/resource closure.
- Preserve explicit limitations for Composite, GPU/monitor workflows, POSIX opened-object mutation, serial status families, unavailable devices, and original-version comparisons.
- Keep issue #2 and the parity queue/checklist/matrix synchronized after each completed slice.

## Review Focus

- A moved `.app` must still resolve adjacent helpers/resources and pass literal Finder/URL/Services arguments.
- A package replacement/removal must preserve settings, vault, recovery data, and unrelated handlers.
- A malformed, wrong-architecture, unsigned, unrelocated, or incomplete native artifact must remain rejected.
- Permission denial/revocation and cancellation must not leak capture callbacks, held input, processes, sockets, or authority files.
- Intel-only, physical-device, GPU/monitor, and original-runtime requirements must remain explicitly open when not evidenced.

---

### Task 1: Baseline Compatibility and Evidence Ledger

**Files:**
- Modify: `src/remote_files/transfer/operations/source.rs`
- Modify: `src/remote_files/transfer/folder_archive/unix.rs`
- Test: existing Rust unit tests and full workspace suite
- Create: `docs/parity-evidence/issue-2/BASELINE.md`

**Interfaces:** Preserve the existing `rustix` mode conversion semantics while making the pinned toolchain compile. The evidence ledger records the exact baseline command, host architecture, toolchain, and unrelated pre-existing warnings.

- [ ] **Step 1:** Add focused tests or assertions for mode values exceeding the platform raw-mode width and verify they fail before the conversion fix.
- [ ] **Step 2:** Run the focused tests and record the expected type mismatch or failing boundary.
- [ ] **Step 3:** Implement checked `u32` to `u16` mode conversion at the existing Unix file/archive boundaries without changing behavior for valid modes.
- [ ] **Step 4:** Run focused tests, formatting, and the full locked suite; record any environment-only failures.
- [ ] **Step 5:** Commit `fix: restore locked rustix mode compatibility`.

### Task 2: Native Artifact Manifest and Mach-O Verification

**Files:**
- Modify: `src/x11/native_policy.rs`
- Modify: `src/browser/native_macos.rs`
- Modify: `src/runtime/manifest.rs`
- Create/modify: `tests/native-component/src/lib.rs`
- Create: `docs/parity-evidence/issue-2/artifact-contract.md`

**Interfaces:** Native runtime discovery consumes architecture-specific Mach-O verification and file manifests; native policy produces a `NativePin` only for complete approved output.

- [ ] **Step 1:** Add failing tests for wrong CPU slices, duplicate/missing universal slices, forbidden `/opt/X11` references, missing resources, and incomplete manifests.
- [ ] **Step 2:** Run the focused native-component tests and confirm the new cases fail for the missing contract.
- [ ] **Step 3:** Implement minimal manifest validation and architecture/resource checks using existing `VerifiedSnapshot` and runtime-manifest patterns.
- [ ] **Step 4:** Run native-component tests and host-side manifest checks; keep `macos_pin()` closed until a real artifact is available.
- [ ] **Step 5:** Commit `feat: define macOS native artifact acceptance contract`.

### Task 3: Private XQuartz Build, Relocation, and Trust Pinning

**Files:**
- Modify: `packaging/x11/build-macos.sh`
- Modify: `packaging/x11/native/prepare-source.py`
- Modify: `packaging/x11/native/source-contract.json`
- Modify: `src/x11/native_macos.rs`
- Modify: `src/x11/native_policy.rs`
- Create: `tests/xquartz_macos_contract.py`
- Create: `docs/parity-evidence/issue-2/xquartz-build.md`

**Interfaces:** The builder outputs a fresh staged `macos-universal-private1` tree; the Rust provider consumes only its pinned executable, resources, dependencies, and authority-safe inherited socket behavior.

- [ ] **Step 1:** Add failing offline contract tests for source checksum validation, required configure flags, no `meson install`, no `/opt/X11`, private bundle-id prefix, relocation, and complete arm64/x86_64 closure.
- [ ] **Step 2:** Run them against the current script and confirm the missing protections/output contract.
- [ ] **Step 3:** Implement the smallest script and manifest changes needed to prepare, relocate, sign, inventory, and hash the staged tree without executing upstream installers.
- [ ] **Step 4:** On the available native host, run the builder or a bounded dry-run; run Mach-O/resource/hash checks and preserve unavailable Intel evidence as open.
- [ ] **Step 5:** Enable `macos_pin()` only when the exact staged artifact satisfies the acceptance contract; otherwise retain the gate and record the reason.
- [ ] **Step 6:** Commit `feat: complete private macOS XQuartz packaging contract`.

### Task 4: `.app` Packaging and Desktop Lifecycle

**Files:**
- Modify: `packaging/macos/Info.plist`
- Modify: `packaging/macos/desktop-launcher.m`
- Modify: `packaging/macos/desktop-integration.py`
- Modify: `tests/packaging_release.py`
- Create: `tests/macos_desktop_lifecycle.py`
- Create: `docs/parity-evidence/issue-2/app-lifecycle.md`

**Interfaces:** The packaged launcher forwards literal CLI/Finder/URL/Services arguments to the adjacent Rust executable; registration owns only recorded bundles and never changes unrelated defaults.

- [ ] **Step 1:** Add failing tests for relocation, multiple URL/file events, missing helper failure, registration receipt ownership, moved/replaced bundles, and unrelated-handler preservation.
- [ ] **Step 2:** Run the focused Python tests and confirm the gaps.
- [ ] **Step 3:** Implement minimal launcher and integration fixes, including safe argument handling and lifecycle receipt validation.
- [ ] **Step 4:** Run package manifest/lifecycle tests and any available native launcher checks.
- [ ] **Step 5:** Commit `feat: harden macOS app lifecycle packaging`.

### Task 5: Browser, RDP, Capture, Clipboard, and Input Qualification

**Files:**
- Modify: `src/browser/native_macos.rs`
- Modify: `src/services/vnc/macos.rs`
- Modify: `src/services/vnc/macos_frame.rs`
- Modify: `src/services/vnc/macos_lifecycle.rs`
- Modify: `engines/rdp/src/printer_macos.rs`
- Modify: `engines/rdp/src/native_file_macos.rs`
- Create/modify: focused platform tests and `docs/parity-evidence/issue-2/providers.md`

**Interfaces:** Existing provider APIs remain unchanged; tests pin bounded frame conversion, permission errors, held-input release, cancellation, clipboard direction, architecture selection, and native RDP provider availability.

- [ ] **Step 1:** Add failing tests for denial/revocation/cancel, late callback ownership, held-key/button release, malformed BGRA frames, clipboard limits, and provider capability reporting.
- [ ] **Step 2:** Run focused tests and confirm expected failures.
- [ ] **Step 3:** Implement only the missing ownership, validation, or error-propagation behavior.
- [ ] **Step 4:** Run focused Rust tests plus the full suite; record physical consent/GPU/device gaps without substituting synthetic passes.
- [ ] **Step 5:** Commit `feat: qualify macOS provider lifecycle contracts`.

### Task 6: Native Architecture and Package Evidence

**Files:**
- Modify: `packaging/macos` build/release scripts as required by Tasks 2–5
- Create: `docs/parity-evidence/issue-2/arm64.md`
- Create: `docs/parity-evidence/issue-2/x86_64.md` when an Intel runner is available
- Modify: `PARITY_CHECKLIST.md`
- Modify: `PARITY_EXECUTION_QUEUE.json`
- Modify: `FEATURE_PARITY.md`
- Modify: `README.md` and `docs/USAGE.md` only for verified support changes

**Interfaces:** Evidence records identify exact source revision, architecture, deployment target, SDK/toolchain, lockfiles, hashes, package contents, and observed workflow results.

- [ ] **Step 1:** Add failing evidence-schema checks for required identity fields and explicit unavailable-fixture states.
- [ ] **Step 2:** Run schema checks and confirm incomplete records fail.
- [ ] **Step 3:** Implement the evidence writer/checker and capture all available native results.
- [ ] **Step 4:** Update issue #2 and repository parity records only for demonstrated implementation/evidence changes.
- [ ] **Step 5:** Commit `docs: record macOS native qualification evidence`.

### Task 7: Final Verification and Issue Closure Readiness

**Files:** All touched files plus issue #2 comment/checklist

- [ ] **Step 1:** Run formatting, focused tests, full locked workspace tests, RDP helper tests, package checks, and available native smoke checks.
- [ ] **Step 2:** Run the final evidence/hash/integrity review and inspect the complete diff.
- [ ] **Step 3:** Update issue #2 with completed items, remaining evidence gaps, exact commands, and artifact identifiers.
- [ ] **Step 4:** Commit final documentation/evidence synchronization.
- [ ] **Step 5:** Perform final whole-branch review before claiming completion.
