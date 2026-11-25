# XDMCP first-client lifecycle diagnosis and private native candidate

## Result and confidence

The pinned Xorg C implementation permits the authenticated internal clipboard client to become the XDMCP session's first client. Its root child window is then eligible for xdm's normal SecureDisplay/pseudoReset cleanup. If it is the recorded sessionSocket, killing it causes a full server DE_RESET even with the manager connected. Immediate bridge recovery can repeat the ordering in the next server generation. This is a concrete source mechanism supported by the existing traced failure, not a conclusion drawn from Rust EOF alone.

**Directly observed:** the bridge's three recovery connections each have X11 resourceIdBase 0x200000 and each create window 0x200000; the selected xdm logs three SecureDisplay/pseudoReset XKillClient 0x200000 operations; the first two are followed immediately by xdm fatal IO, requery, and another REQUEST/ACCEPT/MANAGE. The precise native XdmcpOpenDisplay state/socket assignment is not present in existing syscall traces, so attributing each DE_RESET to that assignment remains a strongly supported inference requiring a narrow diagnostic trace to prove instruction-level causation. At the initial source-analysis checkpoint no new experiments, builds, product edits, or auth changes had been made. The subsequent authorized private native candidate and its validation are recorded below.

## Pinned source identity

All paths below are relative to the app root unless absolute.

* `packaging/x11/xdmcp/manifest.json`: Xorg 21.1.26 source SHA256 c8609f9cdafde14de7ec7cb406dac9507add1d536df8d3c3e3d3d9c1cd864066; xdm 1.1.17 source SHA256 9494aef0911a031c53670725b5c8c9bb9d3f7c5ea7318b1f72ddd9dcbbeceb6a.
* Both hashes were independently checked against `target/agent-work/x04/source-packages/{xorg-server-21.1.26,xdm-1.1.17}.tar.xz`.
* Native source examined: `target/agent-work/x04-keyboard/build4/engine-src` (called X below). `configure.ac:29` states 21.1.26. Its adjacent `Xephyr-session-ready` SHA256 is 8cf61e9f10f5c797c249eae12a36f8cc0f43ca51ceda89c62bc663593f30f460, exactly the manifest/current failure feature binary.
* xdm source examined: `target/agent-work/x04/source-packages/xdm-1.1.17` (called D below).
* Current `packaging/x11/xdmcp/readiness/xephyr.patch` hash verified 2d03a69b2f3a0aee25141890b98b9524082de39af350a6df56e586c15fd9a344. It adds the one-shot session-ready signal inside XdmcpOpenDisplay and the explicit layout behavior; it does not change first-client selection or close semantics.

## Complete causal source chain

