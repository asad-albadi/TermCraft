# Issue #4 — Fluent 1 consistency and usability design

## Authority and outcome

The owner explicitly requests all of issue #4, research first, Fluent **1 rather than 2**, with **Windows 11 desktop controls** as the visual reference (the owner explicitly corrected the initial Windows 10 interpretation), continuous implementation until done. Preserve TermCraft's identity and current workflows; this is a refinement of the existing Rust/egui interface. Research and the complete starting inventories are in `docs/uiux/issue-4/{research,screen-audit,dialog-audit}.md`.

## Scope and invariants

- Linux x86-64, Windows x64 and Apple Silicon macOS arm64 only; no toolkit migration, Intel macOS or Android work.
- Preserve terminal rendering, remote input semantics, trust/authentication binding, encrypted secrets, dirty-document and overwrite safeguards, transfer cancellation/recovery and persistence. Modal presentation must not bypass or deadlock these workflows.
- Preserve penguin branding, emerald accent, established command/session organization, stable IDs and user settings. Do not inflate the 46/60 parity count.
- Use Windows 11 desktop geometry and interaction references; do not substitute a Windows 10/UWP look or migrate to a Fluent 2 web toolkit. Existing egui controls should inherit the shared style; custom controls must expose equivalent pointer, keyboard and accessible states.
- No obsolete native Reveal API or invented acrylic support: use bounded Reveal-inspired feedback and solid material fallback in existing egui. Do not add idle animation loops, delay commands or shift hit targets.
- User research is qualitative, not a prevalence claim. Improve discoverability of inherited settings, clipboard rules and connection status without silently changing those semantics.
- Every Cargo invocation uses `CARGO_BUILD_JOBS=2`. Locked dependency resolution and native CI must remain meaningful; missing fixtures or headless rendering are not native accessibility/hardware evidence.

## Shared design system

Extract the two existing theme variants into a focused `src/ui/fluent.rs` module. Use semantic surface/text/accent/error/warning/success/border tokens and a 4-point spacing grid. Ordinary controls are at least 32 effective pixels tall; 14-point body, 12-point helper, 20-point dialog title and 28-point page title form the hierarchy. Maintain readable keyboard focus and 4.5:1 body-text contrast. Use the documented Windows 11 geometry: 4 effective pixels for ordinary controls/in-page backplates, 8 for floating dialogs/flyouts, and square touching/docked edges. Use coherent shadows for transient depth and retain solid material fallbacks where native Mica/Acrylic is unavailable; do not claim a tint is native material.

Buttons distinguish rest/hover/pressed/focus/disabled; navigation additionally distinguishes selection after pointer departure. Use reusable painting helpers for custom controls. Primary actions keep their semantics but receive real state colors and focus outlines. Menus inherit standard egui keyboard behavior and the same surface family. Thin resting scroll indicators widen on hover/drag, reserve content clearance and respect reduced motion. Settings expose reduced motion and high-contrast options with backward-compatible defaults; support available platform preference integration where the existing backend supplies it, and explicitly document any unavailable automatic detection. Do not claim screen-reader compatibility unless the backend is enabled and verified within the stated environment.

## Shell and navigation

Keep the menu/command organization and rail/session tree. Per the owner’s subsequent instruction, remove the redundant full-width app-name/theme header and place the compact theme picker in the existing menu row; the native window title and existing branding retain app identity. Add responsive wrapping or safe overflow to commands, proper accessible names to icon-only controls, keyboard focus distinct from selection, and pressed state. Saved-session activation must support keyboard as well as double click. Local-file rows must expose names/state and clip long filenames before the size column. Tabs retain separate close-hit routing and terminal ownership; close buttons get equivalent focus and pressed paint. Long statuses remain discoverable. Preserve sidebar width persistence only after genuine user resize.

## Dialogs and forms

Use a shared bounded presentation helper rather than indiscriminately changing every Window into a Modal. Blocking confirmations and authentication use actual modal interception with an explicit safe action; modeless documents and comparison tools remain usable together. Keep the action footer outside the scrolling content, viewport-bound dimensions, wrapping errors and long identities. Prefer primary action(s) then safe Cancel/Close, according to the UWP ContentDialog reference. First focus goes to the first meaningful field or safe cancel for destructive/trust decisions; restore invoking focus after dismissal. Escape means safe cancellation only, never implicit trust, overwrite, discard or publication. Enter respects the focused action and existing validation; no blanket destructive default.

The new/edit session form is the priority: retain every protocol branch and advanced option, group identity/connection/advanced settings clearly, preserve drafts on validation errors and keep Save/Connect/Cancel visible. Nested key/vault/source flows must remain reachable and correctly layered. Fix shortcut/raw-input leakage only for genuinely blocked owners; do not freeze independent modeless documents or unrelated session work.

## Pages and specialist surfaces

Audit and correct every listed screen, including Tools, Settings, Servers, protocol settings, packages/plugins, transfer/remote file panes, SSH/FTP editors/recovery, remote desktop/browser chrome, games and idle chrome. Use shared headings, spacing, wrapping command rows and bounded scrolling. Improve action icons where useful without inventing a new family or altering vendor-owned content. Global, per-session and per-tab terminal settings must clearly identify scope/inheritance. Keep exact trust/permission warnings and cancellable background work prominent. The app-owned noVNC web shell receives matching accessible controls, light/dark and reduced-motion treatment; vendor code and the remote framebuffer remain unchanged.

## Validation and completion

Maintain the screen/component inventory and a component-state checklist with each reviewed surface's disposition (fixed or retained) and evidence. Exercise real pointer/keyboard state changes, modal isolation and safe dismissal, long fields, narrow/main/detached layouts and existing credential/transport/dirty-state regressions. Capture before/after app output and test logical layouts at 100/150/200% with System/Light/Dark. Record native Linux and hosted Windows/macOS results distinctly from synthetic egui layouts. Native screen-reader, GPU, permission and physical scaling checks that cannot be observed remain explicit platform acceptance, not fabricated passes. Update issue #4 and a PR continuously; do not stop between implementation batches. No actual tag/release publication is involved.
