# Issue #4 Fluent 1 implementation plan

> **For agentic workers:** Use subagent-driven-development, one implementation writer at a time, with scoped independent reviews and one final whole-branch review. Continue through all tasks without milestone approval pauses.

**Goal:** Resolve app-owned Fluent 1 layout, state, form and accessibility inconsistencies across TermCraft.

**Architecture:** Consolidate existing styling and presentation helpers, then apply them to the shell, dialogs and specialist screens. Preserve egui and all transport/storage/safety contracts. Keep changes attributable to an audited defect or missing interaction state.

**Tech Stack:** Rust, egui/eframe 0.36.2, existing native capture fixtures, HTML/CSS for the app-owned VNC shell.

**Spec:** `docs/superpowers/specs/2026-11-27-issue-4-fluent-1-design.md`

## Global constraints

- Linux x86-64, Windows x64, Apple Silicon macOS arm64 only. Windows 11 desktop Fluent, retaining the owner’s preference over Fluent 2; preserve emerald/penguin identity and established organization.
- Preserve terminal/remote input, trust/authentication, vault secrecy, cancellation, dirty/overwrite/recovery behavior, stable IDs and user settings.
- Every Cargo invocation uses `CARGO_BUILD_JOBS=2`, locked dependencies and isolated user/test data.
- No assertions weakened, prerequisites skipped as qualification, claims of unobserved native behavior, or parity-count increases.
- One implementation writer at a time. Keep reports/briefs under the plan-owned SDD workspace; source audits/research remain in docs/uiux/issue-4.

## Review focus

- Narrow/large-scale content and long Unicode/path/error values keep all required actions reachable.
- Nested key/vault/authentication and SSH+FTP pending documents cannot deadlock through modal layering.
- A focused terminal/desktop/game receives no leaked keys/clicks while its blocking decision owns input.
- Hover, held press, keyboard focus, persistent selection and disabled states remain distinct without moving hit targets.
- New appearance options preserve old settings and System theme; effects have reduced-motion/contrast fallbacks.

## Task 1 — Shared Fluent 1 controls and appearance

Files: create src/ui/fluent.rs; modify src/ui.rs theme/bootstrap/shared helpers, src/model.rs appearance preferences, src/ui/pages.rs appearance-settings section as needed; Cargo.toml/Cargo.lock only if the accessibility backend demonstrably requires it.

- [x] Implement semantic palettes, typography, spacing, geometry, menu/window materials, scroll indicator behavior and bounded feedback motion.
- [x] Correct primary/button/field feedback and custom focus helper; provide bounded dialog/form helpers consumed by later tasks.
- [x] Add backward-compatible reduced-motion/high-contrast settings, enable/inspect available accessibility backend rather than assume screen-reader support.
- [x] Meaningful state/layout/contrast/persistence tests, focused locked gate, self-review, commit and independent task review.

Interfaces: expose documented theme setup, semantic colors, focused/hover/pressed paint, bounded dialog dimensions/body and form sizing. Preserve existing helper wrappers where callers depend on them.

## Task 2 — Shell, navigation, tabs and split chrome

Files: src/ui.rs shell/sidebar/tabs, src/ui/{layout,windows,shared_sessions,session_groups,shortcuts}.rs where navigation/owner behavior lives.

- [x] Apply shared states to rail, command tiles, session rows, tabs/close and split chrome; accessible names and keyboard activation.
- [x] Responsive toolbars/sidebar/status and long-title behavior, preserving actual drag-only width persistence and close-hit separation. Remove the redundant name/theme header per owner steering; integrate the theme picker into the existing menu row.
- [x] Real pointer/keyboard selection/press/disabled/close workflows and focused regressions, commit and review.

## Task 3 — All dialogs, protocol forms and safety ownership

Files: every actual Window/Modal listed in docs/uiux/issue-4/dialog-audit.md, especially src/ui/pages.rs, credentials, ssh_keys, session_import, shared_sessions, session_confirmations, terminal_settings, highlighting, remote_files, remote_editor, ftp, windows and games/play. Do not touch test-only Owned overlay as a production dialog.

- [x] Fix session form first: all 13 protocols, grouped content, adaptive sizing, preserved draft, viewport budget and fixed actions.
- [x] Migrate audited blocking prompts to bounded modal shells; retain modeless document/recovery comparisons and coordinate nested flows.
- [x] Normalize titles/padding/actions/inline validation, initial focus/Escape/restore, long content and persistent footers across the complete dialog inventory.
- [x] Test actual modal input isolation, safe cancellation, nested key/vault, dirty SSH+FTP close, detached ownership and protocol form visibility; preserve existing safety regressions.
- [x] Commit and independent review, correcting findings before moving on.

## Task 4 — Pages, embedded tools, games and web shell

Files: audited page/protocol/service/remote/game modules; assets/vnc-browser/index.html, viewer.css, viewer.js only app-owned chrome.

- [x] Fix local files clipping/keyboard metadata, Tools scrolling, form sizing/wrapping, headings and semantic statuses across remaining surfaces.
- [x] Clarify global/session/tab appearance inheritance and clipboard behavior without changing protocol defaults.
- [x] Normalize remote files selection/transfer controls, browser/desktop/FTP/editor chrome and service/package/plugin reviews.
- [x] Fix game shared chrome/contrast/responsive controls while preserving board logic and idle privacy; Fluent 1 noVNC controls with accessibility/theme/motion fallbacks.
- [x] Meaningful existing and covering interaction regressions, commit and independent review.

## Task 5 — Complete inventory, native visual checks and integration

Files: docs/uiux/issue-4/*, focused UI regression/capture harness as necessary, updated screenshots/README only after current-source native build.

- [ ] Mark every inventoried app-owned surface fixed or retained with evidence; no unreviewed nested screens.
- [ ] Native Linux before/after captures; System/Light/Dark, 100/150/200% logical/native scale, narrow/main/detached layouts and long values. Keep physical/native-platform limits explicit.
- [ ] Run appropriate full regression/native hosted matrix; diagnose and repair failures without weakened tests or skipped gates.
- [ ] One broad final review, one consolidated fix wave and scoped re-review; resolve all material findings.
- [ ] Update issue #4 and PR with checked items, exact source/tests/captures and remaining native-only acceptance. Keep parity46/60. No public tag/release.