1. **Connection and authorization precede lifecycle accounting.** X `dix/dispatch.c:3776–3806` ProcEstablishConnection calls ClientAuthorized after protocol checks, then SendConnSetup. X `os/connection.c:511–582` ClientAuthorized calls CheckAuthorization and returns on failure (unless the existing host authorization policy independently authorizes); only after success does it set `auth_id`, clear connection timeout, and call `XdmcpOpenDisplay(priv->fd)`.
2. **No distinction between private bridge cookie and manager cookie.** X `os/auth.c:110–143` LoadAuthorization loads the private `-auth` file through protocol Add. `CheckAuthorization:165–222` selects the protocol check. `os/mitauth.c:51–85` adds cookies to the MIT cookie list and checks equality against any list entry. `os/xdmcp.c:1180–1222` recv_accept_msg adds the manager's accepted authorization via XdmcpAddAuthorization (`909–920`), whose normal path is AddAuthorization (`os/auth.c:271–284`). Both result in valid auth IDs. ClientAuthorized does not pass cookie source, peer role, or auth ID into XdmcpOpenDisplay. Merely using the private initial cookie therefore does not exempt the bridge from XDMCP lifecycle ownership.
3. **First *authorized* client, not first accepted socket/window.** X `os/xdmcp.c:1244–1269` send_manage_msg sets `state=XDM_AWAIT_MANAGE_RESPONSE` before sending MANAGE. `XdmcpOpenDisplay:639–653` returns in every other state; in this state it selects the caller's fd as sessionSocket, sets RUN_SESSION, arms dormancy, and emits the one-shot ready signal. An accepted socket can precede MANAGE but become sessionSocket if its authentication is processed afterwards. SO_PEERCRED on the app side proves the engine identity; it does not stop the engine selecting that app-side bridge connection.
4. **Ordinary client accounting is separate.** X `dix/dispatch.c:3690–3772` SendConnSetup increments nClients and moves the client to Running, then invokes ClientStateCallback. XdmcpOpenDisplay happens earlier, inside ClientAuthorized. `CloseDownClient:3484–3570` calls CloseDownConnection, decrements nClients for clients that advanced beyond Initial, and requests the ordinary last-client exception when the final Running client closes. The latter is not required for the sessionSocket reset. The phrase `serverResetOnLastClient` is not a symbol in this pinned source; here the relevant implementation is nClients + dispatchExceptionAtReset + SetDispatchExceptionTimer (`dispatch.c:403–436`, default DE_RESET at3480).
5. **Bridge window is a cleanup target.** `src/x11/xdmcp_clipboard.rs:222–259` Endpoint::new initializes XFixes/atoms, creates a window, subscribes, then samples selection owner. `create_window:758–782` creates a 1x1 INPUT_ONLY child of the root with PROPERTY_CHANGE. Unmapped/InputOnly does not exempt a child from XQueryTree enumeration.
6. **Manager cleanup kills the owner connection, not just its window.** D `xdm/dm.c:809–820` SetAuthorization -> WaitForServer -> chooser or ManageSession. `xdm/server.c:269–307` WaitForServer opens the display. Selected manager uses ManageSession (`xdm/session.c:323`); D `greeter/greet.c:273` calls SecureDisplay. D `xdm/session.c:474–503` SecureDisplay grabs server/keyboard and calls pseudoReset. D `xdm/reset.c:58–74` killWindows enumerates all root children and calls XKillClient for each; `pseudoReset:91–113` repeats per screen and XSyncs. Its own source comment explicitly says a client with no windows can remain connected. X `dix/dispatch.c:3341–3367` ProcKillClient resolves the resource owner with dixLookupClient and calls CloseDownClient(owner). This can kill the bridge as soon as its first root child exists, including during Endpoint::new before its final GetSelectionOwner reply.
7. **Recorded client death forces full reset with manager still alive.** X `dix/dispatch.c:3522` CloseDownClient -> X `os/connection.c:777–789` CloseDownConnection -> `CloseDownFileDescriptor:756–768` -> `XdmcpCloseDisplay(connection)`. X `os/xdmcp.c:656–668` checks active session state and exact sessionSocket equality, sets INIT_STATE and DE_RESET (or DE_TERMINATE for -once), and yields. It never checks nClients. Thus the manager's own connection does not protect against this reset. X `dix/main.c:143–163` next generation invokes ResetWellKnownSockets; X `os/connection.c:348–354` resets authorization/hosts and restarts XDMCP. Reloading the private file makes the bridge able to authenticate again; there is no cookie-rotation barrier distinguishing the manager.
8. **Separate Rust consequence.** Current `run:784–875` counts 3 resets/30sec and reconnects during a 2sec recovery deadline, requiring retained child pidfd liveness and freshly matching peer PID before sending private cookie. Endpoint::new at850 uses `?`, so an initialization reset escapes immediately rather than being retried as another generation. Reaching the limit also returns an error. These are separate bounded-recovery handling issues; full initializer retry alone cannot prevent the native first-client feedback loop.

`D/xdm/server.c:312–316` ResetServer deliberately skips pseudoReset for FromXDMCP displays; that does not disable the unconditional SecureDisplay pseudoReset above. A transport-family-wide AF_UNIX exemption is unsupported: a legitimate manager can connect by Unix transport and must remain eligible as lifecycle client.

## Existing actual failure evidence (no secrets)

