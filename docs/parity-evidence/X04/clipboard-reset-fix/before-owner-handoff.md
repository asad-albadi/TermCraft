# Local XDMCP chooser reset RED: owner handoff

Status: concrete application failure captured twice under syscall tracing; four untraced controls passed. Stop additional cases. No product changes were made. Review/fix the local lifecycle defect before claiming only other-machine qualification remains. This does not retrospectively identify the older run-2 timeout.

## Reproduction inputs and cases

Use the exact f02457ae354d6948e08bb4362d3547688a5e2362b726beaae7276b2dc20c5525 full archive extraction. Source d6e6c3aca99a068aedc0479cb376a7d0927a4b2847ef3b0aed8e7d6d88a2e7d9; actual ordinary release app SHA 9aad4916cc3c2b5cf85d0bd06a05328ca3bff26a26db93ca640edee88a710fea; feature 8cf61e9f10f5c797c249eae12a36f8cc0f43ca51ceda89c62bc663593f30f460. Each start verified the actual app executable, live sealed engine bytes/seals15 and owned TCP listener. The Direct companion uses stock d7f1f49ee428fd06b26707a79ba6d65b4e9ada4e6b53aca4146baaa92c79e593. Do not relabel this as a 31aa GUI run.

Driver: run-cases.py; immutable-artifact verifier: verify.py; fixture modification: fixture-change.json. Reproduce a single existing RED with CASE_ID=traced-00 PENDING_HOLD=0 SELECTED_HOLD=0 TRACE_SELECTED=1 TERMCRAFT_VALIDATION_OBSERVED_COORDINATES=420,281 ./run.sh EXACT_EXTRACTION. The0/11 RED uses SELECTED_HOLD=11. strace is privately extracted from package SHA b3c67815289238f99ab6422d0cd562dbbb5becca8b49179fffad72be9f23e1a0, pinned by the existing local Arch database; no host installation. It runs as parent of the actual selected-session app with -ff -ttt -s128 -xx -e trace=%network,%process,%signal. All tracing and network/input activity are confined to owned private namespaces. Raw traces include private X11 authorization bytes; inspect locally and never print their payloads into public reports.

## Exact RED evidence

All paths below are relative to this reliability-boundary directory.

| Case | Evidence | GUI failure | Observation |
|---|---|---|---|
| traced0/0 | evidence.BWIRaaYM | XDMCP text clipboard stopped: The X11 server closed the connection |75.028s after real selection |
| traced0/11 | evidence.HeBm4oYc | XDMCP text clipboard stopped: Nested clipboard reset limit reached: The X11 server closed the connection |75.023s after real selection |

Both: actual app PID115, tracer PID113 (stored TracerPid), native engine PID156, app worker TID154, clipboard bridge TID160. Each directory has selected.trace.156 (native engine), selected.trace.160 (bridge), selected.trace.154 (worker), live-state.jsonl, observations.log, case-result.json and inspected await-selected-10.png/42.png/74.png. Main GUI remained alive and displayed disconnected/error; engine was absent through the extended observation. The driver did not tear the case down at ten seconds. It observed75s, exceeding ten seconds plus two32s maximum native retry periods. Remaining application and private parent namespace stayed present until owned cleanup.

### traced0/0 wall-clock sequence

-1795601075.107109: bridge nested fd15 recvmsg returned0; recovery began.
-1795601075.107538: new fd16 connected to /tmp/.X11-unix/X177; fresh SO_PEERCRED at.107596 confirmed owned PID156/UID0/GID0 before private cookie transmission.
-1795601075.159907: native engine received selected WILLING on its UDP fd13; .159991 sent REQUEST, .160276 received ACCEPT and .160355 sent MANAGE. WILLING→REQUEST84 microseconds. Thus no missing UDP response or ten-second marker cutoff in this RED.
-1795601075.174827: bridge fd16 EOF, then recovery fd15 connected and freshly verified PID156; another complete native REQUEST/ACCEPT/MANAGE cycle followed.
-1795601075.232415: fd15 EOF; third recovery fd16 connected at.232704 and freshly verified PID156 at.232747.
-1795601075.288998: bridge fd16 sent X11 opcode23 GetSelectionOwner after extension/atom/window/XFixes initialization. At.289412 its recvmsg returned0. This pins the propagated error to Endpoint::new's final get_selection_owner(...).reply() operation.
-1795601075.302671: app worker TID154 called kill(156,SIGTERM)=0. At.302723 native PID156 received SIGTERM with si_pid115, then exit_group(0) and exited0 at.306657. The engine did not spontaneously crash. The app terminated its owned engine after its clipboard worker failed.

