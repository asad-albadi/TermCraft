# Final post-reset-fix bundle: browser and normal-app Mosh PASS

Fresh bounded qualification of exact archive **398d19fc6dbdf7d7dd5ee2a3a71ae8eda907bbbb31ad0117031c3a73fb106fe5**, production source **22bbf26840638474044431d242b92efa076123e87f39849cf0c8d967b33ecb04**, normal application **c8d09ba5fd2c36d567c6885f117d6068e2a87185648db883853cde84d678f7e6**. Both new normal-app workflows exit0 on first execution with the solved fixtures. No builds, helper fallback, source/MAIN changes, old packet mutation or XDMCP duplication.

## Exact identity and reuse bounds

`payload-equivalence.json` independently checks archive398d SHA, appc8d SHA and exact new extraction path. `verification-browser.json` freshly verifies all **3372** manifest regular-file hashes and expected source22bb. `source-equivalence.json` compares the actual packaged production-source maps: all **32** selected browser/Mosh/runtime/connection/process source and payload-manifest paths match prior f024 exactly. The complete596-file production map differs in only eight declared X11/readiness/clipboard files. All **2202** browser/Mosh payload files match f024 by SHA-256, size and mode with zero differences.

This permits retaining prior reviewed negative/tamper/cancellation matrices as prior bounded evidence; no full matrix replay occurred. `prior-packet-preservation.json` independently checks all **128** old sealed evidence files unchanged against the old SHA256SUMS index. New observations are exact398d/appc8d execution, not a relabel of f024 or diagnostic app982. Root owns genuine producer0/full-source-gate provenance; this packet does not claim that unfinished gate passed.

## Actual normal-app managed browser

`browser-current-result.json`: exit0. New appc8d launches its configured embedded BundledChromium. Visually inspected screenshot shows initial loopback page marker, exact typed `V08_NATIVE_INPUT_OK`, and page-JavaScript native typing echo. Fresh HTTP log shows initial document200 and exact input receipt. App window resize produces distinct browser viewport receipts **781x334 ->1021x454** (initial1021x440 also recorded). The minimal fixture intentionally returns404 for receipt/resize paths; their exact request query proves event delivery. Same actual app's native terminal accepts typing and writes exact `V08_NAMESPACE_NATIVE_PTY_OK\n`.

Actual executable and sandbox sampling records **five renderers** hashing to packaged Chromium35d59a09075378c3c7ec9b60ebd46bb327563b87f6feb1365417fea4349db126, all Seccomp2/NoNewPrivs1, each in nested user/PID namespaces distinct from browser parent. No system Chromium or sandbox bypass is used. `browser-current-cleanup.json`: zero owned app/browser clients after shutdown.

Exact bwrap invocation is retained: --unshare-all, UID/GID1000, capabilities dropped, read-only retained declared-runtime root and **new398d package**, private /proc,/dev,/tmp,/home, newly empty private HOME, own Xvfb and loopback HTTP. No host input/display/clipboard/home/credentials/network mount or policy weakening.

## Actual normal-app Mosh, real SSH PTY and private clients

`mosh-result.json`/`verification-mosh.json`: exit0. Native process capture additionally proves the actual running `/opt/termcraft/harbor-workspace` hashes to **appc8d**. New normal app uses `--session`, then real native terminal typing. Independent strict exact known-host-key/publickey SSH bootstrap allocates a **real SSH PTY**; fresh encrypted Mosh UDP terminal input writes exact `CURRENT_MOSH_ENCRYPTED_UDP_PTY_OK\n` and `CURRENT_MOSH_TYPED_UDP_OK\n`. Remote `stty size` changes **18x118 ->9x87** after native app resize. Visually inspected screenshot shows actual remote execution, visible marker and returned prompt.

25ms bootstrap sampling directly captures private `/tmp/termcraft-mosh-*/bin/perl`, private `bin/ssh` and private mosh-client ELF hashes/maps/environment. Each equals the corresponding new packaged file; **all three actual processes have empty PATH**. Container-local installed Perl, SSH client and Perl module directories are removed/asserted absent. No system/host-client fallback, helper substitute or --no-ssh-pty is used.

Own Docker container `termcraft-post-reset-mosh-398d` uses immutable declared-runtime image1ab61a2bb16febd0a86829a43c26849b06ca82aca31be8385880cc7a3570c79d, --network none, default capabilities, private PID/IPC/mount namespace and own private Xvfb. App runs UID1000 with fresh private HOME (distribution defaults only). Root sshd, generated client/server keys and trust file are container-private. Retained genuine mosh-server/libutempter are read-only **server-only fixtures**; exact new398d package is read-only; only this packet's own evidence mount is writable. No privileged mode, host devices/display/credentials/home, security override or external service.

Zero owned client processes remain before container termination; the exact owned container is absent afterward. Immutable image/old fixtures and unrelated processes/containers are retained. No reruns/setup failures were needed for this final packet.

## Limits

Arch-compatible Linux x86_64/glibc2.44 declared-userspace smoke only. OS glibc/kernel/display requirements remain. This does not qualify arbitrary distros, hardware/GPU, external networks, arbitrary SSH configuration/auth providers, Windows/macOS, signing/notarization, full V08/original parity or root's pending complete gate. Actual XDMCP reset behavior is a separate worker's scope. Prior V08 sandbox-blocked Docker Chromium observations remain historical limitations; successful native Chromium here uses the independently isolated bwrap route.