Packet root: `target/agent-work/current-packaged-xdmcp/reliability-boundary`. Read owner-handoff.md, report.md, safe-failure-timelines.json, protocol-analysis.json. Actual app PID115, engine156, bridge TID160, worker TID154; tracer113. Case BWIRaaYM is the traced0/0 failure, actual sealed feature8cf6. Times below are wall seconds 1795601075 + fractional suffix. Trace payloads were decoded locally solely to extract non-secret X resource IDs; no cookie payload is reproduced.

| Time suffix | Evidence |
|---|---|
| .081485 | trace.160:14 initial chooser bridge nested fd15 setup ridBase0x800000 |
| .087457 | trace.160:72 fd15 CreateWindow0x800000 |
| .107109 | bridge fd15 EOF (expected chooser transition starts recovery) |
| .107538/.107596 | bridge recovery fd16 connect/private Unix X177; fresh SO_PEERCRED engine156 |
| .159484 | engine156 accepts AF_UNIX as server fd1 |
| .159907/.159991 | engine receives selected WILLING / sends REQUEST on UDP fd13 |
| .160276/.160355 | ACCEPT / MANAGE |
| .160891 | trace.160:97 recovery fd16 successful X setup ridBase0x200000 |
| .163392 | trace.160:122 fd16 CreateWindow0x200000 |
| .174827 | fd16 EOF; recovery reconnects fd15 with fresh engine156 peer |
| .222630/.226112 | trace.160:148/173 fd15 setup ridBase0x200000 / CreateWindow0x200000 |
| .232415/.232704/.232747 | fd15 EOF / third fd16 connect / fresh engine156 peer |
| .278520/.286872 | trace.160:197/222 fd16 setup ridBase0x200000 / CreateWindow0x200000 |
| .288998/.289412 | fd16 GetSelectionOwner request / EOF during initializer |
| .302671/.302723 | app worker154 kill(156,SIGTERM)=0 / engine receives sender115 |
| .306657 | native engine exits0 after app-requested stop |

BWIRaaYM `managers/selected/log:199–204`, `281–286`, `363–369` each explicitly shows SecureDisplay -> XGrabServer -> XGrabKeyboard -> pseudoReset -> **XKillClient0x200000**, exactly the bridge's window in the three recovery generations. First two immediately log fatal IO from manager children163/165; third reaches pseudoReset done before manager167 later receives fatal IO. These log lines have no wall timestamps; ordering and resource IDs correlate, but they cannot alone prove sub-instruction timing.

Second traced case HeBm4oYc repeats four EOFs at1795601181.150814/.214568/.273050/.329694; worker154 kills156 at.340878. GUI explicitly reports reset limit. Native WILLING->REQUEST47us and repeated MANAGE exclude missing UDP reply or observation cutoff. Four untraced controls passed; tracing alters scheduling and exposed a real source race, not a normal-user rate estimate.

## Bounded fix choices and qualification still needed

* **Identity-scoped lifecycle exclusion** is the strongest direct prevention: for the explicit private feature mode only, capture/retain the launching app identity, and after existing authorization succeeds exclude only positively identified internal bridge sockets from XdmcpOpenDisplay sessionSocket selection. Exact AF_UNIX SO_PEERCRED PID and UID matching the retained parent identity can distinguish this bridge while preserving a Unix manager. Preserve all ordinary client accounting and authentication. Handle failed credential queries conservatively; never infer bridge identity from AF_UNIX alone. The owner should ensure parent identity lifetime/reparenting/PID-reuse assumptions are explicit; the Rust bridge already retains the owned engine pidfd and checks peer PID on every reconnect.
* **Epoch signal** can prevent eager window creation until the manager's own lifecycle connection is selected in each generation, but the existing one-shot marker is inadequate: it signals whichever authorized client won once, then closes. A reusable generation signal would itself need the identity guard so a bridge cannot manufacture readiness. Session socket selected is not equivalent to SecureDisplay finished; one legitimate pseudoReset of a bridge window remains possible and should be boundedly retried.
* **Retry** should encompass the entire reconnect + Endpoint::new sequence and reuse a single bounded recovery deadline/cancellation/child-liveness/security policy. Retry only transport/reset errors appropriate to the operation. Do not solve feedback by increasing reset limit or arbitrary delay.
* **Precise residual proof:** a private diagnostic build can record non-secret generation/state, selected session fd, and bridge-vs-manager peer identity at XdmcpOpenDisplay and XdmcpCloseDisplay, plus client-window kill mapping. Run the existing owned traced RED once to establish the predicted bridge selection, then the candidate and normal controls. Include initializer EOF regression and legitimate UNIX-manager case. This report does not claim that experiment was performed or that any fix passed.