The second RED has the same repeated-reset shape: bridge EOFs at1795601181.150814/.214568/.273050/.329694; worker154 kill(156,SIGTERM) at.340878. Its inspected GUI specifically reports the3-reset limit. Native selected WILLING→REQUEST47 microseconds and repeated REQUEST/MANAGE again establish progress through the network handshake before local teardown.

## Root call chain and review target

src/x11/xdmcp_clipboard.rs:run (around784–875) catches nested tick EOF, retains a three-reset/30s budget, checks retained pidfd, bounds recovery to2s, clears parent state, connects only to the owned socket, verifies SO_PEERCRED, reconnects with the private cookie, then calls Endpoint::new(...)? at851. The initializer's GetSelectionOwner reply at255–259 can receive another reset EOF; that error currently escapes recovery immediately. The other captured failure reaches the explicit reset-limit return at815–816. Bridge::check at133–140 turns the finished worker error into the GUI text. XdmcpSession::is_running at src/x11/xdmcp_engine.rs40–43 propagates bridge.check failure; the managed worker's session is dropped and its owned process is stopped, matching the traced SIGTERM.

There is a likely feedback race requiring native-source review, not an approved fix: bridge reconnection accepts occur during the native reset/requery before the selected manager finishes its session open. Native os/connection.c calls XdmcpOpenDisplay after authorization, and os/xdmcp.c records the first accepted authorized client while awaiting MANAGE as sessionSocket; closing it triggers DE_RESET. The trace shows AF_UNIX recovery accepts before selected WILLING/ACCEPT, with repeated requery cycles. Whether an early bridge authorization becomes the native lifecycle client, or another xdm reset/client ordering is responsible, still requires owner attribution. The current trace selection does not record every native read/write needed to prove the exact authorization instruction timing. Do not present this inference as established fact.

Fix review must decide whether a bounded full-endpoint retry is sufficient or whether bridge recovery needs a qualified session-generation/readiness boundary to prevent a self-reset loop. Do not merely raise the reset limit, add arbitrary sleep, swallow every error or weaken peer/cookie/pidfd/deadline/cancellation gates. Preserve zeroization, parent clipboard clearing and per-generation selection sampling. A private candidate should retain a real traced reset RED, add targeted regression coverage for EOF during initialization and repeated ordinary chooser resets, and be reviewed before MAIN edits/new actual package qualification. No fix implementation is included in this packet.

## Controls and excluded attempts

Four complete untraced controls (pending/selected hold seconds):0/0 evidence.clzK4u7r;0/11 evidence.CXJhbIix;11/0 evidence.c79fH4im;11/11 evidence.G7b1YFPA. All passed genuine real chooser1177→177, same sealed feature PID after reset, German29/52=122/121, exact39/47-byte bidirectional Unicode, wrong-cookie rejection, complete parent XKB equality, visible own-resource cancellation preserving Direct companion, and three app exits0.

Two earlier traced attempts,11/0 evidence.OTJz9PYJ and11/11 evidence.8bFVMQM5, aborted in the diagnostic collector while an fd vanished between /proc directory enumeration and readlink. They are excluded from product results. probe-v1-fd-race.py preserves that collector; current probe tolerates vanished fds. No replacement cases were run after the two genuine REDs established the failure.

Eight attempts total; six complete functional cases: four controls pass and two traced cases fail. Tracing changes scheduling substantially, so observed rates are not normal-user failure rates. It exposes a real EOF/reset error path in the unchanged product; untraced passes do not invalidate that captured failure. protocol-analysis.json contains only protocol headers/state summaries, including actual engine receive/send boundaries. No choice expired. All selected-manager logs contain zero PAM RedrawFail messages under the persistent fixture.

The only fixture behavior change was the authenticated green setup client's sleep 20→120s, preserving its exact window/authentication/map observations while keeping it alive beyond holds plus75s observation. Previous20s behavior and PAM-denial flood are documented in ../diagnosis-delay/report.md. No xdm security or choice-timeout setting changed.

Per-case exit codes in case-results.json are durable. The outer tool session returned143 after every one of the eight END records had been persisted; do not claim outer-driver exit0. This does not alter the independently recorded per-case statuses. All previous202+62 sealed file hashes were checked unchanged before sealing this packet. No MAIN/product source edits occurred.
