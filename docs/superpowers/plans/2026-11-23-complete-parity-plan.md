> Current execution status(2026-11-25):46/60implementation checkpoints(76.7%), strict native/original0/60. Reviewed local fixes and representative independent-server cells pass; current source32e512 has986passing tests/0failures/73ignored and genuine Linux full plus Debian12 core/revision3 qualification. Remaining native/equipment/original dependencies and POSIX architecture limits are explicit in [the authoritative queue](../../../PARITY_EXECUTION_QUEUE.json) and [local closure report](../../parity-evidence/execution/2026-11-25-local-completion.md). Initial planned-status tables below are historical inventory, not current execution state.

# Complete TermCraft Parity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Plan and implement all 60 remaining Personal 26.6 parity outcomes, by actual dependencies, with honest completion evidence.

**Architecture:** Keep the existing Rust/egui workspace while introducing focused embedded transports, remote-file services, terminal/workspace models, packaged runtime engines and independent evidence runners. Embedded SSH/SFTP is user-selected. The user approved the full plan for inline execution on 2026-11-23; no external-client launcher is silently promoted to embedded parity.

**Tech Stack:** Rust edition 2024, eframe/egui, serde JSON, russh/russh-sftp asynchronous SSH/SFTP, candidate IronRDP/vnc-rs desktop engines, packaged X.Org-family engines, and target-native validation tools.

**Spec:** [Checklist](../../../PARITY_CHECKLIST.md), [project design](../../../DESIGN.md), [approved F01 design](../specs/2026-11-23-remote-files-design.md).

**Status:** Complete planning coverage for 60/60 points; **0/60 implemented or verified by this planning work**. The user approved this full plan and selected inline implementation. This was the planning baseline; current implementation/evidence is tracked in PARITY_PROGRESS.md.

## Global Constraints

- Deliverables stay inside `app/`; Rust edition 2024 with eframe/egui remains the application stack.
- Preserve old workspace JSON, penguin branding, emerald light/dark palettes and correct System-theme behavior.
- Treat installed-client adapters as partial until the required integrated behavior is implemented and verified.
- Credentials stay out of ordinary session JSON, logs and exports; never disable host/certificate checks to make a test pass.
- Network/protocol/file work stays off the egui render thread; cancellation and bounded queues are required.
- Keep full Personal 26.6 scope; Professional-only and uncertain requirements need evidence before inclusion/exclusion.
- No checklist outcome is completed by a plan, screenshot, cross-target compilation, or unavailable native/hardware test.

## Review Focus

- Host-key/certificate changes and credential responses must not authenticate the wrong endpoint or leak secrets (F01/F06/S05/D01).
- Remote/local filenames, archives and redirects must not become commands or escape their intended target root (F02/F05/P05/P06/B02).
- Disconnects, full queues and stale generations must preserve user data and cancel without freezing the interface (F01/F04/W02).
- Packaged/native workflows must use declared dependencies and preserve user settings across install/upgrade/remove (P08/B04/B05/V05/V08).
- Uncertain original behavior and absent hardware/machines must remain evidence gaps, never silently excluded scope or passing tests (E01-E05/V01-V08).

## Execution method and ordering

Recommended: native implementation in this chat, one task at a time, with an independent review at subsystem boundaries. A subagent-driven method is available if selected. The requested progress updates apply during execution; this plan does not schedule background work or claim autonomous progress after the chat stops.

Execution amendment (2026-11-24, explicit user instruction): use PARITY_EXECUTION_QUEUE.json as the single durable queue and work by actual dependencies. The coverage table is an inventory, not a serial barrier. Continue D03 using D01 while unfinished D02 providers remain open. Small passing changes and rebuild_check Ready do not end execution. Batch full/host checks at subsystem boundaries. Each point can contain multiple red/green increments. Build the minimum shared primitives inside the first task that needs them; later tasks extend them rather than introducing a backward dependency. Evidence probes E01-E05/V07 may be needed early as preflight work, but their final outcomes remain separately tracked.

Implementation status and verified parity status are distinct. A finished code increment can remain Partial or Awaiting validation until its native/original-runtime evidence is present. Do not stop all independent development because one native machine is unavailable, and do not check off that missing evidence. Keep the blocker, next action and affected point in the progress log.