The initial source-only phase ended here. The following section records the later explicit expansion of this task to native candidate implementation and validation; MAIN remains unchanged.


## Authorized phase 2: native candidate implementation

Root expanded this task to own the private native fix, dual build, and native controls. Rust recovery/caller files belong to `/root/xdmcp_clipboard_reset_fix`; real actual-app operational scenarios belong to `/root/current_packaged_xdmcp`. No MAIN edits or Git commits were made.

### Design and owned files

Production delivery files are under `source/packaging/x11/xdmcp/` in this report's directory: `manifest.json`, `readiness/xephyr.patch`, and `readiness/Xephyr-session-ready.xz`. Other readiness inputs were copied unchanged. Cumulative patch retains every existing session-ready and keyboard/RMLVO hunk and changes only native `os/xdmcp.c`, `os/connection.c`, and `os/osdep.h` for lifecycle classification.

The new **explicit** `-termcraft-internal-clipboard-parent` option must follow `-termcraft-session-ready`, may appear once, and is added by the Rust owner only for bidirectional clipboard. Without it, including keyboard-only feature sessions, `internalClientPidFd` remains -1 and native first-client selection is unchanged. The stock Xephyr artifact is unchanged.

On Linux, parsing this option captures `getppid()` and inherited effective UID, opens a parent pidfd, and verifies `getppid()` still matches after opening. Kernel pidfd_open creates a close-on-exec fd. The parser runs once before the server generation loop (`dix/main.c:138` versus loop143), so that single fd persists across generations without repeated allocation. Process exit closes it. Unsupported platforms fail explicitly on this feature option; this does not claim Windows/macOS support. Parent death cannot be mistaken for PID reuse: each classification checks both retained pidfd liveness and current parent equality. Effective UID is inherited by this unchanged unprivileged launch path; the matching credential must equal that captured UID.

After all existing ClientAuthorized checks have succeeded, the caller uses `XdmcpIsInternalClient(ClientPtr)` and skips **only** the XdmcpOpenDisplay notification when native GetLocalClientCreds reports both PID and UID equal to the retained live launching parent. This uses the existing credential helper, which verifies local transport before asking the kernel. Failed/unavailable credentials, mismatched identities, invalid/exited pidfd, and nonlocal clients retain native first-client behavior. The private cookie is still checked by existing MIT authorization. No client is exempted merely for being AF_UNIX. All ordinary SendConnSetup, nClients, ClientState callbacks, windows, and XKillClient semantics remain unchanged.

This prevents the internal bridge owning sessionSocket; it deliberately does not protect its window from xdm's cleanup. Bounded Rust recovery remains necessary for one legitimate SecureDisplay cleanup, including EOF during Endpoint::new. The one-shot readiness marker retains its original semantics and is not repurposed into a new generation or greeter-ready signal.

### Reproducibility and pins

| Artifact | SHA256 |
|---|---|
| Production build1 + independently clean build2 ELF | `81315f3d245b241a7fd8fd6ecf30a192edde0028bc43d5d0ba3d17b36889c92e` |
| Production cumulative patch | `6de42a3d543e69a8220fc52cd26f69d339c86e4c4befaa847c14025a6e2b31c8` |
| Production xz | `95201c0d7d93ae1f27d94f0cf2806f8db0d52f8d38330257d1062f48345037c3` |
| Original baseline + diagnostics ELF | `3a40955f1a0ec783dbb3c1805c137be480b877345fb917d1922274f058c392aa` |
| Candidate + diagnostics ELF | `ec3733c72d2789924cf2bde8a4f018fe8fa8d3463c1bc3fd293973d7c13ab7ca` |

