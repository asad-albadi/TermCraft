# Remaining panes and panels cleanup

Continue the approved issue #4 design: everyday actions and data first, clear task sections, aligned forms, details/help on demand, full-width scroll viewports with content clearance. Preserve all behavior, trust/credential decisions, active-operation cancellation and pending-work protection. The owner explicitly approved session create/edit dialogs; their implementation and shared styling are protected. PR #8 stays draft and unmerged until full human checks and explicit approval.

## Working inventory

- [x] Home, menu/command bands, navigation rail and saved-session sidebar.
- [x] Local file navigation/actions/filter/list and local editor.
- [x] Existing refined SSH/SFTP/SCP pane and transfer states.
- [x] FTP/FTPS browser, transfers, editor and recovery.
- [x] Tunnel library and tunnel controls.
- [x] Macro library, recording/playback controls and event editing.
- [x] Tools: diagnostics, comparisons, images, TCP checks and capture.
- [x] Packages: portable Unix and legacy review/installation.
- [x] Plugins: installed list, reviewed consent and active operations.
- [x] Runtime/provider information and Windows platform tools.
- [x] Servers: HTTP, SSH/SFTP, FTP, TFTP, NFS, Telnet, VNC, Iperf, Cron and X server.
- [x] Settings: appearance, terminal, input/safety, workspace backups and About.
- [x] Embedded RDP/VNC and browser chrome; keep trust/authentication prompts explicit.
- [x] Remote document commands, recovery details and pending-close decisions.
- [x] Game catalog/play/options, idle screen and detached windows.
- [x] Vault, key/agent, shared-source, import, shortcuts and highlighting/profile managers: inspect and retain already-clean protected workflows.
- [x] Verify protected session dialog sources remain unchanged.
- [ ] Focused interaction checks, native visual review and consolidated platform validation.
- [ ] Owner completes human checks; no merge before explicit approval.

A checked inventory entry must record either a concrete correction or an inspected retention decision below. Runtime, synthetic-layout and actual endpoint evidence remain distinct.

## Changes and inspected retention decisions

- Home, command bands and the saved-session sidebar retain the compact layout from the preceding correction: quick connect, search, tree, a session overflow menu and on-demand help. Native review at 1360×880 confirms the main viewport gutter sits at its right edge. X server entry points now select the X server configuration directly rather than the last unrelated service.
- Local files now shows navigation, Refresh, New folder, More and filter above the list. Copy, paste, rename and delete retain their original callbacks in More. Existing file icons, raw-path identity, editor and pending-change confirmations are retained.
- SSH/SFTP/SCP retains the previously refined address bar, primary transfer commands, contextual secondary actions, Downloads footer and on-demand shell/help settings. Its trust, authentication, conflicting-file and deletion prompts are unchanged.
- FTP/FTPS now puts primary upload/download/edit actions above a list that uses the remaining height. Secondary mutation commands are in More; pending operations and transfer results have reserved room. Transport security remains explicit. Native FTP editing and SSH/FTP pending-close tests exercise actual endpoints separately from layout previews.
- Tunnels and macros reserve width for actions and elide long identity text without losing the full tooltip. Start/Run stay primary; Edit/Remove move to More. Macro creation and import/export are grouped, with recording/playback controls retained outside menus.
- Tools separates Network, Files, Packages, Plugins and Runtime, plus Windows tools on Windows. Comparisons and image viewing show one selected task. Polling continues when changing tool categories. Packages opens its own category directly. The redundant Plugins expander was removed after the pointer workflow exposed the duplicate name.
- Portable and original package workflows retain their distinct collapsible review/install flows within Packages. Checksums, review/consent, cancellation and publication safeguards are retained. Plugin consent and invocation controls retain their existing safety boundaries.
- Runtime keeps its factual provider table in an explicit advanced category. Windows integration/registry controls remain scoped to their own category; read-only defaults and before/after write confirmation are retained. Background Windows operations continue to be polled.
- Servers shows one chosen service among HTTP, SSH/SFTP, FTP, TFTP, NFS, Telnet, VNC, Iperf, scheduled commands and X server. Active services are marked in the selector and keep running when changing views. Configuration drafts remain in the same controllers. Long protocol/engine/scheduler explanations move to bounded help menus. Encryption, shell authority, command ownership and clipboard/access consent remain explicit. Logs are expandable; active state, errors and Stop/Cancel remain visible.
- Settings retains its five task categories. Input/safety now has separate paste/session and clipboard/selection cards. Terminal memory guidance is a tooltip. About now points to the actual qualification checklists and Tools → Runtime, replacing outdated blanket platform claims. Backup privacy information and damaged-store recovery safeguards remain visible.
- Embedded RDP/VNC and browser chrome retain their compact navigation and explicit certificate/authentication controls. Clipboard guidance is a tooltip; clipboard actions and readiness remain visible. Provider-owned display/browser content keeps its own renderer.
- SSH and FTP document editors show file identity and state first; optional recovery paths/transport mechanics move to Document details. Failures, warnings, conflicts, replacement confirmation, dirty-close decisions and cancellation remain visible. Recovery comparison remains available.
- Game catalog/play/options, idle display and detached windows retain their earlier responsive layouts and contextual Help/More menus. Existing tests cover narrow browser/game controls, all thirteen boards, detached ownership and dirty-game/session close decisions.
- Vault, key/agent, shared-source, import, shortcut, highlighting and profile managers retain their bounded bodies, visible footers and explicit sensitive-operation decisions. Key/agent explanatory paragraphs move to help menus. Session create/edit and the protected shared styling are unchanged.

## Validation in progress

The first task-section regressions were observed failing before implementation and passing after it. An initial broader run passed 79 cases and exposed one Plugins navigation failure; one unrelated portable-package fixture was explicitly ignored because its pinned archive is absent locally. The redundant expander was corrected. A fresh consolidated run, final native review, protected-source verification and the consolidated hosted platform/package batch remain in progress. These are not yet claimed complete. PR #8 remains draft and unmerged; owner human acceptance remains open.

The corrected broader scoped run passed **84 checks**, zero failures, one explicitly ignored pinned-archive package fixture. The subsequent picker/plugin/accessibility run passed **9/9**, zero skips, including the folder-row regression observed RED beforehand. Native short-width inspection motivated the HTTP/SSH/other service picker width reservation; shared session styles were not changed. These local runs use the documented verified cache and do not replace hosted native-adapter/platform qualification.
