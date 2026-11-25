# Private ordinary-debug XDMCP fix regression qualification

Result: four minimal AFTER GUI cases passed. These are explicitly private debug installations with newly compiled native assets, not the original f024 archive, a31aa archive GUI run, or a genuine release qualification. Root must independently review/integrate the fix and build/qualify the final release package.

## Exact identities

| Private profile | Ordinary debug app SHA-256 | Native feature SHA-256 | Canonical captured source BuildIdentity |
|---|---|---|---|
| diagnostic982 |616bc73220393874980baf946f5cd573cdbbc89c350bec56b6276b2a12c19593 |ec3733c72d2789924cf2bde8a4f018fe8fa8d3463c1bc3fd293973d7c13ab7ca |f85dd11f0fa2c02941fd5868d0c7327a880b8a43e725a78808b48d4421dbc479 |
| production-native981 |edef4919d431ff3c4b05c1b2ff64c7345a57d6acc1a07188e623c14371bddec2 |81315f3d245b241a7fd8fd6ecf30a192edde0028bc43d5d0ba3d17b36889c92e |522b8148978241b9503bc69b2c4147e543cf404086644a5ead41eab1e8b932e4 |

Normal binaries came from target/validation-rust1961/debug/termcraft-xdmcp-reset982 and termcraft-xdmcp-reset981. Source roots are target/agent-work/xdmcp-clipboard-reset-fix/source-diagnostic982 and source. Diagnostic source-capture file SHA is52870b62e7e618588e1266cc324ec14730618a5b74728d74385f71ee916ddf64; production final capture SHA is716e1ad5651ced1cd0e7a2bfafc8d6c7ece1016e613d244c6fc4c7d8d6c2f8f9. These file hashes differ intentionally from canonical JSON source-input digest in the table. Both owner identities and exact Cargo commands/results are copied into their profile directories. The owner built ordinary normal debug targets with private versions0.1.982 and0.1.981; no test executable was used as the GUI. Diagnostic982 is frozen before production981's additional owner-conversion typed-error regression; they are distinct source identities.

Each experimental-installation/EXPERIMENTAL-MANIFEST.json states profile debug and private experimental provenance. The independent experimental-allowlist.json pins its exact manifest SHA, normal app SHA, both native executable SHAs, full captured source-input map and canonical BuildIdentity. verify.py checks every source-map byte, every installation file hash, private profile and exact app/native pins before launching. The app's source executable.sha256.bin and engine manifest were checked against the actual staged native bytes. The actual app and live sealed memfd engine hashes were checked at runtime; all four immutable seals15 and TCP ownership remained required. Stock Direct companion retains the originald7f1f49ee428fd06b26707a79ba6d65b4e9ada4e6b53aca4146baaa92c79e593. Installation files are read-only inside the namespace. The existing normal release verifier was never edited or relaxed, and no manifest was relabeled release after substituting private assets.

## Minimal AFTER cases

| Case | Evidence directory | Result | Selected-manager marker observation |
|---|---|---|---|
| diagnostic982 traced0/0 |diagnostic982/evidence.lMAOxgSp |exit0 |0.123s |
| diagnostic982 untraced11/11 |diagnostic982/evidence.fuspoWet |exit0 |0.095s |
| production-native981 traced0/0 |production981/evidence.7Eb8I9Pn |exit0 |0.153s |
| production-native981 untraced11/11 |production981/evidence.awth8kAI |exit0 |0.095s |

Holds are pending-chooser/selected-chooser seconds. Each case launched three actual normal debug app processes from their own --data-dir/--session saved workspaces: a concurrent stock Direct178 companion, a pending German bidirectional Indirect176 chooser canceled from the GUI, and selected German bidirectional Indirect177. The real pinned xdm chooser accepted a target at UDP1177 and forwarded it to the selected real manager at standard UDP177. The same owned feature PID and exact feature digest survived reset; its negotiated setup client mapped a green800×600 window; German29/52 returned122/121. Exact39-byte and47-byte UTF-8 clipboard payloads passed in both directions, and an incorrect MIT cookie was rejected. The production981 traced0/0 case repeats the same timing configuration and syscall observation family that exposed the baseline RED; tracing additionally captures write for diagnostic stderr observation.

Every visible pending/selected/Direct cancel action reaped only its own engine, removed its app authority, X socket and owned TCP listener. The Direct companion's engine/listener and original private parent remained alive while the other sessions were canceled. Complete parent XKB maps matched before/after. All three normal debug apps in each case then exited0 after WM_DELETE_WINDOW; the parent remained responsive. test-results.json records individual observations and identities. The caller flag -termcraft-internal-clipboard-parent was required only for Bidirectional clipboard and absent for the stock disabled-clipboard companion.

