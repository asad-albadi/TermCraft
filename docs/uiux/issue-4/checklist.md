# Issue #4 component and acceptance checklist

Windows 11 desktop Fluent, retaining the owner’s preference for Fluent 1 over Fluent 2. Starting research and independent source inventories are complete; implementation and runtime evidence are tracked below. An unchecked entry is not accepted. Retained components require a recorded reason and evidence just as changed components do. Native OS-owned surfaces and unobserved hardware/assistive-technology results remain explicit. Overall parity46/60 is unchanged.

**Merge hold:** keep PR #8 open until the owner completes full human checks and explicitly approves merging.

## Implementation batches

| Batch | Status | Evidence |
| --- | --- | --- |
| Research and complete source inventories | Complete | research.md, screen-audit.md, dialog-audit.md |
| 1. Shared foundation | Independently reviewed throughca444ba | Windows 11 geometry correction approved; six current component tests and one actual-model disk round-trip passed; native integration pending |
| 2. Shell/navigation/tabs/splits | Independently reviewed atb77581a | Seventeen actual-source component tests, strict remote-pane loading/ready probe and app type check passed; dedicated name/theme bar removed |
| 3. Dialogs and protocol forms | Independently reviewed through acc3132 | Twenty-five focused production UI/PTY/protocol cases, eight shared components and actual SSH/Local native previews; readable aligned fields, accessible labels, scoped input ownership and safe recovery workflows |
| 4. Specialist pages and web shell | Independently reviewed through 8f4e0ac | Forty-two focused production tests, actual Chromium appearance checks, native previews; folder/file-type icons in local, SSH/SFTP and FTP explorers |
| 5. Integrated visual/interaction verification | In progress | Before captures in before/; no human/native-only acceptance claimed |

The detailed boxes below are acceptance tracking for the integrated interface; a completed implementation batch does not mark unexecuted native checks as passed.

## Foundations and states

- [ ] Light, Dark and System palettes
- [ ] Semantic error, warning, success and disabled states
- [ ] Typography, control heights, field spacing and surface insets
- [ ] Primary/secondary actions: rest, hover, held press, focus and disabled
- [ ] Reduced motion and high-contrast settings
- [ ] Scroll indicators, track hover, thumb drag and content clearance
- [ ] Icon family/stroke/alignment, accessible names and tooltips
- [ ] Explorer icons distinguish folders and common file types consistently in local, SSH/SFTP and FTP listings; unknown files have a clear fallback

## Shell and navigation

- [ ] Title/menu/command bars and overflow
- [ ] Navigation rail: selected, hover, pressed, focus and short heights
- [ ] Quick connect and saved-session search/tree
- [ ] Shared-source and folder/group rows
- [ ] Tabs: selection, title truncation, close hit, focus and context menus
- [ ] Split pane commands, drag handles and close controls
- [ ] Detached window chrome and saved geometry
- [ ] Home, status messages and full-value access

## Session and protocol forms

- [ ] SSH / embedded SFTP / OpenSSH compatibility
- [ ] SSH keys, agent, gateway/proxy and remote environment options
- [ ] Telnet
- [ ] Rsh / Rlogin
- [ ] RDP / gateways / certificate and device options
- [ ] VNC / TLS and clipboard options
- [ ] FTP / FTPS options
- [ ] SFTP options
- [ ] Serial options
- [ ] Local shell options
- [ ] Mosh options
- [ ] Browser/runtime/proxy options
- [ ] XDMCP discovery and X server options
- [ ] Session identity, advanced disclosure, validation and pinned footer
- [ ] Startup macro and inherited terminal settings

## Dialogs, managers and confirmations

- [ ] New/Edit session first focus, cancel and retained draft
- [ ] Global terminal profile and highlighting editors
- [ ] Keyboard shortcuts editor/capture
- [ ] Shared sources manager, setup and sign-in
- [ ] Vault manager and nested secret requests
- [ ] SSH key/agent manager, import/generate/inspection
- [ ] Session import preview, warning and progress
- [ ] Folder/group launch report
- [ ] Local rename/delete/editor lifecycle
- [ ] Remote file actions and capability gates
- [ ] SSH remote editor, recovery comparison and pending-document close
- [ ] FTP editor and independently dismissible recovery comparison
- [ ] Multiline paste confirmation
- [ ] Active session close/reconnect confirmation
- [ ] Detached rename/close ownership
- [ ] Game overwrite/discard prompts
- [ ] Feature parity and Help/About surfaces

