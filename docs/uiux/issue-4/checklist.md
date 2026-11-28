# Issue #4 component and acceptance checklist

**Games and remote desktop follow-up:** all thirteen boards now center and fit both window dimensions. Embedded desktops default to Inherit pane size, retain fixed resolution, negotiate supported remote resizing, and reduce local input/frame batching. [Behavior, validation and review command](games-and-desktop-resizing.md). PR #8 remains draft and unmerged pending owner human checks.

**RDP review fix:** the review directory now includes its verified, matching RDP helper. Development builds prefer the helper beside the GUI, and dedicated `TC-RDP-119` / `120` messages distinguish missing local support from a local launch failure. Actual frame/input, certificate-change and shutdown checks passed against a current local endpoint. [Details](error-messages.md#rdp-review-build-correction). The launcher is updated; PR #8 remains draft and unmerged.

**Latest app-wide error pass:** application-owned failure surfaces use stable `TC-…` codes, readable explanations, suggested next steps and explicit copying of redacted technical detail. Errors retain their feature context; ordinary progress, successful actions and cancellation remain ordinary messages. A real SSH cause-loss bug was fixed in TCP error reporting and post-exit resizing. Approved session dialog structure is retained, with only error feedback changed for this request. [Guide, coverage and code catalogue](error-messages.md). PR #8 remains draft and unmerged pending owner checks.

**Latest terminal correction:** focused cursors now blink on a 600 ms half-cycle, restart visible on input/focus/movement, respect remote hiding and stay steady with reduced motion. JetBrains Mono Nerd Font Mono is bundled as the default for unspecified profiles and as the icon fallback for existing font choices; explicitly saved choices are preserved. Font registration/choice are owner-authorized exceptions to the earlier font-source hold. Dialog organization remains unchanged. [Native symbol and cursor evidence](captures/terminal-font-cursor/README.md). The same local review launcher now selects this build. PR #8 stays draft and unmerged.

**Latest icon correction:** the owner explicitly requested replacing the old glyphs in the session-type selector. All thirteen protocols now use distinct themed vector icons. This is an icon-only exception to the earlier dialog-source hold: layout, form fields, labels, selection and footer behavior stay intact. [Current native dark/light previews](captures/session-protocol-icons/README.md). The local Linux review launcher now selects this build and preserves Wayland. PR #8 remains draft and unmerged.

## Owner correction: terminal controls, separate app bars

The owner clarified that “one row” refers to controls **inside the terminal pane/window**, not the app menus, toolbar or connection tabs. The combined app bar has been reverted: menus/theme and labelled command tiles are separate again, and connection tabs have their own strip. The terminal’s status/title, primary actions and secondary action menu occupy one row above its content. Detached tabs are separate from their one-row window controls. Secondary actions remain reachable without adding permanent rows.

The sidebar Sessions/SFTP tab arrangement, new themed icons and other pane cleanups are retained. Approved session create/edit sources and shared styling remain unchanged. Earlier `05a2234` packages and unified-bar pictures below are historical and do **not** show this correction. PR #8 remains draft and unmerged; owner human checks and explicit merge approval are still required.


Windows 11 desktop Fluent, retaining the owner’s preference for Fluent 1 over Fluent 2. The boxes in the component sections record software disposition. Each corresponding route, retained behavior and level of evidence appears in the [validation record](validation.md). The verification section tracks independent gates. Human acceptance is outstanding. Feature parity remains 46/60.

**Human review rejected an earlier build on 2026-11-27.** The previous checks below record technical work on that build, not acceptance. [Correction checklist](human-review-correction.md) is active: simplify session creation and terminal/sidebar controls, provide help on demand, fix viewport scrollbars and organize settings/games/detached layouts. The corrected implementation, scoped independent review and final three-platform CI/package checks are recorded in that checklist. Owner acceptance remains open.


**Current remaining-pane pass:** source `05a2234` reviews all other panes, combines menus/commands/connection tabs/theme/actions in one bar, moves SFTP into the shared left sidebar and applies the TermCraft icon set. The approved session create/edit sources and shared styling remain unchanged. [Current inventory and dispositions](all-pane-cleanup.md) · [Current native previews and limits](captures/unified-workspace/README.md). All three source-platform CI jobs, all three unsigned package jobs and aggregate verification passed. [Exact new build identities](all-pane-builds.json) and the [current Linux command](all-pane-cleanup.md) are recorded. Owner human checks and merge approval remain open.

**Previous pane-build record:** `db07f66` UI and the `8f121b2` test-only correction retain their actual [historical identities](pane-builds.json). They are not relabelled as the new UI. Earlier aggregate skips/failures retain their original outcomes.

**Merge hold:** keep PR #8 open until the owner completes full human checks and explicitly approves merging.

## Implementation batches

| Batch | Status | Evidence |
| --- | --- | --- |
| Research and complete source inventories | Complete | research.md, screen-audit.md, dialog-audit.md |
| 1. Shared foundation | Independently reviewed through `ca444ba` | Windows 11 geometry correction approved; six current component tests and one actual-model disk round-trip passed; adapter-enabled matrix tracked below |
| 2. Shell/navigation/tabs/splits | Independently reviewed at `b77581a` | Seventeen actual-source component tests, strict remote-pane loading/ready probe and app type check passed; dedicated name/theme bar removed |
| 3. Dialogs and protocol forms | Independently reviewed through acc3132 | Twenty-five focused production UI/PTY/protocol cases, eight shared components and actual SSH/Local native previews; readable aligned fields, accessible labels, scoped input ownership and safe recovery workflows |
| 4. Specialist pages and web shell | Independently reviewed through 8f4e0ac | Forty-two focused production tests, actual Chromium appearance checks, native previews; folder/file-type icons in local, SSH/SFTP and FTP explorers |
| Human-review correction | Source `72db3f4`, reviewed guidance fix `8c50a01` | 74 focused cases plus seven overlapping covering cases; native normal/ultrawide/compact/detached captures; independent review approved. Full CI additionally caught the shared read-only heading; `6e58b3c` restores it and passes nine covering cases. Final source `df5dbb8` passed all three native CI lanes and package jobs; [build identities](human-builds.json). Owner acceptance pending |
| Remaining panes and combined bar | Source `05a2234`; all three source CI jobs passed | [Inventory](all-pane-cleanup.md), 91 scoped cases/one explicit archive-fixture skip, overlapping targeted bar/detached/real-browser checks, [native gallery](captures/unified-workspace/manifest.json); all three native packages and aggregate passed with verified source/payload hashes; [build identities](all-pane-builds.json), owner acceptance open |
| 5. Integrated visual/interaction verification | Local evidence collected | [Validation record](validation.md), [original screenshots](captures/manifest.json) and [baseline](before/manifest.json). Hosted and human gates below |

A checked component has a recorded change or retention decision. Consult the validation record for the distinction between source inspection, actual production interaction and native pixels.

## Foundations and states

- [x] Light, Dark and System palettes
- [x] Semantic error, warning, success and disabled states
- [x] Typography, control heights, field spacing and surface insets
- [x] Primary/secondary actions: rest, hover, held press, focus and disabled
- [x] Reduced motion and high-contrast settings
- [x] Scroll indicators, track hover, thumb drag and content clearance
- [x] Icon family/stroke/alignment, accessible names and tooltips
- [x] Explorer icons distinguish folders and common file types consistently in local, SSH/SFTP and FTP listings; unknown files have a clear fallback

## Shell and navigation

- [x] Title/menu/command bars and overflow
- [x] Navigation rail: selected, hover, pressed, focus and short heights
- [x] Quick connect and saved-session search/tree
- [x] Shared-source and folder/group rows
- [x] Tabs: selection, title truncation, close hit, focus and context menus
- [x] Split pane commands, drag handles and close controls
- [x] Detached window chrome and saved geometry
- [x] Home, status messages and full-value access

## Session and protocol forms

- [x] SSH / embedded SFTP / OpenSSH compatibility
- [x] SSH keys, agent, gateway/proxy and remote environment options
- [x] Telnet
- [x] Rsh / Rlogin
- [x] RDP / gateways / certificate and device options
- [x] VNC / TLS and clipboard options
- [x] FTP / FTPS options
- [x] SFTP options
- [x] Serial options
- [x] Local shell options
- [x] Mosh options
- [x] Browser/runtime/proxy options
- [x] XDMCP discovery and X server options
- [x] Session identity, advanced disclosure, validation and pinned footer
- [x] Startup macro and inherited terminal settings

## Dialogs, managers and confirmations

- [x] New/Edit session first focus, cancel and retained draft
- [x] Global terminal profile and highlighting editors
- [x] Keyboard shortcuts editor/capture
- [x] Shared sources manager, setup and sign-in
- [x] Vault manager and nested secret requests
- [x] SSH key/agent manager, import/generate/inspection
- [x] Session import preview, warning and progress
- [x] Folder/group launch report
- [x] Local rename/delete/editor lifecycle
- [x] Remote file actions and capability gates
- [x] SSH remote editor, recovery comparison and pending-document close
- [x] FTP editor and independently dismissible recovery comparison
- [x] Multiline paste confirmation
- [x] Active session close/reconnect confirmation
- [x] Detached rename/close ownership
- [x] Game overwrite/discard prompts
- [x] Feature parity and Help/About surfaces

## Pages and specialist surfaces

- [x] Local file list, long paths/names, selection and keyboard actions
- [x] Tunnels and mode-specific editor
- [x] Macros, recording, event editing and cancellation
- [x] Tools: packages/plugins, comparisons, image viewer, network diagnostics
- [x] Settings: scope/inheritance, clipboard and workspace data/recovery
- [x] HTTP server controls/status/logs
- [x] FTP/TFTP/NFS managed servers
- [x] SSH/SFTP, iPerf and scheduled jobs
- [x] VNC service, capture permission and viewer controls
- [x] X server/XDMCP management/status
- [x] Remote file sidebar, transfer queues, progress and cancel
- [x] FTP tab browse/transfer/auth/trust controls
- [x] RDP/VNC certificate/auth/clipboard/fullscreen chrome
- [x] Browser address/commands, auth origin and secure fields
- [x] Terminal profile/find/selection/output chrome; raw renderer retained
- [x] Thirteen games, preset/options/status chrome and board contrast
- [x] Idle/screensaver controls, privacy and wake ownership
- [x] App-owned noVNC web form/themes/focus/motion/status
- [x] Native OS picker/permission chrome and remote/vendor content boundaries

## Verification

- [x] Real pointer/keyboard state and safe modal isolation tests
- [x] Nested vault/key/source and mixed SSH+FTP pending-work workflows
- [x] Long Unicode/path/error values and narrow/resized main/detached layouts
- [x] System/Light/Dark and 100/150/200% layout/capture coverage
- [x] Current-source native Linux development visual captures and before/after comparisons
- [x] Required Linux x86-64, Windows x64 and Apple Silicon macOS regression lanes at `c2dded6`
- [x] Independent implementation reviews and final whole-branch source review. The optional-font correction in `36ba3ba` passed scoped review. The subsequent evidence review identified a detached History contrast defect. The correction and its native evidence passed scoped review at `5575601`.
- [x] Final technical review approved the specification, code quality, distribution evidence and platform packages for `c2dded6`.
- [ ] Human checks — owner explicitly required before merge


### Earlier implementation checks (`c2dded6`)

The integrated Linux suite on `c2dded6` passed 61 production cases. The separate shared-component suite passed all ten cases. It used the existing pre-adapter graphics dependencies. The native screenshots cover the three theme selections and three real window scales on an isolated X11 display. The browser checks exercised the actual app-owned noVNC assets in Chromium. See the [capture manifest](captures/manifest.json) and [validation record](validation.md) for exact scope.

[Run 46010039731](https://github.com/asad-albadi/TermCraft/actions/runs/46010039731) passed the earlier dialog source `eb439c9` on Linux x86-64, Windows x64 and Apple Silicon macOS. The [corrected CI run](https://github.com/asad-albadi/TermCraft/actions/runs/46023041969) passed on all three platforms. All three regression jobs, all three native package jobs and the aggregate inventory check passed in the [unsigned build](https://github.com/asad-albadi/TermCraft/actions/runs/46023047742). The [preceding candidate build](https://github.com/asad-albadi/TermCraft/actions/runs/46018225239) for `36ba3ba` was canceled and supplied no accepted packages.

The detached-client correction includes a real native-child regression and [Light](captures/detached-light-100.png) and [Dark](captures/detached-dark-100.png) evidence at 360×260. The History label measured 15.23:1 and 15.62:1 respectively, up from 1.35:1 in the original System/Light capture.

- [x] Packaged Linux visual checks and [full-HD gallery](../../../shots/README.md), including detached contrast, session form and filename glyph fallback.

### Final correction checks (`df5dbb8`)

- [x] Final correction source passed Linux x86-64, Windows x64 and Apple Silicon macOS CI.
- [x] Separate unsigned-build regression matrix passed on all three platforms.
- [x] All three native core package startup/lifecycle jobs passed; downloaded payloads match their source and SHA-256 inventories.
- [x] Linux AppImage, corresponding-source, X11, native Wayland and Debian/per-user lifecycle checks passed.
- [x] Combined three-platform package inventory passed; publication was skipped.
- [x] Local Linux launcher points to the exact verified final core package; earlier runtime directories remain intact.
- [ ] Owner completes the [corrected-build review](human-review-correction.md#owner-review-of-the-corrected-build) and explicitly approves merging PR #8.

See [exact build identities](human-builds.json) and [final correction evidence](human-review-correction.md#final-automated-platform-checks). The one Mac vault-wait timeout, its unchanged-source successful rerun and the independent successful package regression are recorded there. This UI correction does not increase the recorded 46/60 feature-parity count.

### Latest pane refinement (`db07f66`, test-only follow-up `8f121b2`)

- [x] File list uses available pane height; setup/preferences/help are on demand.
- [x] Fixed-size navigation prevents idle width drift; compact footer and narrow-terminal overflow fit their viewports.
- [x] Actual row-menu target, raw-path safeguards, capability gates, overwrite visibility and active transfer cancellation retained.
- [x] Original real-SSH native-drop privacy/upload assertions pass against the new controls.
- [x] Twenty-one focused checks and current three-platform CI passed; all three package jobs passed and all six distribution hashes were verified.
- [x] Linux launcher updated to the exact verified UI package; seven development capture identities and limitations recorded.
- [ ] Owner accepts the refined panes and explicitly approves merging.

Package and test source identities remain distinct in [pane-builds.json](pane-builds.json). This does not claim a successful aggregate release workflow or change the recorded 46/60 feature-parity count.
