# Independent workspace review — 2026-11-24
Reviewer /root/workspace_review, fresh-context gpt-6.2-astra/high; read-only; verdict fixes required. Supplied suite 276 passed/6 ignored; not independently re-run. One fix pass, no re-review.

Strengths: stable-ID transport ownership across layout/window registries; embedded auth responses stay with owning handle; pending paste keeps original IDs; native/original evidence gaps remain explicit.

## Critical
C1 Compatibility/OpenSSH and SFTP PTY clients are considered ready before interactive authentication. A password can follow keyboard/paste MultiExec into unrelated terminals. Exclude opaque external network clients from MultiExec unless readiness/private sign-in can be established. Controlled prompt plus receiving PTY regression required. connection.rs:76, ui.rs:767, session_confirmations.rs:31, ui/broadcast.rs:116.

## Important
I1 Enabling/re-enabling MultiExec silently selects every ready tab, including excluded/detached targets. Preserve membership across toggles; explicit Select connected enrolls all. ui.rs:1034, ui/broadcast.rs:73/161.
I2 Embedded fallback children dispatch shortcuts on shared root context before root; root-focused A can close child B. Route event once using focused terminal's owning window. ui/windows.rs:181, ui/shortcuts.rs:264/370, ui.rs:1647.
I3 Native child dispatcher removes key before encoding but leaves paired text; custom Alt chord text reaches/broadcasts into child shell. Consume chord plus paired text atomically. ui/windows.rs:181/205, ui/shortcuts.rs:293, pages.rs:1376.
I4 Detaching active B leaves root toolbar targeting B while main layout paints attached A. Reconcile root active selection on placement changes and bind toolbar to displayed main selection. ui/windows.rs:98, ui/layout.rs:80, pages.rs:121/169.
I5 Detached Find lacks search field/focus; subsequent query can go to shell. Render search editor in owning viewport and transfer focus. ui/shortcuts.rs:506, pages.rs:200, ui/windows.rs:202.
I6 Every successful dispatch replaces failures/removal notices; partially delivered commands can diverge without persistent warning. Retain failures until acknowledged and identify terminal. ui/broadcast.rs:102/129/231.
I7 Output can be marked seen from previous paint on frame switching A to B without new A revision being painted. Track output since paint; clear only when actual changed revision paints visibly. ui.rs:1559/1627, terminal_output.rs:49, ui/activity.rs:90.
I8 Per-byte whole-grid double hashes stall GUI. Reviewer probe: four 1-byte feeds at 500x200 took273ms, vs14ms at80x24 (local debug). Track parser visual mutations/dirty rows instead; add small-output/large-grid regression measurement. ui/activity.rs:68, terminal.rs:413, terminal_output.rs:39, ui.rs:1684.
I9 Reconnect keeps detached bool only, replacing original window ID/fullscreen/group placement with newly allocated child. Preserve window identity and replace terminal generation in same placement. ui.rs:832/843, ui/windows.rs:102.
I10 Close/paste confirmations use saved session titles instead of user tab names; identical session duplicates cannot be distinguished. Show override plus stable session/endpoint detail. ui/broadcast.rs:172, session_confirmations.rs:377/397.

## Minor
M1 Stale hardcoded Ctrl help/tooltips after rebind/unbind/macOS. ui.rs:982/1000/1057, pages.rs:1058.
M2 Shortcut resolve rebuilds/validates map per lookup; repeated allocations. workspace/shortcuts.rs.

## Declined to judge
D1 Native fullscreen/compositor/macOS runtime: already pending native evidence.
D2 Exact Personal26.6 behavior: pending original comparison.
D3 Install/upgrade/uninstall/archive/redirect/certificate implementations: later subsystems; only workspace interactions reviewed.
D4 Integrated remote-desktop capture has no production producer for current context hook; existing external adapters do not establish integrated workflow.
D5 Release/native latency thresholds: reviewer probe is current debug library with synthetic grid only.
