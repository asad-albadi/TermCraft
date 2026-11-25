# W01–W05 workspace subsystem review

Read-only independent review. This supplied app directory has no Git repository; there is no base/head or diff to inspect. Review the current production files and relevant tests against the approved plan, and inspect adjacent transport/confirmation code for interactions. Do not claim a native/original workflow passed based on model or headless GUI tests.

Plan: docs/superpowers/plans/2026-11-23-03-workspace.md
Spec: PARITY_CHECKLIST.md W01–W05, DESIGN.md, shared docs/superpowers/plans/2026-11-23-parity-contracts.md.
Ledger/rulings: PARITY_PROGRESS.md workspace entries after “W01 start and workspace interface pre-flight”. User approved all 60 points and inline execution; this is one review at the workspace subsystem boundary, not per task.

## Implemented scope

- Stable terminal-ID layout tree; Single/two-column/two-row/four presets, dragged ratios persisted through old-compatible Preferences, minimum sizes with explicit small-window single-pane fallback. Runtime transports stay in Workspace.tabs; layout/window registries contain IDs only.
- Immediate egui detached viewports borrow those same owners, with embedded fallback on unsupported backends. Context menu detach/reattach/title override; titles remain separate from OSC state and transfer on reconnect. Fullscreen is per window with saved available geometry restored after native acknowledgement. Close goes through existing remote-document and active-session confirmations; cancelling preserves the detached session.
- Explicit MultiExec selected membership and failures, paged four-pane target view, no implicit enrollment of new tabs. Accepted source keyboard bytes go once, targets each once. Only selected-source input broadcasts; unselected source stays local. Paste/async clipboard snapshot selected IDs, with existing all-target validation; auth/file/protocol/mouse paths stay separate.
- Persisted conflict-aware ShortcutMap, capture/unbind/default editor, context resolution, per-window tab navigation/guarded close/fullscreen, local history controls and selection clipboard/search. Platform command normalization; actual TextEdit/auth capture contexts; native OS clipboard events retain their platform semantics. Associated text from custom modifier chords does not enter the terminal. Capture/dialog guards prevent typed input behind modal configuration. Authentication is scoped to the focused auth field/own connection rather than globally preventing other connected terminals from working.
- Parsed-screen visual signature, separate unread/error/running flags; output-only changes create unread, visible painted main/detached panes mark seen, minimized/occluded viewport metadata excludes them. Protocol queries and unfinished parse fragments do not count. Tabs use accessible text/tooltips, independently of existing running dots. Signature work is bounded by the capped live grid and shared 8-KiB output tick, never history cloning; native latency certification is still pending.

## Files

Production: src/workspace/{layout,windows,broadcast,shortcuts,activity}.rs, src/ui/{layout,windows,broadcast,shortcuts,activity,terminal_output,terminal_selection,session_confirmations}.rs; integration in src/ui.rs, src/ui/pages.rs, src/model.rs, src/lib.rs, src/terminal.rs.
Tests: tests/{workspace_layout,workspace_windows,multiexec,shortcuts,tab_activity}.rs; inline GUI tests in the UI modules; existing terminal/F01–F06 regressions.
Logs: docs/parity-evidence/W01 through W05. Final W05 suite and Windows source-check logs are copied there after completion. Windows source check compiles actual model/workspace/terminal/ConPTY/serial/input/filesystem modules, not the whole app (full target is blocked by ring SDK headers). Native Windows/macOS compositor and actual original comparison remain unavailable. Linux PTYs/owned local SSH/SFTP fixtures exercise real transports in the default suite.

## Review Focus (verbatim from approved plan)

- Host-key/certificate changes and credential responses must not authenticate the wrong endpoint or leak secrets (F01/F06/S05/D01).
- Remote/local filenames, archives and redirects must not become commands or escape their intended target root (F02/F05/P05/P06/B02).
- Disconnects, full queues and stale generations must preserve user data and cancel without freezing the interface (F01/F04/W02).
- Packaged/native workflows must use declared dependencies and preserve user settings across install/upgrade/remove (P08/B04/B05/V05/V08).
- Uncertain original behavior and absent hardware/machines must remain evidence gaps, never silently excluded scope or passing tests (E01-E05/V01-V08).

Spec silence is not permission to violate reasonable user expectations. Return specific file:line, trigger, impact, and severity (Critical/Important/Minor); verify the code path before flagging. List every behavior considered and declined to judge, with reason. Do all review yourself; no nested reviewers. No mutations or new implementation. You may run bounded read-only checks; if running tests use CARGO_BUILD_JOBS=2 (parallel linking caused Btrfs reservation pressure). One subsequent fix pass, no re-review, is planned. Do not treat evidence gaps already labelled pending as falsely claimed passing cases; report any actual overclaims.
