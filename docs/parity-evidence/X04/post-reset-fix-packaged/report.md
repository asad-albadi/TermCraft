# Genuine fixed Linux release XDMCP qualification

Result: both required minimal normal-release AFTER cases passed, with no new failure observed. This report qualifies the exact398d archive and scoped Linux workflows below. It does not automatically advance X04 or change the project count46/60.

## Exact genuine artifact

- Archive: target/package-post-xdmcp-reset-fix-reviewed/TermCraft-0.1.0-linux-x86_64-arch-xwayland-xdmcp-mosh-unix-chromium-wayland-sharing.tar.gz.
- Archive SHA-256 independently verified before launch and again after the runs:398d19fc6dbdf7d7dd5ee2a3a71ae8eda907bbbb31ad0117031c3a73fb106fe5.
- Strict root extraction: target/package-local-completion-qualified/398d19fc6dbdf7d7dd5ee2a3a71ae8eda907bbbb31ad0117031c3a73fb106fe5/TermCraft-0.1.0-linux-x86_64-arch-xwayland-xdmcp-mosh-unix-chromium-wayland-sharing.
- Canonical source snapshot:22bbf26840638474044431d242b92efa076123e87f39849cf0c8d967b33ecb04.
- Ordinary release app SHA-256:c8d09ba5fd2c36d567c6885f117d6068e2a87185648db883853cde84d678f7e6.
- Production feature engine SHA-256:81315f3d245b241a7fd8fd6ecf30a192edde0028bc43d5d0ba3d17b36889c92e.
- Stock engine SHA-256:d7f1f49ee428fd06b26707a79ba6d65b4e9ada4e6b53aca4146baaa92c79e593.

The new verifier explicitly requires profile release, local locked release build provenance, this source-input digest, this actual app digest, all 3372 manifest file hashes and the independently copied current production engine pins. It does not accept the old d6 source, old8cf native engine or private debug manifests. The actual process executable was hashed and matched c8d0. Its live feature child was executed from a sealed memfd whose SHA matched production81315; all four immutable seals 15 were required. The feature hash/PID was rechecked after the real chooser reset, and TCP LISTEN inode ownership was checked against that child's fd table. No package bytes, root source or extraction files were modified. No compilation or substitute app/native asset was used.

archive-identity.json records absolute paths and the exact PACKAGE-MANIFEST hash. Package production/build exit evidence and the independently qualified RDP helper belong to Root's packaging/startup packet; this probe independently verified their package inventory bytes but does not claim to rerun the helper workflow.

## Two cases and observations

| Case | Evidence | Selected app/native PID | Marker observation after chooser helper | Exit |
|---|---|---|---|---|
| traced0/0 |evidence.PrMfQ25S |115/156 |0.126s |0 |
| untraced11/11 |evidence.agqoL0kd |113/154 |0.128s |0 |

Numbers indicate pending-chooser and selected-chooser idle hold seconds. In each fresh namespace, three actual archive CLI invocations used --data-dir and --session with separate owned saved workspaces. A Direct disabled-clipboard stock session at178 negotiated the real manager and remained alive while a pending German bidirectional Indirect chooser at176 was visibly canceled, then a German bidirectional Indirect chooser at177 accepted a real target. The selected manager received the forwarded client at UDP177 from the actual initial chooser manager on UDP1177. The selected negotiated setup client mapped its green 800×600 window; the same feature PID survived reset. German keycodes 29/52 returned 122/121, and the complete private parent XKB map stayed byte-identical.

Independent parent/nested clients transferred exact 39-byte and 47-byte UTF-8 strings in both directions, including accented, Arabic, Japanese and emoji characters. An incorrect MIT-MAGIC-COOKIE-1 was rejected. The selected app screenshots in both cases were inspected and showed “XDMCP display session :177 established with bidirectional clipboard active” and its displayed Cancel / disconnect XDMCP control. They rendered correctly. This status concerns the display session; no PAM/user-login claim is made.

Each pending, selected and Direct visible cancel action removed its owned engine PID, app authority, X socket and TCP listener. The Direct companion's engine/listener and original private parent PID/socket identity remained intact while the other sessions were canceled. The app stayed alive after its cancel. Each of the three actual release apps then exited0 after graceful WM_DELETE_WINDOW; the private parent remained responsive until its own wrapper cleanup. The parent-before.xkb and parent-after.xkb files match in both cases.

## Flags, protocol timing and fix boundary

The feature engine used -termcraft-session-ready and -termcraft-internal-clipboard-parent only for the Bidirectional sessions. Actual selected flags included :177 -port 1177 -indirect 127.0.0.1 -from 192.0.2.1 -listen tcp -nopn -screen 800x600 -termcraft-keyboard-layout de -auth OWNED_AUTHORITY -displayfd 1. Indirect omitted -once; Direct used -query with its normal selected numeric endpoint and no internal-parent flag. Never -ac. The saved sessions explicitly allowed the inbound listener in the private network, and retained normal selected-endpoint/hostname validation. engine-identities.jsonl in the untraced evidence records the full actual argv, digest, PID and seals for each engine. The traced process exec record independently captures these feature flags; its argv0 path is truncated by strace's128-byte string limit, while the app/executable digest verification is complete.

traced-protocol-observations.json contains only safe six-byte native XDMCP headers and non-secret executable flags decoded from the private trace. The selected engine actually received WILLING and sent REQUEST 101 microseconds later, then received ACCEPT and sent MANAGE. It performed the expected two query epochs: initial chooser and selected manager. It did not enter the former repeated reset/teardown feedback. Selected-window marker observation followed promptly in both cases; the ten-second diagnostic boundary was never reached. The native production asset has no TC_LIFECYCLE instrumentation, and no such diagnostic evidence is invented for this final release.

The earlier original f024 traced REDs, bounded historical diagnosis, controlled native baseline proof, and explicit diagnostic982/production981 private-debug AFTER experiments remain separate under target/agent-work/current-packaged-xdmcp. Their report explains the actual app-initiated bridge failure and reviewed local fix. These two fresh genuine release cases repeat the formerly failing traced ordering and delayed untraced control with the integrated production assets. This is finite regression coverage, not proof of every possible scheduling order. The precise cause of the first historical missing-REQUEST run-2 timeout is not retroactively asserted.

## Isolation and preserved scope

The same pinned persistent fixture retained its authenticated green setup client for 120s to avoid the earlier 20s setup exit/PAM-greeter flood. No manager security or choice-cache settings changed. Parent Xvfb, real xdm managers, private UDP177 capability, listener addresses, synthetic XTest input and the tracing tool were confined to owned user/PID/network/socket namespaces. The wrapper verified that network and parent socket-directory identities differed from the host. The apps used private HOME/config/cache/data and owned workspaces; host display, clipboard, bus, input devices and services were not used or changed. Read-only host libraries/keyboard catalog mean this remains configured-host evidence, not a clean-distribution result. No host installation occurred.

Exactly two fresh GUI cases were executed. No new stress matrix, source edits, package edits or checkpoint edits occurred. The prior202+62+608+397 evidence seals were independently rechecked unchanged. This directory has its own artifacts.json. Physical input, OS/PAM login, remote logout policy, other platforms and the overall X04 acceptance boundary remain separately scoped.
