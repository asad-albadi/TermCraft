# Terminal subsystem review package

Review T01–T06 in the current app/ checkout. No Git repository, BASE or commit lineage exists; do not invent a diff. This is one read-only subsystem-boundary review, not six per-task reviews or a repeat of F01–F06.

Plan: docs/superpowers/plans/2026-11-23-02-terminal.md. Spec: PARITY_CHECKLIST.md, DESIGN.md and shared parity-contracts plan. Approved master: docs/superpowers/plans/2026-11-23-complete-parity-plan.md. Ledger: PARITY_PROGRESS.md T01–T06 entries/rulings and older deferred findings.

Sources introduced/changed: src/terminal.rs, src/terminal/{unicode,modes,conformance,history,mouse,highlight,selection,paste,settings}.rs; src/process.rs, src/process_input.rs, src/process_unix.rs, src/process_windows.rs, src/serial.rs; src/model.rs; src/ui.rs, src/ui/{pages,terminal_mouse,terminal_selection,terminal_settings,highlighting,session_confirmations}.rs and the active-close handoff in src/ui/remote_editor.rs. Dependencies pinned in Cargo.toml/Cargo.lock. No unrelated implementation or docs changes while review runs.

Tests: tests/terminal_conformance.rs, terminal_search.rs, terminal_mouse.rs, highlighting.rs, terminal_selection.rs, terminal_safety.rs, terminal_settings.rs, process_input.rs, terminal_programs.rs, terminal_performance.rs and new GUI tests in the listed UI modules. Evidence: docs/parity-evidence/T01 through T06. Latest full suite 226 passed / 6 opt-in ignored, both memory/CPU benchmarks explicitly passed. Windows isolated source check passed; full app/native runtime is unavailable. Vim/less real PTY cases default; optional htop explicitly passed under T01.

Review the real production paths, not only tests. Focus on streaming Unicode/VT/wide edits, history ownership/cache/memory, incremental encoding/color source preservation, actual glyph/row geometry, pointer clipping/focus/history/Shift routing, extraction boundaries/expiry, malformed settings and regex limits, clipboard/paste targeting and cancellation, full admission queues, background input/Drop and interaction with existing remote-editor close protections. Test failures discovered during review may be reproduced in owned temporary fixtures; do not mutate source/index or spawn agents.

## Review Focus (verbatim from approved plan)

- Host-key/certificate changes and credential responses must not authenticate the wrong endpoint or leak secrets (F01/F06/S05/D01).
- Remote/local filenames, archives and redirects must not become commands or escape their intended target root (F02/F05/P05/P06/B02).
- Disconnects, full queues and stale generations must preserve user data and cancel without freezing the interface (F01/F04/W02).
- Packaged/native workflows must use declared dependencies and preserve user settings across install/upgrade/remove (P08/B04/B05/V05/V08).
- Uncertain original behavior and absent hardware/machines must remain evidence gaps, never silently excluded scope or passing tests (E01-E05/V01-V08).

Use requesting-code-review/code-reviewer.md. Judge reasonable user expectations even when spec silent; grade by effect. Report Critical/Important/Minor with exact file:line and reproducible input/user impact. List EVERY behavior considered and declined to judge with reason before verdict. Read-only; no source edits, no sub-agents, no second opinion. Executor re-grades, one TDD fix pass for Important/Critical, minors deferred, no re-review.
