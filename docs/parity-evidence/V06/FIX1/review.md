# V06 FIX1 independent review

**Verdict: prior V06 review findings ADDRESSED.** The append-only corrections establish the historical adapter baseline and complete drift scope. The new packaged .708 focused replay supports the integrated current parent's default-inactive normal desktop paths at 1.5× on X11 and native Wayland. No new blocking finding was identified within that scope. This does not qualify active games or repeat the historical six-session matrix/engine lifecycle.

## Prior findings

- **ADDRESSED — historical patch baseline.** Reversing `historical706-instrumentation-only.patch` against the retained .706 source succeeded; every reconstructed historical production source hash matched `historical-base-source-sha256.json` (274 base source files; the added probe makes 275 frozen files). The original `ui.rs` base is `710ea401…`, and the original detached source is `d67917e5…`. The earlier mislabeled diff is retained as historical evidence and explicitly superseded, while later integration has its own named drift patch. All 248 original evidence inventory entries still match.
- **ADDRESSED — incomplete production drift.** Independently compared the historical and current-parent source manifests, including additions: the exact changed-path set equals all 14 entries in `complete-production-source-drift.json`. The four modified UI/model files and ten additions are accounted for. Current-parent manifest entries still match MAIN's source at review time.
- **ADDRESSED — verification versus execution.** FIX1 explicitly separates the original three retained-evidence validators from .706 native observations and separates the .708 native executions from subsequent artifact auditing. Counts remain historical **6 sessions / 84 observations / 1 engine lifecycle**, plus focused current **2 sessions / 28 normal observations / 2 guard-state observations / 0 ignored**. Guard callbacks are supporting records, not hundreds of new independent native cases.
- **ADDRESSED — current-parent compatibility replay.** The .708 patch reverses cleanly against its executed source and reproduces every one of the 284 current-parent source manifest entries. Both root `games_frame(root)` calls and the detached `games_frame(ui)` call remain. Current game models and screensaver integration remain. Fresh native X11 and no-DISPLAY Wayland at 1.5× exercise the requested normal paths with those hooks present.

## Observer integrity and native results

The guard observation is inside the existing production `if !play.is_open()` branch, immediately before its unchanged `return false`. It adds an assertion over existing state and a file record; it does not assign launcher/session/suspended/close-owner state, replace the `is_open` condition, clear input, inject an early return, or alter game/idle defaults. The literal `return:false` in the record describes the adjacent unchanged return. The root observer independently asserts game/idle inactivity and records it; it does not force inactivity. The adapter's ordinary action commands operate existing terminal, clipboard, appearance and detach paths and never open or close a game. These are actual inactive callbacks rather than a fabricated stand-in guard.

Independently checked both retained raw runs:

| Native replay | Observations | Guard records and viewports | System pixel difference | Override maximum | Cleanup |
|---|---:|---|---:|---:|---|
| X11, 1.5× | 14 normal + 1 guard | 924; root and both detached viewports | 640.271 | 0.109 | 0 survivors |
| Wayland, 1.5× | 14 normal + 1 guard | 1,016; root and both detached viewports | 640.783 | 0 | 0 survivors |

All guard records contained the actual default inactive fields and false-return observation; 28 X11 callbacks and 43 Wayland callbacks also contained input events. Exact Unicode/multiline payload bytes were present in each raw PTY and matched each native peer TextEdit output. Independently expected root/detached SGR sequences were present in raw PTY bytes, and both VNC pointer observations per run matched raw RFB input records within the stated one-pixel tolerance. Wayland records have no DISPLAY, `xwayland disable` in compositor configuration, and actual `xdg_shell` native window identities; X11 has no WAYLAND_DISPLAY. The X11 native RequestPaste workaround remains disclosed and does not establish the peer keyboard shortcut.

All **349** entries in the appended hash inventory matched before this review file was added; all **287** executed .708 source/qualification entries matched their frozen source. Published and executed controller/audit/pins/VNC/probe scripts matched. The packaged executable matched `77289a5766bf8fa6a3e81cf7e8d1c0088e8acf6bb683cfc5a1dac33c86cb298a`; its package archive matched `64840f72a1d2548917051efdbbbbe5d7ed3d1826625b7cf1f4f482746660648d`. A fresh read-only process scan found no live V06/.708 native fixture processes. No build, native repeat, production edit or background job was started by this review.

## Remaining boundaries

**NOT ADDRESSED BY DESIGN:** active-game native play, suspension/close-owner handling, and game/screensaver lifecycle remain B06 pending. Physical GPU/display/input hardware, mixed-monitor DPI, native Windows/macOS and comparison with the original application remain outside this evidence; strict original/native-all-platform completion stays **0**. The focused replay establishes the named source parent and default-state paths, not equivalence of a later release executable merely because it shares those sources. Root owns release qualification, checkpoint integration and aggregate count decisions.