Screenshots were retained. Diagnostic selected and production untraced selected screenshots were inspected and show established display session with bidirectional clipboard active. One production traced selected screenshot captured a black frame before repaint; its pending screenshot and production untraced selected screenshot rendered correctly. That one image is not asserted as visual status evidence. The actual UI control, known from inspected matching geometry, still performed verified cleanup in the traced case. A legacy final completion line in the first diagnostic private log says “ordinary release GUIs” because it was inherited from the original probe; the unmodified private manifest/verifier headers and actual ELF identities establish debug throughout. Subsequent probes were corrected to say PRIVATE EXPERIMENTAL normal-debug. No release claim follows from that inherited line.

The persistent fixture differs from the earlier original fixture only by keeping its authenticated green setup client alive120s instead of20s. This avoids a transition into the unqualified PAM greeter during the bounded lifecycle observation. All manager security/choice-timeout settings, native auth, peer/pidfd/deadline/cancellation checks and socket gates remain active. No host display, bus, input device, service or HOME was used or changed; synthetic input and tracing stayed within the owned namespaces. Private raw traces may contain X11 authorization bytes and must not be printed publicly.

## Causal proof and BEFORE bounds

Actual BEFORE evidence remains ../reliability-boundary/evidence.BWIRaaYM and evidence.HeBm4oYc: genuine original f024 app9aad/native8cf with two traced normal-chooser failures, each observed75s. Native WILLING was received and REQUEST/MANAGE sent repeatedly; bridge EOF during GetSelectionOwner recovery or its three-reset limit propagated to the app, which sent SIGTERM to its own engine. This was app-initiated teardown, not an unexplained native crash or ten-second driver cutoff. All old evidence is left unchanged.

The additional original instrumented baseline app build was explicitly skipped with Root authorization. The C owner's controlled evidence at target/agent-work/xdmcp-c-lifecycle-diagnosis/evidence/native-baseline-cause.log exercises extracted original native functions: original first-client selection retains the parent bridge socket, a separate-PID Unix manager remains live, and native XdmcpCloseDisplay on the bridge sets DE_RESET. This is a source-function causal test, not an actual full xdm pseudoReset or a captured original TC_LIFECYCLE GUI trace. Exact bridge assignment in the old original GUI traces remains an inference supported by this controlled proof; it is not invented as a recorded baseline diagnostic.

The actual diagnostic982 AFTER trace supplies the live counterpart. diagnostic982/native-lifecycle-observations.json decodes only TC_LIFECYCLE messages and asserts:

- The actual app PID115/UID0 is excluded from lifecycle selection in generation2 before the selected manager's sessionSocket is chosen.
- The parent bridge is never recorded as the selected lifecycle owner.
- The generation2 selected manager fd3 matches a native AF_INET accept. Linux TCP SO_PEERCRED diagnostics show PID0/UID4294967295, so the transport is established by accept, not treated as a real local PID.
- There is the expected generation1 close-reset when the chooser ends, then stable selected-generation clipboard work until the later owned cancel. Generation2 close-reset appears during the explicit selected-session cancellation, not a startup feedback loop.

The C owner's separate real-child Unix manager test in evidence/native-green.log supplies the distinct positive control that a different PID authenticated over Unix still counts as a native lifecycle client. That control is not mislabeled as this GUI's TCP manager. Its other native peer identity/pidfd/flag-off controls and the Rust bounded bootstrap/typed-retry tests remain owner evidence, to be independently reviewed with their packets.

These results support the reviewed candidate behavior under the formerly failing traced ordering while keeping authentication and ownership active. They do not prove the exact cause of the historical first run-2 missing-REQUEST timeout. No arbitrary sleeps, larger reset limits, protocol interposers, firewall changes, credential bypass or family-wide Unix exclusions were introduced by this operator.

## Preparation exception and seals

A production staging preflight correctly rejected the obsolete initial capture map after the owner added the final typed-error regression. No GUI launched in that preflight. The operator then selected the owner-published final capture map716e1ad5 and reverified every source file before generating the private manifest. production981/traced-00-preflight.log preserves the subsequent expected missing-allowlist error; actual runtime cases use the final correct source identity. It is an operator preparation failure, not a product/runtime failure.

Four new GUI cases only; no repeat of the old stress matrix. No product source or MAIN file was edited by this operator. Old202+62+608 evidence seals were rechecked unchanged. This directory has its own artifacts.json. Root remains responsible for independent review, production-source integration and genuine final package build/qualification.
