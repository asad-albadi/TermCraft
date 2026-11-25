# macOS Native Implementation and Delivery Design

## Intent

Issue #2 completes TermCraft's macOS implementation and delivery path for Apple Silicon and Intel. The goal is a relocatable, app-owned workspace whose advertised macOS features are backed by native builds and evidence, while unsupported workflows remain explicit and gated.

## Goals

- Build and package the GUI, locked RDP helper, browser payload, native providers, and private XQuartz runtime for arm64 and x86_64.
- Preserve the existing provider boundaries, trust gates, cancellation ownership, permission checks, and user-data separation.
- Make `.app` relocation, Finder/URL/Services events, registration/unregistration, upgrade/removal, and helper/resource adjacency testable.
- Record exact sources, architectures, SDK/toolchain inputs, hashes, runtime dependencies, and native qualification results.
- Keep issue #2, `PARITY_CHECKLIST.md`, `PARITY_EXECUTION_QUEUE.json`, `FEATURE_PARITY.md`, and evidence records synchronized.

## Non-goals

- Claiming native qualification from Linux tests, cross-compilation, source inspection, or synthetic permission tests alone.
- Replacing the embedded providers with system applications or installing/reconfiguring system XQuartz.
- Inferring unsupported Composite, GPU, monitor, hardware, serial, or original-runtime behavior.
- Making signing or notarization a prerequisite for unsigned development qualification.

## Constraints

- The private XQuartz engine must use the pinned source contract, inherited authenticated listeners, private runtime paths, relocated dependencies, and no upstream install hooks.
- Native artifacts are accepted only through compiled trust pins and complete file/dependency manifests.
- Permission denial, revocation, cancellation, held-input release, and cleanup must be observable and bounded.
- User settings, vault data, recovery files, unrelated LaunchServices handlers, and defaults must survive package lifecycle operations.

## Approaches

1. **Platform rewrite:** replace existing seams with a new macOS-specific stack. Rejected because it would duplicate reviewed ownership and security boundaries.
2. **Evidence-only packaging:** document the existing source candidates without enabling them. Rejected because it cannot satisfy the issue's implementation and delivery goals.
3. **Contract-first completion (selected):** add missing tests and packaging contracts around the existing macOS modules, build/verify exact native artifacts where the host allows, and enable only verified outputs. This preserves the current architecture and keeps unsupported evidence honest.

## Delivery slices

1. Baseline/build reproducibility and macOS artifact manifest contracts.
2. Private XQuartz source preparation, relocation, signing, and trust-pin integration.
3. App packaging, Finder/URL/Services lifecycle, relocation, and user-data preservation.
4. Native browser, RDP/device, screen capture/control, clipboard, and permission qualification seams.
5. Native arm64 execution plus Intel/physical/original evidence capture where available.
6. Final parity documentation, issue checklist synchronization, and release-artifact handoff.

## Completion rule

A checklist item is checked only when its own acceptance evidence exists. Unavailable architecture, hardware, consent, GPU, network, or original-runtime fixtures remain open with a precise evidence gap and next action.