Both clean production builds used the existing rebuild.sh, same pinned Xorg archive, xtrans headers and fixed `/mnt` build view. The entire prepared source trees match byte-for-byte (1661 files), independently of matching stripped ELF hashes; `prepared-source-file-hashes.json` records every prepared-source file. `artifacts.sha256.json` seals the input contexts, outputs, and native regression script/logs. No broad Cargo builds were performed by this native owner.

Diagnostic output is deliberately separate from production. `diagnostic-source/packaging/x11/xdmcp/` supplies its own cumulative patch, manifest, xz and pins for an explicitly diagnostic app build. It differs only by non-secret stderr messages: `TC_LIFECYCLE select generation=... fd=... peer_pid=... peer_uid=... engine_parent=...`, nonlocal selection, `close-reset generation=... fd=...`, and candidate `exclude generation=... peer_pid=... peer_uid=... engine_parent=...`. No cookie data is logged. Baseline diagnostic does not accept the new option; it requires the original caller. `diagnostic-baseline.patch` records its full source delta.

### Native controlled validation

`test-native-lifecycle.py` extracts the actual original/candidate XdmcpOpenDisplay/XdmcpCloseDisplay bodies (and candidate classification helper) into a small C harness. It uses real socketpairs, forks, AF_UNIX accept/connect, SO_PEERCRED, pidfd_open, and poll. The harness provides the native GetLocalClientCreds interface using kernel credentials; it does not simulate identity by rewriting peer PID values.

* `evidence/native-red.log`: original source fails the new required invariant that an exact launching-parent bridge must not select sessionSocket (exit134), captured before the implementation.
* `evidence/native-baseline-cause.log`: separate original-behavior control proves the real parent-created socket becomes sessionSocket; a distinct same-UID manager child connects and remains connected, then the exact native bridge-close callback sets DE_RESET. **Scope:** it invokes the callback reached by ProcKillClient; it does not claim to execute xdm pseudoReset in this small harness. The existing actual traces and manager logs above supply that complementary kill evidence.
* `evidence/native-green.log`: candidate excludes the exact parent; bridge close causes no lifecycle reset; a real distinct-PID/same-UID AF_UNIX manager remains eligible and its close still resets; option-disabled behavior remains original. It additionally checks pidfd CLOEXEC,64 repeated classifications without fd-count growth, and rejection of mismatched UID, mismatched PID, and an exited retained pidfd.
* `evidence/native-parser-checks.json`: executing the actual production ELF rejects missing-readiness and duplicate internal-clipboard options with the expected fatal error (exit1). Parsing occurs before a display connection, so these checks do not access a host display.

Two exploratory independent native chooser harness attempts are **excluded setup failures**, not product results: `native-baseline.QyR326Ql` used incorrect native -port/-indirect ordering; `native-baseline.pauzCyhz` reached the initial genuine chooser readiness but Python-Xlib did not send the wildcard-file cookie. Their logs are retained and no successful lifecycle assignment proof is claimed from them. The first orphaned owned engine was explicitly terminated after the failed cleanup; subsequent ownership cleanup was corrected. This runner was stopped when root assigned all operational BEFORE/AFTER scenarios to the dedicated operator, avoiding duplicate GUI scenarios.

### Remaining integration evidence

At this native handoff checkpoint the production assets and native tests are complete. The operator owns the actual diagnostic/fixed app runs, and the Rust owner owns full bounded-recovery regressions and the conditional caller flag. Their results must be linked into a cumulative review packet before claiming the overall application defect fixed. The original actual trace's precise native sessionSocket assignment remains inferred until an instrumented actual baseline records it; the controlled callback test and existing exact-window-kill correlation are not relabeled as an instrumented actual-app BEFORE run.