S03 introduces group resolution before S04; shortcut execution is verified after S04 provides dispatch. X01 can introduce its package manifest contract before P08 finalizes installers. F01 owns its minimal direct/jump/trusted-X11 option preservation; X02 later adds automatic bundled-server lifecycle. P05/P06/B06 cannot claim original-format/game equivalence before E04/V07 evidence.

## Plan set

| Order | Subsystem | Points |
| --- | --- | --- |
| 01 | [Remote files and embedded SSH/SFTP](2026-11-23-01-remote-files.md) | F01, F02, F03, F04, F05, F06 |
| 02 | [Terminal conformance and interaction](2026-11-23-02-terminal.md) | T01, T02, T03, T04, T05, T06 |
| 03 | [Layouts, tabs, MultiExec and shortcuts](2026-11-23-03-workspace.md) | W01, W02, W03, W04, W05 |
| 04 | [Session migration, automation and credentials](2026-11-23-04-sessions-auth.md) | S01, S02, S03, S04, S05, S06, S07 |
| 05 | [Embedded RDP, VNC and SSH gateways](2026-11-23-05-remote-desktops.md) | D01, D02, D03, D04 |
| 06 | [Bundled X server and remote Unix desktops](2026-11-23-06-x11.md) | X01, X02, X03, X04, X05 |
| 07 | [Protocol completeness, packages, plugins and releases](2026-11-23-07-protocols-tools.md) | P01, P02, P03, P04, P05, P06, P07, P08 |
| 08 | [Servers, desktop integration and games](2026-11-23-08-services-platform.md) | B01, B02, B03, B04, B05, B06 |
| 09 | [Evidence and edition-sensitive scope decisions](2026-11-23-09-scope-evidence.md) | E01, E02, E03, E04, E05 |
| 10 | [Endpoint, platform and original-runtime validation](2026-11-23-10-validation.md) | V01, V02, V03, V04, V05, V06, V07, V08 |

## Shared contracts

Read [Parity contracts](2026-11-23-parity-contracts.md) before any subsystem plan. It defines transport IDs/events, remote paths, queue limits, owned fixtures, evidence records, backwards compatibility and completion rules. Test assertion fragments in the subsystem plans are specifications for real observations, not executable tests that have already passed.

## Coverage and progress

