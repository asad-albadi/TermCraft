# Issue #4 component and acceptance checklist

Windows 11 desktop Fluent, retaining the owner’s preference for Fluent 1 over Fluent 2. The boxes in the component sections record software disposition. Each corresponding route, retained behavior and level of evidence appears in the [validation record](validation.md). The verification section tracks independent gates. Human acceptance is outstanding. Feature parity remains 46/60.

**Human review rejected the current build on 2026-11-27.** The previous checks below record technical work on that build, not acceptance. [Correction checklist](human-review-correction.md) is active: simplify session creation and terminal/sidebar controls, provide help on demand, fix viewport scrollbars and organize settings/games/detached layouts. These items must be reviewed again before a new human-check build is offered.

**Merge hold:** keep PR #8 open until the owner completes full human checks and explicitly approves merging.

## Implementation batches

| Batch | Status | Evidence |
| --- | --- | --- |
| Research and complete source inventories | Complete | research.md, screen-audit.md, dialog-audit.md |
| 1. Shared foundation | Independently reviewed through `ca444ba` | Windows 11 geometry correction approved; six current component tests and one actual-model disk round-trip passed; adapter-enabled matrix tracked below |
| 2. Shell/navigation/tabs/splits | Independently reviewed at `b77581a` | Seventeen actual-source component tests, strict remote-pane loading/ready probe and app type check passed; dedicated name/theme bar removed |
| 3. Dialogs and protocol forms | Independently reviewed through acc3132 | Twenty-five focused production UI/PTY/protocol cases, eight shared components and actual SSH/Local native previews; readable aligned fields, accessible labels, scoped input ownership and safe recovery workflows |
| 4. Specialist pages and web shell | Independently reviewed through 8f4e0ac | Forty-two focused production tests, actual Chromium appearance checks, native previews; folder/file-type icons in local, SSH/SFTP and FTP explorers |
| 5. Integrated visual/interaction verification | Local evidence collected | [Validation record](validation.md), [original screenshots](captures/manifest.json) and [baseline](before/manifest.json). Hosted and human gates below |

A checked component has a recorded change or retention decision. Consult the validation record for the distinction between source inspection, actual production interaction and native pixels.

## Foundations and states

- [x] Light, Dark and System palettes
- [x] Semantic error, warning, success and disabled states
- [x] Typography, control heights, field spacing and surface insets
- [x] Primary/secondary actions: rest, hover, held press, focus and disabled
- [x] Reduced motion and high-contrast settings
- [ ] Scroll indicators, track hover, thumb drag and content clearance
- [x] Icon family/stroke/alignment, accessible names and tooltips
- [x] Explorer icons distinguish folders and common file types consistently in local, SSH/SFTP and FTP listings; unknown files have a clear fallback

## Shell and navigation

- [x] Title/menu/command bars and overflow
- [x] Navigation rail: selected, hover, pressed, focus and short heights
- [ ] Quick connect and saved-session search/tree
- [ ] Shared-source and folder/group rows
- [x] Tabs: selection, title truncation, close hit, focus and context menus
- [ ] Split pane commands, drag handles and close controls
- [ ] Detached window chrome and saved geometry
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
- [ ] Session identity, advanced disclosure, validation and pinned footer
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


### Current-source checks

The integrated Linux suite on `c2dded6` passed 61 production cases. The separate shared-component suite passed all ten cases. It used the existing pre-adapter graphics dependencies. The native screenshots cover the three theme selections and three real window scales on an isolated X11 display. The browser checks exercised the actual app-owned noVNC assets in Chromium. See the [capture manifest](captures/manifest.json) and [validation record](validation.md) for exact scope.

[Run 46010039731](https://github.com/asad-albadi/TermCraft/actions/runs/46010039731) passed the earlier dialog source `eb439c9` on Linux x86-64, Windows x64 and Apple Silicon macOS. The [corrected CI run](https://github.com/asad-albadi/TermCraft/actions/runs/46023041969) passed on all three platforms. All three regression jobs, all three native package jobs and the aggregate inventory check passed in the [unsigned build](https://github.com/asad-albadi/TermCraft/actions/runs/46023047742). The [preceding candidate build](https://github.com/asad-albadi/TermCraft/actions/runs/46018225239) for `36ba3ba` was canceled and supplied no accepted packages.

The detached-client correction includes a real native-child regression and [Light](captures/detached-light-100.png) and [Dark](captures/detached-dark-100.png) evidence at 360×260. The History label measured 15.23:1 and 15.62:1 respectively, up from 1.35:1 in the original System/Light capture.

- [x] Packaged Linux visual checks and [full-HD gallery](../../../shots/README.md), including detached contrast, session form and filename glyph fallback.
