# TermCraft → MobaXterm parity checklist

Reviewed: 2026-11-23. Baseline: the supplied MobaXterm Personal 26.6 target, TermCraft's current source, and MobaXterm's public documentation. Public documentation is not pinned to 26.6; version-sensitive requirements need original-runtime confirmation.

This began as the 2026-11-23 gap inventory. Current reviewed implementation and evidence are recorded in PARITY_PROGRESS.md and FEATURE_PARITY.md; 46/60 implementation checkpoints (76.7%) are present, while full native/original parity remains unverified. Unchecked outcomes retain their outstanding scope rather than erasing implemented subsets.

Execution status and evidence: [PARITY_PROGRESS.md](PARITY_PROGRESS.md).

Current platform scope: Linux x86-64, Windows x64 and Apple Silicon macOS (arm64). Intel macOS is excluded at this stage; remaining macOS requirements apply to arm64. See [support scope](docs/SUPPORT.md). This scope reduction does not itself advance the 46/60 implementation count.

Current local qualification(2026-11-25): source32e512;986tests pass,0fail,73ignored. Current genuine Linux full and Debian12 core/revision3 builds,37actual GUI client checks and125source-bound server checks are qualified within their documented scopes. [Local closure and exact remaining dependencies](docs/parity-evidence/execution/2026-11-25-local-completion.md). The14 remaining implementation checkpoints include native/equipment/original requirements and documented POSIX architecture limits. After publication, the first hosted CI run also exposed an RDP allocation-cleanup failure; D02 investigation is actionable again and tracked in [issue #3](https://github.com/asad-albadi/TermCraft/issues/3).

## How to read this checklist

- **Missing:** no equivalent TermCraft workflow was found.
- **Partial:** some functionality exists, but the item describes what remains.
- **Verify:** code exists or a requirement is inherited, but evidence is insufficient.
- **P1:** core remote-work workflows. **P2:** broader compatibility. **P3:** remaining suite features and platform integrations.

An unchecked item stays open until its stated outcome is demonstrated. Availability through an installed external client does not establish equivalent integrated behavior. Use [FEATURE_PARITY.md](FEATURE_PARITY.md) for the existing capability matrix and [MECHANICS.md](MECHANICS.md) for original investigation evidence.

## P1 — Remote files

The original integrates graphical transfer and remote editing with SSH. The original baseline used a separate interactive SFTP tab; the current embedded pane and outstanding validation are recorded in the progress log. [MobaXterm feature reference](https://mobaxterm.mobatek.net/features.html)

- [ ] **F01 · Partial:** Automatically open a graphical remote directory pane with an SSH connection.
- [ ] **F02 · Partial:** Browse and manage remote entries; support graphical upload/download and drag-and-drop, including folders.
- [ ] **F03 · Partial:** Follow the active shell directory, with an explicit fallback when synchronization is unavailable.
- [ ] **F04 · Partial:** Open a remote file in an editor and upload saved changes; report failed saves and preserve recoverable edits.
- [ ] **F05 · Partial:** Offer SCP browser compatibility for endpoints that cannot use SFTP; make the chosen transport visible.
- [ ] **F06 · Partial:** Coordinate SSH and transfer authentication; handle repeated prompts and reconnects predictably rather than relying on independent terminal sessions.

**Current implementation:** embedded SSH/SFTP, graphical remote files and transfer coordination are implemented. The original interactive-client observation is historical; current code and remaining native/original evidence are tracked in [PARITY_PROGRESS.md](PARITY_PROGRESS.md).

## P1 — Terminal and workspace

The terminal implementation supports common VT operations, two-pane splits, search, export, and logging. Remaining documented interaction gaps are listed below. [MobaXterm terminal and interface documentation](https://mobaxterm.mobatek.net/documentation.html)

- [ ] **T01 · Partial:** Complete terminal conformance: Unicode cell widths, combining text, resizing, scrolling regions, alternate-screen state, and advanced escape modes. Exercise full-screen terminal programs.
- [ ] **T02 · Partial:** Forward terminal mouse events in the application modes requested by terminal programs.
- [ ] **T03 · Partial:** Add configurable keyword/syntax highlighting; ANSI colors and search highlighting alone do not supply it.
- [ ] **T04 · Partial:** Add terminal-specific selection and clipboard behavior, including rectangular selection and configurable right-click paste.
- [ ] **T05 · Partial:** Add configurable multiline-paste confirmation, paste delay, and protection against accidentally closing an active session.
- [ ] **T06 · Partial:** Expose terminal font family, color customization, character encoding, and keyboard options per session as well as globally.
- [ ] **W01 · Partial:** Add vertical and four-terminal split layouts beyond the existing two-column view.
- [ ] **W02 · Partial:** Detach and reattach terminal tabs; support session-tab rename and fullscreen workflows.
- [ ] **W03 · Partial:** Expand MultiExec from input broadcast/two-pane display to a manageable view of multiple target terminals.
- [ ] **W04 · Partial:** Make keyboard shortcuts configurable and complete terminal scroll/navigation controls.
- [ ] **W05 · Partial:** Indicate new output in inactive tabs; the current running-process dot does not indicate unread activity.

**Primary code:** `src/terminal.rs`, `src/ui.rs`, `src/ui/pages.rs`. Validate performance with long scrollback as part of terminal work; current default retention is 360,000 lines.

## P1 — Sessions and authentication

Saved sessions, folders, JSON import/export, keys, and SSH jump hosts already exist. These checklist items cover migration, shared use, and missing app-level management. [Session and password features](https://mobaxterm.mobatek.net/features.html)

- [ ] **S01 · Partial:** Import MobaXterm session/configuration files and PuTTY sessions; preserve supported settings and report unsupported ones.
- [ ] **S02 · Partial (implementation checkpoint; native/original validation pending):** Load shared session definitions without overwriting local sessions; define refresh, read-only, and conflict behavior.
- [ ] **S03 · Partial (implementation checkpoint; native/original validation pending):** Launch a session folder as a group and create shortcuts for saved sessions/groups.
- [ ] **S04 · Partial (implementation checkpoint; native/original validation pending):** Add command-line operations for opening a bookmark, running a command/macro, choosing a starting folder, and opening a tab in an existing instance. Current flags only cover help, screenshots, and the data directory.
- [ ] **S05 · Partial (implementation checkpoint; native/security evidence pending):** Add opt-in encrypted credential storage, master-password management, and credential deletion. Saving remains opt-in; ordinary settings contain opaque references only.
- [ ] **S06 · Partial (implementation checkpoint; native/original validation pending):** Add app-level SSH key/agent management. Key bundles, fingerprints, protected import and owned/system agent management are implemented for the tested Linux path; native Windows/macOS and broader format/algorithm evidence remain pending.
- [ ] **S07 · Partial (reviewed implementation checkpoint; native/original validation pending):** Expose global/session timers, HTTP CONNECT/SOCKS5 proxies, bounded jump chains and supported OpenSSH configuration with explicit compatibility reports. Real proxy, authentication, keepalive and two-gateway transport cases pass on owned Linux fixtures.

**Primary code:** `src/model.rs`, `src/main.rs`, `src/connection.rs`, `src/ui.rs`, `src/ui/pages.rs`.

## P2 — Remote desktops and X11

External launch adapters exist. Integration and options remain incomplete. [Remote desktop, gateway, and X11 feature reference](https://mobaxterm.mobatek.net/features.html)

- [ ] **D01 · Partial:** Embed RDP sessions in the workspace rather than opening an external client. Reviewed Linux implementation checkpoint: owned headless IronRDP helper, real TLS/NLA/frame/input/certificate-change/resize/close tests pass. Native/original interoperability remains pending; see `docs/parity-evidence/D01/`.
- [ ] **D02 · Partial:** Reviewed options, clipboard/vault/fullscreen/admin/drive negotiation, native printer/smartcard source, Linux RXCHAR/BREAK/ERR and per-open driver statistics, Windows per-open statistics/bounded serial owners and opened-handle mutation/allocation are integrated. Current merged helper passes82unit+10worker tests; controlled owner27/pure22 and MSVC source checks pass. POSIX status/properties/modem/identity-safe mutation limitations and native physical/original RDP qualification remain explicit. See `docs/parity-evidence/D02/`.
- [ ] **D03 · Partial (reviewed implementation checkpoint):** Embedded VNC with verified VeNCrypt TLS, Raw/CopyRect/Hextile, Rich Cursor, directional Unicode/Latin-1 clipboard, resize and shared canvas/fullscreen. Real TCP and actual workspace paint/input tests pass; native/third-party/original evidence remains pending. See `docs/parity-evidence/parallel-vnc/`.
- [ ] **D04 · Partial (reviewed implementation checkpoint):** Managed SSH gateway streams for RDP, VNC and embedded Telnet preserve separate gateway trust and target identity, support RDP certificate retries and cancel/clean up owned channels. Real transport/workspace/CLI failure tests pass; native/deployed/original checks remain pending. See `docs/parity-evidence/parallel-gateway/`.
- [ ] **X01 · Partial:** Linux pinned owned Xwayland is integrated; finish patched native Windows/macOS engine builds, bundled dependency closure and native lifecycle qualification.
- [ ] **X02 · Partial:** Coordinate X-server startup, DISPLAY selection, authorization, and SSH forwarding automatically.
- [ ] **X03 · Partial:** Linux OpenGL, Composite and RandR behavior is demonstrated. The private Windows candidate passed authenticated Composite pixel readback and live GLX/RANDR version requests. Windows OpenGL context, RANDR changes, GPU/monitor and original comparison remain open.
- [ ] **X04 · Partial:** Reviewed Linux Direct/Indirect/discovery, fresh hostname resolution, nondefault ports, display/monitor selection, authority/cancellation, bidirectional text clipboard and single installed XKB layouts are integrated. Genuine Linux traced/untraced normal-app qualification passed after the reviewed reset fix; native Windows/macOS engines and original comparison remain open. See `docs/parity-evidence/X04/`.
- [ ] **X05 · Partial:** Add configured remote desktop environment launch through SSH.

**Current implementation:** Linux owns and manages its bundled X server, authorization, display and per-session SSH forwarding. Native Windows/macOS engines and their runtime validation remain open. See [progress](PARITY_PROGRESS.md).

Windows x64 has a locally built private VcXsrv candidate with adjacent DLLs, fonts and XKB data. A controlled native probe verified occupied-display refusal, loopback socket ownership, cookie authentication and listener removal. An independent SSH fixture mapped a real forwarded X window. Native Direct and Indirect XDMCP completed through MANAGE, including selection in the native chooser and authenticated X setup with a controlled peer. The app-owned Windows XDMCP source reached the artifact gate. A second loopback address, Git for Windows SSH, a German XKB mapping and first-monitor fullscreen geometry passed on the development candidate. Private Direct XDMCP passed Unicode clipboard in both directions on a distinct loopback address after manager-cookie authentication. Distribution approval, routed-network and broader graphics workflows remain open. [Windows evidence](docs/parity-evidence/execution/2026-11-25-windows-native.md).

## P2 — Protocols, tools, and packaging

- [ ] **P01 · Partial:** Supply and integrate required protocol clients instead of depending on users to install every client separately. Validate a clean-machine setup.
- [ ] **P02 · Partial — reviewed Linux checkpoint:** Embedded verified FTP/FTPS, active/passive and ASCII, graphical browser/editor and file/folder transfers with conflict/cancel/recovery. MLSD required; native/deployed/original coverage remains pending.
- [ ] **P03 · Partial (reviewed implementation checkpoint):** Embedded Telnet/Rlogin/Rsh propagate endpoint/user, implement Telnet IAC/NAWS/NVT/BINARY/echo, Rlogin urgent/window/flush and real reserved-port handshakes with cancellation. Deferred paste/Enter is ordered. Native and independent daemon/original evidence remains pending. See `docs/parity-evidence/parallel-legacy/`.
- [ ] **P04 · Partial (reviewed implementation checkpoint):** Configurable data bits/parity/stop bits/flow control, compatible defaults, cancellable reader/writer and unplug reporting pass Linux PTY/UI regressions. Windows exclusive-open/source check passes; native drivers and physical hardware remain pending.
- [ ] **P05 · Partial:** Portable Unix payload inventory, package verification and explicit execution are implemented. Reviewed legacy plain-ZIP/LZMA package inspection and inert quarantine import are integrated; original payload execution/resolution compatibility and clean native distribution remain open.
- [ ] **P06 · Partial:** Native TermCraft plugin discovery/install/load permissions are implemented. Legacy package inspection/quarantine import is available; it does not register or execute original plugins and does not establish universal compatibility.
- [ ] **P07 · Partial — reviewed Linux checkpoint:** File/folder comparison, image preview, explicit-target port scan and bounded native packet capture are integrated and tested. Native Windows/macOS capture and original comparisons remain pending.
- [ ] **P08 · Partial — qualified Linux scope:** Current source32e512 full Linux optional release has 3372 verified payload entries and 37 genuine startup/helper/GUI checks within its declared Arch-compatible ABI. Native Debian12 core/revision3 has 1074 payload hashes and 43 runtime commands across actual install/upgrade/replacement/remove/reinstall. Native Windows/macOS payloads, installers, clean runtime/upgrade and original comparisons remain open; signing identities are needed if signed releases are chosen. See [current checkpoint](PARITY_PROGRESS.md#current-checkpoint) and `docs/parity-evidence/P08/`.

**Primary code:** `src/connection.rs`, `src/serial.rs`, `src/ui/pages.rs`, `Cargo.toml`. Protocol and Unix-tool scope comes from the [official feature overview](https://mobaxterm.mobatek.net/features.html); detailed options are in the [official documentation](https://mobaxterm.mobatek.net/documentation.html). Packaging is a delivery gap observed in this project.

## P3 — Servers and platform integrations

- [ ] **B01 · Partial:** Add managed FTP, TFTP, and Telnet servers.
- [ ] **B02 · Partial (reviewed implementation checkpoint):** HTTP sharing has optional private bearer authentication, opt-in atomic new-file uploads, HEAD/ranges/ETag and bounded confined I/O. Backend and real UI/controller cases pass. Hard-link publication is required; FAT/exFAT uploads and native/original verification remain open. See `docs/parity-evidence/parallel-http/`.
- [ ] **B03 · Partial — reviewed implementation checkpoint:** Managed embedded SSH/SFTP and pinned Iperf2/3 service integration, independent-client checks, listener ownership and cancellation are implemented. Native/deployed/original coverage remains open. See `docs/parity-evidence/B03/`.
- [ ] **B04 · Partial:** Add Windows Explorer launch integration, protocol handlers, drive/path mapping, and registry filesystem capabilities where applicable.
- [ ] **B05 · Partial:** Add equivalent Linux/macOS desktop launch integration for portable workflows; document Windows-only concepts rather than presenting them as native on every OS.
- [ ] **B06 · Reviewed implementation checkpoint:** Default-off session-only idle screen, all thirteen game entries,51 stock puzzle presets, advanced controls and Save/Load are integrated. Current release overwrite qualification and measured original/native behavior remain open. See [progress](PARITY_PROGRESS.md) and `docs/parity-evidence/B06/`.

**Evidence:** embedded services and desktop integration are described in [MobaXterm documentation](https://mobaxterm.mobatek.net/documentation.html). Registry mapping, games, and additional service requirements also appear in the existing [parity matrix](FEATURE_PARITY.md) and investigation scope. Exact 26.6 behavior still needs runtime comparison.

## Requirements needing stronger original evidence

These remain open requirements from the earlier project scope. This review did not establish their exact Personal 26.6 surface from the two public reference pages. Confirm them before designing equivalent subsystems; absence from a reference page does not prove the original lacks them.

- [ ] **E01 · Qualified Linux implementation; native/original work remaining:** Embedded Chromium provides in-tab navigation, address entry, forms, proxy and explicit TLS errors. System browser launch remains a separate choice. Actual packaged Linux Chromium navigation/input/resize/PTY/sandbox/cleanup is qualified on source 22bb; its unchanged browser source/runtime payload carries that bounded evidence through the current VNC-client-only fix, without claiming a current browser replay. Windows installed and privately pinned Chromium passed independent session tests. A redistributable Windows payload, native macOS browser qualification and original comparisons remain open. Evidence: [current checkpoint](PARITY_PROGRESS.md#current-checkpoint), [implementation](docs/parity-evidence/E01/implementation.md), [original scope](docs/parity-evidence/original-scope/README.md).
- [ ] **E02 · Partial — reviewed implementation checkpoint:** Timed graphical/terminal recording and replay, waits/calls, cancellation, startup attachment and named invocation are implemented and tested. Native/original timing/focus and recurring scheduler scope remain pending. Evidence: [macro review](docs/parity-evidence/E02/review.md).
- [ ] **E03 · Partial — qualified Linux scope:** Reviewed Cron/NFS/VNC core, X11 clipboard, optional Wayland portal/PipeWire helper and offline noVNC viewer are integrated. Actual Wayland capture/control/idle/held-release/Stop/revocation/view-only/denial/cancel GUI qualification has 125 checks, explicitly retained in the current release through unchanged 13 server-source pins and exact helper identity; 125 checks were not replayed. Current genuine GUI TigerVNC-client proof includes clipboard exchange in both directions. Native Windows/macOS service/capture/control completion, physical desktop/GPU/monitor/consent fixtures, original service measurements and unresolved Personal FTPS exposure remain open. See [current checkpoint](PARITY_PROGRESS.md#current-checkpoint) and `docs/parity-evidence/E03/`.
- [ ] **E04 · Verify:** Confirm version-specific session/tunnel limits and protocol defaults rather than treating estimated values as recovered facts.
- [ ] **E05 · Verify:** Keep Professional-only customization outside the Personal baseline unless the target scope is explicitly expanded. The Professional Customizer is not established as a missing Personal feature.

## Validation gates — separate from missing features

Existing adapters must not be classified as absent solely because endpoints were unavailable. These tasks establish whether present functionality works.

- [ ] **V01 · Verify:** Exercise SSH/SFTP against real endpoints: authentication variants, host-key changes, jump hosts, transfer failures, and reconnects.
- [ ] **V02 · Verify:** Exercise local/remote/dynamic tunnels with IPv4 and IPv6; test unavailable ports and cleanup after closing sessions.
- [ ] **V03 · Reviewed implementation checkpoint:** Reproducible runner and30 actual controlled Linux endpoint cases cover FTP/FTPS, Telnet, Rlogin/Rsh, RDP, VNC and XDMCP. Native/original, independent deployed daemons and RDP end-to-end clipboard/devices remain open. See `docs/parity-evidence/V03/`.
- [ ] **V04 · Verify:** Exercise Mosh through network changes and serial sessions with actual hardware; inspect adapter shutdown and error reporting.
- [ ] **V05 · Partial:** The Windows x64 GUI and independently locked helper built natively. Packaged startup, ConPTY and controlled protocol tests passed. The remaining Windows interactive/provider matrix and macOS qualification remain open.
- [ ] **V06 · Reviewed Linux implementation checkpoint:** Native X11/Wayland scale1x/1.5x/2x, real peer clipboard, idle System/explicit theme images and detached input coordinates pass bounded headless/software matrix. Current-parent1.5x replay retains game guards. Hardware/GPU, active-game and named shortcut/native-original gaps remain. See `docs/parity-evidence/V06/FIX1/`.
- [ ] **V07 · Verify:** Capture the original Personal 26.6 workflows and compare them with equivalent TermCraft sessions; reconcile the mechanics evidence with measured behavior.
- [ ] **V08 · Reviewed implementation checkpoint:** Current Debian12 core clean install, actual version upgrade, replacement, removal/reinstall and complete workspace/test-vault/corrupt-original preservation pass independent review. Full optional Arch artifact closure, other native platforms and original evidence remain open. See [qualification review](docs/parity-evidence/V08/current-core-qualification/review.md).

## Completion rule and execution guidance

The [Windows x64 qualification checkpoint](docs/parity-evidence/execution/2026-11-25-windows-native.md) records partial D02, P08 and V05 evidence, including clean hosted GUI and helper startup, a local installer replay with two independently versioned development packages, real URL activation and native ConPTY with the optional BusyBox-w32 runtime. Platform and original-product acceptance remain open.

The full plan is approved. Use native workflows, meaningful targeted regressions and the single [durable queue](PARITY_EXECUTION_QUEUE.json). Work by actual dependencies and continue after each passing increment. Historical Superpowers strict-order/stop-after-increment instructions are superseded by the user’s 2026-11-24 execution amendment. Full/host gates are batched at subsystem boundaries.

Mark an item complete only when:

1. Its user-visible outcome exists and is reachable in the app.
2. Relevant tests pass, including failure handling and real endpoint/hardware tests where applicable.
3. Supported platform coverage and any remaining differences are recorded.
4. The evidence is linked here and the corresponding feature-matrix row is updated.

The existing penguin branding, emerald palettes, System/Light/Dark selection, saved-session tree, common PTY terminal operations, basic macros, and JSON persistence are present; they are not missing-feature tasks.
