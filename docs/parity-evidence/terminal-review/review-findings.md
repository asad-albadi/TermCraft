# Independent terminal review — 2026-11-23

Reviewer /root/terminal_review (fresh-context gpt-6.2-astra/high), read-only. Verdict: With fixes. Supplied suite 226 passed/6 opt-in ignored; reviewer independently reproduced issues through isolated library probes. No re-review is planned.

## Fix pass

- C1 Critical: remote Save-all window approval remains latched after a new dirty revision; final active-window approval skips new edits. remote_editor.rs:310, session_confirmations.rs:124, remote_editor.rs:178.
- I1 Important: Paste followed by Enter in one GUI frame sends Enter first; protocol replies interleave with an open paced bracketed transaction. pages.rs:1346/1450/320, ui.rs:267.
- I2 Important: UI RequestPaste has no initiating terminal/target identity; focus/broadcast changes before platform delivery retarget clipboard text. pages.rs:1317/1350, session_confirmations.rs:135.
- I3 Important: native-normalized Event::Copy (Ctrl+C) cannot produce ETX with an empty selection; pinned egui-winit consumes the Key event. terminal_selection.rs:86, pages.rs:1371.
- I4 Important: generated standalone-SFTP native filenames are re-encoded by terminal codec after quoting (UTF-8 café bytes C3 A9 become E9 in CP1252). pages.rs:215, terminal_settings.rs:35.
- I5 Important: alternate Screen::plain/search include hidden primary history; copy/export can disclose unrelated output. terminal.rs:257, pages.rs:152/1169.
- I6 Important: IRM combining extension reserves columns twice; standalone combining marks overwrite a wide continuation without clearing its head. terminal.rs:622/638/662.
- I7 Important: height shrink drops bottom/current prompt instead of preserving the cursor-relative live region/history. terminal.rs:199.
- I8 Important: synchronous per-tab output drain may parse 8 MiB, twice per GUI update; reviewer measured 1.171 s for 1.064 MB ordinary debug output. ui.rs:228/258/1847.
- I9 Important: mouse encoder clears accepted press before refused release admission; out-of-range legacy release has no representable fallback. terminal_mouse.rs:129, mouse.rs:99, ui.rs:97.
- I10 Important: serial running state ignores observed reader disconnect and writer errors are not polled independently, permitting partial broadcast after stale readiness. ui.rs:241/274, session_confirmations.rs:193, process_input.rs:74.
- I11 Important: Reconnect immediately destroys an active session without active-close confirmation. pages.rs:133, ui.rs:725.
- M2 regraded Important: cancelled recorded paste retains opening framing without cancellation close, so playback can enter an unbalanced paste state. Record accepted logical text without framing. session_confirmations.rs:44/26.
- M4 regraded Important: oversized valid-JSON saved rules are repeatedly cloned before validation, defeating bounded render-memory behavior. pages.rs:280, highlighting.rs:14.
- M5 regraded Important: fixed alternate grids larger than the viewport disable both local scroll axes, leaving controls inaccessible. pages.rs:1180.

## Deferred minors

- M1: historical row pointer selection is clipped to the new live-grid column count after narrowing. pages.rs:1154, terminal_mouse.rs:23.
- M3: competing spans within one grapheme paint the first byte-position span rather than the highest-priority whole-cell span. highlight.rs:76, pages.rs:1536.

## Declined to judge (all reviewer entries retained)

1. Original Personal 26.6 behavior, visual equivalence and undocumented defaults need reference runtime.
2. Native Windows ConPTY/COM cancellation/clipboard/fonts need Windows runtime; source checks prove compilation only.
3. Native macOS PTY/viewport/clipboard/fonts need macOS runtime.
4. Physical serial timing, touch, selection-edge auto-scroll, high-DPI and X11/Wayland differences need devices/desktops.
5. Native GPU frame latency/release throughput remain measurements; reproduced debug parse stall is graded separately.
6. Arbitrary fonts, other/stateful encodings, ANSI256 editing and extended mouse modes retain explicit parity gaps.
7. Full paragraph reflow, physical-row copying, long-grapheme limits and live-selection snapshots retain explicit rulings; they do not excuse height loss/glyph ownership.
8. File staging/archive extraction/redirects and original F01–F06 issues are outside this boundary; the generated SFTP command regression was assessed.
9. Host-key/certificate/provider/recovery-identity subsystems retain their owners; terminal gating/clipboard targeting/close handoff were assessed.
10. Installer/upgrade/removal/client availability/OS settings need packaging/native evidence; added dependencies are declared and lockfile pinned.
11. Layouts/detached tabs/MultiExec selection/shortcuts/general macro expansion belong to later tasks; current clipboard/Ctrl+C/cancelled recording/close issues remain graded.