| Point | Implementation / evidence task | Planning status |
| --- | --- | --- |
| F01 | [Automatic graphical remote directory pane](2026-11-23-01-remote-files.md#task-f01-automatic-graphical-remote-directory-pane) | Implementation checkpoint; validation pending |
| F02 | [Graphical transfers and remote file management](2026-11-23-01-remote-files.md#task-f02-graphical-transfers-and-remote-file-management) | Implementation checkpoint; validation pending |
| F03 | [Follow the remote shell directory](2026-11-23-01-remote-files.md#task-f03-follow-the-remote-shell-directory) | Implementation checkpoint; validation pending |
| F04 | [Edit remote files and upload saved changes](2026-11-23-01-remote-files.md#task-f04-edit-remote-files-and-upload-saved-changes) | Implementation checkpoint; validation pending |
| F05 | [SCP browser compatibility](2026-11-23-01-remote-files.md#task-f05-scp-browser-compatibility) | Implementation checkpoint; validation pending |
| F06 | [Authentication reuse and reconnect coordination](2026-11-23-01-remote-files.md#task-f06-authentication-reuse-and-reconnect-coordination) | Implementation checkpoint; validation pending |
| T01 | [Terminal conformance and long-session performance](2026-11-23-02-terminal.md#task-t01-terminal-conformance-and-long-session-performance) | Reviewed implementation checkpoint; validation pending |
| T02 | [Terminal mouse reporting](2026-11-23-02-terminal.md#task-t02-terminal-mouse-reporting) | Reviewed implementation checkpoint; validation pending |
| T03 | [Configurable terminal keyword highlighting](2026-11-23-02-terminal.md#task-t03-configurable-terminal-keyword-highlighting) | Reviewed implementation checkpoint; validation pending |
| T04 | [Selection and configurable clipboard interaction](2026-11-23-02-terminal.md#task-t04-selection-and-configurable-clipboard-interaction) | Reviewed implementation checkpoint; validation pending |
| T05 | [Paste and active-session close controls](2026-11-23-02-terminal.md#task-t05-paste-and-active-session-close-controls) | Reviewed implementation checkpoint; validation pending |
| T06 | [Global and per-session terminal settings](2026-11-23-02-terminal.md#task-t06-global-and-per-session-terminal-settings) | Reviewed implementation checkpoint; validation pending |
| W01 | [Vertical and four-terminal layouts](2026-11-23-03-workspace.md#task-w01-vertical-and-four-terminal-layouts) | Reviewed implementation checkpoint; native/original validation pending |
| W02 | [Detached tabs, rename and fullscreen](2026-11-23-03-workspace.md#task-w02-detached-tabs-rename-and-fullscreen) | Reviewed implementation checkpoint; native/original validation pending |
| W03 | [MultiExec target management](2026-11-23-03-workspace.md#task-w03-multiexec-target-management) | Reviewed implementation checkpoint; native/original validation pending |
| W04 | [Configurable keyboard shortcuts](2026-11-23-03-workspace.md#task-w04-configurable-keyboard-shortcuts) | Reviewed implementation checkpoint; native/original validation pending |
| W05 | [Unread activity indicators](2026-11-23-03-workspace.md#task-w05-unread-activity-indicators) | Reviewed implementation checkpoint; native/original validation pending |
| S01 | [MobaXterm and PuTTY import](2026-11-23-04-sessions-auth.md#task-s01-mobaxterm-and-putty-import) | Implementation checkpoint; native/original format validation pending |
| S02 | [Shared session sources](2026-11-23-04-sessions-auth.md#task-s02-shared-session-sources) | Implementation checkpoint; native/original and vault integration pending |
| S03 | [Session groups and launch shortcuts](2026-11-23-04-sessions-auth.md#task-s03-session-groups-and-launch-shortcuts) | Implementation checkpoint; native/original validation pending |
| S04 | [CLI commands and existing-instance dispatch](2026-11-23-04-sessions-auth.md#task-s04-cli-commands-and-existing-instance-dispatch) | Implementation checkpoint; native/original validation pending |
| S05 | [Opt-in encrypted credential vault](2026-11-23-04-sessions-auth.md#task-s05-opt-in-encrypted-credential-vault) | Implementation checkpoint; native ACL/UI/power-loss validation pending |
| S06 | [SSH key and agent management](2026-11-23-04-sessions-auth.md#task-s06-ssh-key-and-agent-management) | Implementation checkpoint; native agent/UI/format validation pending |
| S07 | [Advanced SSH and proxy settings](2026-11-23-04-sessions-auth.md#task-s07-advanced-ssh-and-proxy-settings) | Planned; not implemented |
| D01 | [Embedded RDP view](2026-11-23-05-remote-desktops.md#task-d01-embedded-rdp-view) | Planned; not implemented |
| D02 | [RDP settings and device redirection](2026-11-23-05-remote-desktops.md#task-d02-rdp-settings-and-device-redirection) | Planned; not implemented |
| D03 | [Embedded VNC view](2026-11-23-05-remote-desktops.md#task-d03-embedded-vnc-view) | Planned; not implemented |
| D04 | [SSH gateway for desktop and Telnet sessions](2026-11-23-05-remote-desktops.md#task-d04-ssh-gateway-for-desktop-and-telnet-sessions) | Planned; not implemented |
| X01 | [Integrated packaged X server](2026-11-23-06-x11.md#task-x01-integrated-packaged-x-server) | Planned; not implemented |
| X02 | [X11 startup, DISPLAY and authority integration](2026-11-23-06-x11.md#task-x02-x11-startup-display-and-authority-integration) | Planned; not implemented |
| X03 | [X extension and rendering evidence](2026-11-23-06-x11.md#task-x03-x-extension-and-rendering-evidence) | Planned; not implemented |
| X04 | [Complete XDMCP workflow](2026-11-23-06-x11.md#task-x04-complete-xdmcp-workflow) | Planned; not implemented |
| X05 | [Remote desktop environment through SSH](2026-11-23-06-x11.md#task-x05-remote-desktop-environment-through-ssh) | Planned; not implemented |
| P01 | [Bundled protocol client runtime](2026-11-23-07-protocols-tools.md#task-p01-bundled-protocol-client-runtime) | Planned; not implemented |
| P02 | [FTP/FTPS graphical transport](2026-11-23-07-protocols-tools.md#task-p02-ftpftps-graphical-transport) | Planned; not implemented |
| P03 | [Telnet, Rlogin and Rsh transport options](2026-11-23-07-protocols-tools.md#task-p03-telnet-rlogin-and-rsh-transport-options) | Planned; not implemented |
| P04 | [Complete serial line settings](2026-11-23-07-protocols-tools.md#task-p04-complete-serial-line-settings) | Planned; not implemented |
| P05 | [Portable Unix environment and package manager](2026-11-23-07-protocols-tools.md#task-p05-portable-unix-environment-and-package-manager) | Planned; not implemented |
| P06 | [Plugins and extension lifecycle](2026-11-23-07-protocols-tools.md#task-p06-plugins-and-extension-lifecycle) | Planned; not implemented |
| P07 | [Missing graphical tools](2026-11-23-07-protocols-tools.md#task-p07-missing-graphical-tools) | Planned; not implemented |
| P08 | [Distributable applications and release verification](2026-11-23-07-protocols-tools.md#task-p08-distributable-applications-and-release-verification) | Planned; not implemented |
| B01 | [FTP, TFTP and Telnet servers](2026-11-23-08-services-platform.md#task-b01-ftp-tftp-and-telnet-servers) | Planned; not implemented |
| B02 | [Complete HTTP file sharing](2026-11-23-08-services-platform.md#task-b02-complete-http-file-sharing) | Planned; not implemented |
| B03 | [SSH/SFTP and Iperf service management](2026-11-23-08-services-platform.md#task-b03-sshsftp-and-iperf-service-management) | Planned; not implemented |
| B04 | [Windows Explorer, paths and registry integration](2026-11-23-08-services-platform.md#task-b04-windows-explorer-paths-and-registry-integration) | Planned; not implemented |
| B05 | [Linux and macOS desktop integration](2026-11-23-08-services-platform.md#task-b05-linux-and-macos-desktop-integration) | Planned; not implemented |
| B06 | [Original games/screensaver equivalence](2026-11-23-08-services-platform.md#task-b06-original-gamesscreensaver-equivalence) | Planned; not implemented |
| E01 | [Determine general-purpose embedded browser requirement](2026-11-23-09-scope-evidence.md#task-e01-determine-general-purpose-embedded-browser-requirement) | Planned; not implemented |
| E02 | [Confirm macro timing, scheduling and graphical scope](2026-11-23-09-scope-evidence.md#task-e02-confirm-macro-timing-scheduling-and-graphical-scope) | Planned; not implemented |
| E03 | [Confirm additional server modes and controls](2026-11-23-09-scope-evidence.md#task-e03-confirm-additional-server-modes-and-controls) | Planned; not implemented |
| E04 | [Confirm original limits, defaults, imports and runtime formats](2026-11-23-09-scope-evidence.md#task-e04-confirm-original-limits-defaults-imports-and-runtime-formats) | Planned; not implemented |
| E05 | [Personal versus Professional scope](2026-11-23-09-scope-evidence.md#task-e05-personal-versus-professional-scope) | Planned; not implemented |
| V01 | [SSH/SFTP interoperability matrix](2026-11-23-10-validation.md#task-v01-sshsftp-interoperability-matrix) | Planned; not implemented |
| V02 | [Forwarding and tunnel interoperability](2026-11-23-10-validation.md#task-v02-forwarding-and-tunnel-interoperability) | Planned; not implemented |
| V03 | [Other protocol endpoint matrix](2026-11-23-10-validation.md#task-v03-other-protocol-endpoint-matrix) | Planned; not implemented |
| V04 | [Mosh roaming and physical serial evidence](2026-11-23-10-validation.md#task-v04-mosh-roaming-and-physical-serial-evidence) | Planned; not implemented |
| V05 | [Native Windows and macOS workflows](2026-11-23-10-validation.md#task-v05-native-windows-and-macos-workflows) | Planned; not implemented |
| V06 | [Linux X11/Wayland, scale, clipboard and live theme](2026-11-23-10-validation.md#task-v06-linux-x11wayland-scale-clipboard-and-live-theme) | Planned; not implemented |
| V07 | [Original Personal 26.6 comparison](2026-11-23-10-validation.md#task-v07-original-personal-266-comparison) | Planned; not implemented |
| V08 | [Clean-machine packaging, upgrades and persistence](2026-11-23-10-validation.md#task-v08-clean-machine-packaging-upgrades-and-persistence) | Reviewed implementation checkpoint; native/original validation pending |

## Execution record per point

After every verified increment, update [PARITY_PROGRESS.md](../../../PARITY_PROGRESS.md) with: point ID; increment; changed files; observed RED command/result; GREEN targeted/full-suite commands/results; independent-review findings; native/original evidence; remaining blockers; next action. Keep [PARITY_CHECKLIST.md](../../../PARITY_CHECKLIST.md) and [FEATURE_PARITY.md](../../../FEATURE_PARITY.md) consistent.

## Standard verification commands

Run from `app/` after relevant code changes:

```sh
cargo fmt --check
cargo test --offline
cargo check --offline --target x86_64-pc-windows-msvc
cargo run --offline -- --screenshot shots/parity-progress.png --data-dir data/parity-screenshot
```

Native Windows/macOS execution, real endpoint/hardware runs and original captures are additional required evidence. Cross-compilation, nonblank screenshots and parser tests do not replace them. Every introduced integration target runs all of its task cases before the full suite.

## Candidate and research decisions

- F01: the user selected an embedded SSH/SFTP architecture. Research found session-wide blocking in the initial ssh2 candidate and a public async/X11/forwarding surface in russh; the approved plan selects russh/russh-sftp. Pin exact versions only after capability/target-build checks.
- D01/D03: IronRDP and vnc-rs are candidates, not evidence of complete device/security/encoding support. Confirm the required matrix before engine adoption; preserve unsupported capability reports.
- X01: package verified platform engines and source/license manifests; do not require a user-installed engine and call bundling done. No global X server/authority changes.
- S05: use maintained KDF/authenticated-encryption libraries; do not invent cryptography. New secrets remain separate from portable connection definitions.
- E01-E05: determine ambiguous browser/macro/service/edition/default behavior from evidence. If a requirement is confirmed, add and execute its concrete implementation extension; an evidence report alone does not satisfy the confirmed feature.
- B06: original game catalog is not supplied. Its inventory is a defined prerequisite; this plan does not replace unknown games with arbitrary examples.

## External prerequisites and honest limits

- Designated Windows/macOS native runners and supported architectures.
- Exact Personal 26.6 original runtime/captures for equivalence, edition/default and game questions.
- Controlled RDP/VNC/XDMCP/legacy/FTPS/Mosh endpoints; isolated local fixtures where possible.
- Physical serial adapters/wiring and a designated roaming network test environment.
- Native capture/device access, redistribution inputs, and signing/notarization credentials where applicable.

Provision test-owned local fixtures autonomously within the authorized implementation. Ask only for prerequisites that cannot be obtained from the project/environment; never change the user network, remote services, devices or system installations based on a guessed fixture. Releases/publishing and secret-bearing signing actions remain separately controlled.

## Self-review checklist

- [x] Every current checklist ID appears once, in order, in the coverage table and one task section.
- [x] Each task declares files, interfaces/deliverables, dependencies, implementation/observation steps and acceptance observations.
- [x] Shared interfaces, queue limits, ownership and test-fixture boundaries are documented.
- [x] Backward dependencies and native/hardware/original-evidence prerequisites are explicit.
- [x] No point is marked implemented or verified by planning.
- [x] Source documents and internal plan links have been checked.

## Review and execution checkpoint

Review this master and the linked subsystem plans. Select native or subagent-driven execution and approve or revise the plan before implementation, as required by Superpowers writing-plans. This plan intentionally covers all points now; subsequent discoveries may refine a subsystem with a recorded decision rather than silently changing scope.

## Technical references

- [russh](https://docs.rs/russh/latest/russh/) and [russh-sftp](https://docs.rs/russh-sftp/latest/russh_sftp/).
- [IronRDP upstream](https://github.com/Devolutions/IronRDP).
- [vnc-rs upstream](https://github.com/HsuJv/vnc-rs).
- [Xephyr upstream overview](https://www.freedesktop.org/projects/Xephyr/) and [VcXsrv source](https://github.com/marchaesen/vcxsrv).
- [MobaXterm documentation](https://mobaxterm.mobatek.net/documentation.html) and [features](https://mobaxterm.mobatek.net/features.html).