## Pages and specialist surfaces

- [ ] Local file list, long paths/names, selection and keyboard actions
- [ ] Tunnels and mode-specific editor
- [ ] Macros, recording, event editing and cancellation
- [ ] Tools: packages/plugins, comparisons, image viewer, network diagnostics
- [ ] Settings: scope/inheritance, clipboard and workspace data/recovery
- [ ] HTTP server controls/status/logs
- [ ] FTP/TFTP/NFS managed servers
- [ ] SSH/SFTP, iPerf and scheduled jobs
- [ ] VNC service, capture permission and viewer controls
- [ ] X server/XDMCP management/status
- [ ] Remote file sidebar, transfer queues, progress and cancel
- [ ] FTP tab browse/transfer/auth/trust controls
- [ ] RDP/VNC certificate/auth/clipboard/fullscreen chrome
- [ ] Browser address/commands, auth origin and secure fields
- [ ] Terminal profile/find/selection/output chrome; raw renderer retained
- [ ] Thirteen games, preset/options/status chrome and board contrast
- [ ] Idle/screensaver controls, privacy and wake ownership
- [ ] App-owned noVNC web form/themes/focus/motion/status
- [ ] Native OS picker/permission chrome and remote/vendor content boundaries

## Verification

- [ ] Real pointer/keyboard state and safe modal isolation tests
- [ ] Nested vault/key/source and mixed SSH+FTP pending-work workflows
- [ ] Long Unicode/path/error values and narrow/resized main/detached layouts
- [ ] System/Light/Dark and 100/150/200% layout/capture coverage
- [ ] Current-source native Linux visual captures and before/after comparisons
- [ ] Required Linux/Windows/Apple Silicon regression lanes
- [ ] Independent per-batch reviews and final whole-branch review
- [ ] Human checks — owner explicitly required before merge


### Dialog batch verification

The dialog batch is independently approved through `acc3132`. Focused tests exercise all 13 protocol forms at normal/compact widths and scale, pinned actions, production accessible field names, safe key/vault nesting, dirty mixed SSH/FTP decisions, unrelated-terminal input, save failure and XDMCP close-frame network isolation with a positive control. Native Linux development previews confirmed readable, aligned SSH and Local form rows. These local checks used cached pre-adapter dependencies; full current distribution checks and human acceptance remain pending. The previous hosted run46003044432 failed two old XDMCP coordinate fixtures on all three platforms; the actual Cancel/Escape replacements pass focused checks, but a fresh full run is required.

### Native matrix after the dialog batch

[Hosted run 46010039731](https://github.com/asad-albadi/TermCraft/actions/runs/46010039731), source `eb439c9`, passed Linux x86-64, Windows x64 and Apple Silicon macOS. The previous two XDMCP fixture failures are resolved without dropping the close-frame network and stale-generation guards. This qualifies the foundation/shell/dialog revision; the subsequent specialist-page batch still requires review and a fresh matrix. Native screen-reader and owner visual acceptance are separate checks.

### Specialist pages and explorer icons

The specialist batch is independently approved through `8f4e0ac`. The combined correction passed 42 focused production tests, including real FTP virtual-row scrolling/selection, readable game labels and validation, actual service-field accessible names, remote focus after a listing reorders, and file-type icon geometry, labels and metadata clearance across all three explorers. New icons use the existing drawing style and available filename/kind metadata. They do not read file content, resolve links or add network requests. The final native matrix and integrated acceptance remain pending.

A mixed-language private filename fixture exposed a pre-existing Arabic/CJK glyph fallback limit. The final verification batch will assess existing fallback coverage and record script/shaping limits explicitly; filename geometry tests alone do not qualify glyph coverage.
