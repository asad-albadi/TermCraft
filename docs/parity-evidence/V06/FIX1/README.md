# V06 FIX1 — corrected provenance and focused current-parent replay

This append-only correction supersedes the baseline label of the original `fixture-source/instrumentation-only.patch` and expands the original drift list. **All original 248 evidence files, the original `.706` source, and the original `.706` package bytes remain unchanged.** [Frozen inventory audit](original-248-frozen.json), [final preservation/process audit](current708-final-reap-audit.json).

## Historical patch correction

The `.706` application snapshot/native matrix was captured before the current games/UI integration. The original patch was generated later using a direct comparison of then-current MAIN files against the frozen private instrumented files. MAIN's new games integration had landed by patch-generation time. Consequently, that diff mixes the adapter with changes that reverse later games integration. Its old “instrumentation-only” label and declared baseline were wrong; reversing it yields newer MAIN `ui.rs` hash `698e55…`, rather than the tested historical `710ea…`. Generating that diff did not modify the frozen `.706` application, package or native reports.

The corrected [historical instrumentation-only patch](historical706-instrumentation-only.patch) uses exact historical bases from `docs/parity-evidence/B06/ui-integration-base/src/{ui.rs,ui/windows.rs,ui/games.rs,games/mod.rs}`. These files are copied under [declared-historical-base](declared-historical-base/src/ui.rs). Historical `ui.rs` SHA-256 is `710ea401d58b44183b90811095af4958627063fd6e5bd8d1e2a0ce21b951b22c`; historical detached-window source SHA-256 is `d67917e5d92dc10077116b561b25ef9af1a45554f773025886a15de85c73d701`.

For unchanged leaf files, only the recorded probe/start-stop hooks were removed from a private copy of frozen `.706` source. These reconstructed leaf bases also match current MAIN's unchanged production files. Applying the corrected adapter to this reconstructed historical base reproduces **all 275 frozen `.706` source files byte-for-byte**. [Patch validation](historical-patch-validation.json), [complete historical source hashes](historical-base-source-sha256.json). The original mislabeled patch is retained solely as frozen history; use this corrected patch when reconstructing the tested historical source.

## Complete production drift

The [complete drift inventory](complete-production-source-drift.json) compares the reconstructed historical production source against the frozen current parent, including additions. It contains **14 entries**: four modified files (`src/ui.rs`, `src/ui/windows.rs`, `src/ui/games.rs`, `src/games/mod.rs`) and ten added library/terminal/play source or documentation files. The original two-file list was incomplete. A separately labeled [historical-to-current production drift patch](historical-base-to-late-current-production-drift.patch) records these integration changes; it is not an instrumentation delta.

[Current-parent source hashes](current-parent-source-sha256.json) identify the replay base. Current parent `ui.rs` hash is `698e55be9c8fd8c101660ed9e887cb13230134c64fb93af718bdf18ff966279a`; current detached-window source hash is `b312974d14594bfcaae5572cc383e8d986babd2cc87db00232630f1fa8f835d2`. The historical `.706` native matrix remains qualification of its exact earlier snapshot. It is not renamed as current-game qualification.

## Fresh focused current-parent execution

A new private package **0.1.708**, built with unique bin **`termcraft-v06-current708`**, retains current production models, both root `games_frame(root)` guards, the detached `games_frame(ui)` guard, and the games/screensaver integration. [Boundary checks](current708-integration-boundary.json), [current adapter patch](current708-instrumentation-only.patch), [executed source hashes](current708-instrumented-source-sha256.json). The adapter adds read-only state assertions and guard callback records; it does not replace these guards or change their return conditions.

Fresh native packaged runs under private X11 and private Wayland, both at **1.5×**, passed **28 normal native observations plus 2 additional default-inactive guard observations**, with **0 ignored**. [Summary](current708-summary.json), [provenance](current708-provenance.json), [X11 execution record](current708-native/x11-1.5/observations.json), [Wayland execution record](current708-native/wayland-1.5/observations.json).

- Production portal System mode changed visibly while no command/input was issued for the portal change; explicit Light/Dark remained stable. Independently recomputed System screenshot differences were approximately **640.27 X11 / 640.78 Wayland**; explicit override differences were **0–0.109**, below the threshold of 2.
- Real native peer-window Unicode/multiline clipboard transport passed both directions, including native pointer approval of the existing confirmation and exact owned-PTY payload bytes. The same disclosed X11 `RequestPaste` workaround applies; this is not an X11 keyboard-shortcut qualification.
- Native input produced the independently expected root/detached terminal SGR bytes and detached VNC quarter/three-quarter coordinates observed at the real RFB peer.
- Wayland had **no DISPLAY**, compositor XWayland disabled, and native `xdg_shell` window records. X11 had no Wayland display and used private Xvfb/XTest. Environment and host/private namespace/socket identities are recorded.
- Actual current root and detached guards ran with default inactive state. The logs contain **924 X11 / 1,016 Wayland** callbacks across root and both detached viewports, all with `launcher=false`, `suspended=false`, no session, no close owner and return `false`. The adapter also asserts no active idle screen. No game was opened.
- Cleanup adopted/reaped owned descendants; both sessions report **zero survivors**. No V06 owned native processes remained in the final process audit.

Application SHA-256: `77289a5766bf8fa6a3e81cf7e8d1c0088e8acf6bb683cfc5a1dac33c86cb298a`.

Package SHA-256: `64840f72a1d2548917051efdbbbbe5d7ed3d1826625b7cf1f4f482746660648d`.

The separate package is at `target/agent-work/v06-current708/package-output/TermCraft-0.1.708-linux-x86_64-arch-xwayland.tar.gz`. [Build log](build.log), [package build log](package-build.log), [package manifest](current708-PACKAGE-MANIFEST.json). The pinned `.706` peer/portal/tool executables were reused without rebuilding or overwriting them; [pins](current708-fixture-source/pins.json) identify their exact bytes.

## Verification versus execution

The original `verify.py` / `--verify` PASS audits retained declared metadata, counts, selected byte/scale assertions and artifact presence. It is not fresh native execution, does not independently recompute every original assertion, and does not qualify later source changes. The original three Rust tests are evidence validators with **zero** contribution to the 84 historical native observations.

The fresh `.708` native executions are the two `matrix.py` launches against the new package, recorded in [X11 run log](x11-1.5-run.log) and [Wayland run log](wayland-1.5-run.log). A subsequent independent [read-only audit](current708-audit.log) recomputes retained screenshot differences, payload equality, raw SGR/VNC observations and default-state guard records, and checks source/executable bytes. That audit is verification of the fresh runs, not another native execution. [Audit source](current708-fixture-source/audit708.py), [execution controller](current708-fixture-source/matrix.py).

The focused `.708` replay supports the **current integrated default-inactive normal terminal/clipboard/desktop paths at 1.5×**. It does not repeat the historical six-scale matrix or the historical packaged-engine lifecycle, and does not qualify current active games, game suspension, game close-owner or game/screensaver lifecycle behavior; those remain **B06 pending**. Hardware/mixed-monitor/native Windows/macOS/original-runtime exclusions continue to apply. **Strict original/native-all-platform completion remains 0.** Root owns fresh review and checkpoint decisions.
