# Remaining panes and panels cleanup

**Latest icon correction:** the owner explicitly requested replacing the old glyphs in the session-type selector. All thirteen protocols now use distinct themed vector icons. This is an icon-only exception to the earlier dialog-source hold: layout, form fields, labels, selection and footer behavior stay intact. [Current native dark/light previews](captures/session-protocol-icons/README.md). The local Linux review launcher now selects this build and preserves Wayland. PR #8 remains draft and unmerged.

## Owner correction: terminal controls, separate app bars

The owner clarified that “one row” refers to controls **inside the terminal pane/window**, not the app menus, toolbar or connection tabs. The combined app bar has been reverted: menus/theme and labelled command tiles are separate again, and connection tabs have their own strip. The terminal’s status/title, primary actions and secondary action menu occupy one row above its content. Detached tabs are separate from their one-row window controls. Secondary actions remain reachable without adding permanent rows.

The sidebar Sessions/SFTP tab arrangement, new themed icons and other pane cleanups are retained. Approved session create/edit sources and shared styling remain unchanged. Earlier `05a2234` packages and unified-bar pictures below are historical and do **not** show this correction. PR #8 remains draft and unmerged; owner human checks and explicit merge approval are still required.


Continue the approved issue #4 design: everyday actions and data first, clear task sections, aligned forms, details/help on demand, full-width scroll viewports with content clearance. Preserve all behavior, trust/credential decisions, active-operation cancellation and pending-work protection. The owner explicitly approved session create/edit dialogs; their implementation and shared styling are protected. PR #8 stays draft and unmerged until full human checks and explicit approval.

**Current delivery:** UI source `05a2234` passed all three [platform CI jobs](https://github.com/asad-albadi/TermCraft/actions/runs/46102916112), all three unsigned package jobs and aggregate checks in [the candidate workflow](https://github.com/asad-albadi/TermCraft/actions/runs/46102912942). All six distribution payload hashes, each platform’s 646 Git build inputs and every manifest file were verified. Linux startup, Wayland and lifecycle records bind to its exact archive hash. [Complete build identities and limits](all-pane-builds.json) · [Current native previews](captures/unified-workspace/README.md). Owner acceptance remains open.

Run the current Linux review build after closing an older build:

```bash
"/home/asad/.codex/worktrees/termcraft-issue-3/app/target/issue-4/run-latest-linux"
```

The launcher now points to a newly extracted, verified `05a2234` runtime. Previous runtimes are preserved.

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
- [x] Focused interaction checks, native visual review and consolidated platform validation.
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

## Owner-directed combined bar, sidebar and icon follow-up

The owner subsequently specified **one bar for everything**, explicitly choosing this over separate connection tabs. The app menus, main commands, connection tabs, theme and connection actions now share a single horizontal bar. Narrow windows use a bounded, scrollable command menu instead of extra rows. Browser navigation also belongs to this bar; desktop clipboard controls and the compatibility SFTP commands remain available from connection actions. Clipboard-read cancellation stays immediately available while pending.

The SSH file browser now occupies a tab in the existing left sidebar. Starting or selecting an eligible connection opens its SFTP tab; Sessions remains available alongside it. Switching sidebar tabs retains the connection, directory, selection and operations. Hiding the browser or closing/switching the active connection reconciles the sidebar instead of leaving a second column behind. The transfer/authentication controllers and connection identity are preserved.

The icons use original 24-unit vector drawings, restrained theme accents and the existing Fluent hover/pressed/focus treatment. Command, protocol, transfer and file metaphors follow the familiar roles documented in [MobaXterm’s official guide](https://mobaxterm.mobatek.net/documentation.html), with TermCraft rendering and colors. No MobaXterm bitmap assets were copied. They are applied to the unified bar, navigation, saved connections, remote file commands, tools and matching file/library actions. The approved session create/edit implementation and its existing protocol glyph path remain unchanged.

The sidebar regression was observed RED with the file row in a second column, then GREEN after relocation. The first combined-bar interaction set passed **11/11**, including real FTP editing, theme keyboard selection and command overflow. The broader follow-up run passed **91 checks**, zero failures, one explicitly ignored pinned-archive fixture. Test navigation now follows the consolidated bar and the shared sidebar; original endpoint/file-content, close-protection, selection and timing assertions remain in place. Browser fixture qualification is reported separately from cached layout checks.

The earlier `efddce1` build runs `46100227684` and `46100232997` were cancelled as superseded by this explicit owner steering; they are not claimed successful or used as current human-review packages. New source qualification and native previews are pending at the time of this entry. The owner’s human checks and merge approval remain open.

The final local follow-ups passed **11/11** for the combined-bar/last-browser-close/keyboard/sidebar checks and **8/8** for detached-window actions and transport ownership. An additional run with `TERMCRAFT_REQUIRE_CHROMIUM=1` passed **11/11**, including real Chromium input, redirect/draft reconciliation, queue saturation, resize/capture rejection and last-tab close; this uses the installed owned-test Chromium rather than a production package. The resize fixture now targets DOM pixel 289 directly after the toolbar made the canvas origin pixel-aligned; exact DOM coordinates, viewport widths, stale-input rejection and capture/release counts remain asserted.

Native inspection at 640×480 exposed the horizontal tab scrollbar increasing the bar from 46 to 61 points when a long connection title overflowed. A long-title regression reproduced the 61-point failure before the fix. Tab strips now retain scrolling and keyboard navigation without reserving another chrome row; selection scrolls into view. The regression and surrounding interaction checks passed, and the native minimum-window preview now shows the single 46-point bar. Detached tabs and commands also share one row, with browser/desktop/compatibility SFTP controls available in the correct window’s menu. Full hosted platform/package qualification remains the next gate; no merge is authorized.


## Final package qualification

The three native candidate jobs, all three repeated regression lanes and the aggregate job passed at `05a2234`. Publication was intentionally skipped for the manual branch run; no release or tag was created. Linux’s 1,097, Windows’s 995 and Apple Silicon’s 990 manifest files were verified against the downloaded archives, together with their shared 646-input Git source snapshot. The Mac manifest is located inside `Contents/Resources`, and its file inventory is rooted at the `.app` bundle; static audit uses that actual bundle root. This verification changes no packaged bytes. [Exact hashes and job identities](all-pane-builds.json).

The Linux review launcher was atomically changed to the new qualified runtime after these checks. Its target executable hash matches the archive. Existing binaries, data and profiles were preserved. Native development previews remain explicitly separate from package qualification. The owned capture container was restored to its initial stopped state. The only remaining acceptance gate for this UI work is the owner’s human review and explicit merge approval; PR #8 remains draft and unmerged.
