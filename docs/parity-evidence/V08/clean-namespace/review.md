# Independent clean Linux namespace evidence review

2026-11-25. Verdict: **ACCEPTED for the stated bounded clean Linux browser/native-input qualification. Overall V08 remains open.** No actionable product or evidence-integrity defect was found in this scope. This review added only separate review artifacts and a separate replay directory; original evidence, original seals, source and package files were not edited.

Reviewed `REPORT.md`, `AUDIT.md`, export/extraction and rootfs-audit scripts, exact bwrap commands, all scenario scripts including preserved unsuccessful setup attempts, browser process capture code, input verification code, mount/status evidence, provenance, runtime package inventory, final native screenshots, headless DOM, HTTP logs and cleanup records.

## Independently verified

- Both seal indexes pass before and after replay: first index **60/60**, final index **64/64**, zero mismatches. Final-index SHA-256 is `76d9ca18c4412aaee3d81f2807dc64dfd8513925351cb6550a849518bd9f2b14`.
- Owned exported rootfs archive SHA-256 is `6353fa53c015793f83ee64ae1a335fe2e6cf5ff1649af3696d6269165ed61798`. All **27304** regular files match exported contents. All **3961** links resolve within the root; review additionally compares symlink targets with the extraction's prescribed relative rewriting and hardlink inode identity with their archive targets. No member is below an archive link ancestor. The only additional filesystem entries are the two empty mountpoint directories. Regular files have no setuid/setgid bits.
- Docker reports retained immutable image ID `sha256:1ab61a2bb16febd0a86829a43c26849b06ca82aca31be8385880cc7a3570c79d`. The recorded owned export container is absent. Export code creates the named owned container from exactly this ID, exports without executing it, and removes that same container in `finally`. No new export or container was needed for review.
- All **1415** package manifest file hashes pass. Archive hash is `68f356f63c92a9a6aced6abe3d845ab9d064e5da0d37b52ff0c4c99f0fc4b117`; manifest source snapshot identity matches retained release integrity, `4726eebadb56a15c2e9ed9f4018a63ffe9a55e9834ad927fea5472f2f2c29b53`. This is the existing default-13-games package, predating current advanced/save source work. It is not evidence for a newly rebuilt package.
- Runtime inventory byte-matches the prior declared-runtime image inventory. Exact mount arguments and recorded mountinfo show read-only exported root and package, fresh `/proc`, `/dev`, `/tmp`, `/home`, one writable evidence directory, cleared environment and private namespaces. No host home, libraries, display socket, credential directory or external network mount is introduced. Device helper nodes under bwrap `/dev` are expected; this is not a claim that the sandbox exposes zero host kernel/device interfaces. The host kernel is shared.
- Outer status records UID/GID 1000, zero capabilities and `NoNewPrivs: 1`. Outer seccomp is **0**; this is correctly not counted as renderer protection. The actual Chromium renderer records independently establish the nested browser sandbox: **five** renderers with UID/GID 1000, seccomp **2**, no-new-privileges **1**, and both user and PID namespace identities different from the Chromium parent. All **12** captured actual Chromium executable hashes match packaged `35d59a09075378c3c7ec9b60ebd46bb327563b87f6feb1365417fea4349db126`. No top-level `--no-sandbox` or `--disable-setuid-sandbox` bypass appears. Internally generated zygote/service flags are not misrepresented as protected renderers. Service/GPU/crash processes are outside the renderer-wide assertion.
- Sealed headless DOM contains `V08_SANDBOX_OFFLINE_OK`; the sealed probe reports exit 0. Sealed actual TermCraft screenshots visibly show embedded page `V08_NATIVE_BROWSER_OK`, input `V08_NATIVE_INPUT_OK`, and JavaScript result `Native typed: V08_NATIVE_INPUT_OK`. Corresponding loopback HTTP receipt query is logged. Its **404** is expected fixture behavior; the evidence demonstrates JavaScript/input delivery, not a successful receipt API. Same-app terminal screenshot and exact 28-byte `V08_NAMESPACE_NATIVE_PTY_OK\n` file demonstrate focused native terminal typing through the PTY.

## Independent bounded replay

From the app root, executed:

```sh
python docs/parity-evidence/V08/clean-namespace/review-checks.py > docs/parity-evidence/V08/clean-namespace/review-checks.log 2>&1
```

`review-checks.py` retains the exact independent hash, containment, provenance and sandbox assertions. It copies only `gui3.sh`, `inspect-gui3.py` and `browser-fixture.json` into the initially absent `review-replay/` folder, then executes the original `gui3-run.json` argument array with only the writable evidence mount source changed to that new folder. The full actual argument array is retained in `review-replay/exact-command.json`; no isolation or sandbox argument was relaxed. The retained rootfs and package are reused read-only. No broad scenario rerun, Cargo build, host policy change, root browser, privileged container, image deletion or system installation was performed.

Observed output (`review-replay/replay.log`):

```text
browser process records 15
V08_NAMESPACE_NATIVE_PTY_OK
```

Replay exits **0** in **24.91 seconds**. Independently rechecked executable hashes and nested protection again give **12** matching Chromium processes and **five** protected renderers. Loopback HTTP final input query and exact PTY marker bytes pass again. Replay browser-input and PTY screenshots were visually inspected and reproduce the claimed native application behavior. `review-checks.json` and matching log retain the machine-readable full result. First attempt stopped before replay solely because the review assertion expected uppercase Docker text; the local Docker CLI says lowercase `no such object`. `review-initial-check.log` preserves that review-harness diagnostic, fixed with a case-insensitive comparison; this does not indicate a scenario/product failure.

Owned app/server/Xvfb exit through the scenario trap; completed bwrap process and private PID-namespace teardown reap remaining descendants. Independent `/proc` scan finds **zero** bwrap processes for the owned rootfs. No review container was created. Original export container remains absent. Both original seals still pass after replay. `review-SHA256SUMS.json` separately seals review additions; it does not replace either original index.

## Exact qualification boundary

This accepts an alternative isolated Linux userspace success path for the specified immutable package on this host kernel, with private Xvfb, loopback fixture server, packaged browser and real application/native input. The prior Docker sandbox-blocked evidence remains valid and unchanged; this does not establish that Docker configuration as working. It does not establish physical hardware/GPU behavior, native physical-distro install/upgrade coverage, credential-vault upgrades, external network browsing, the current advanced/save source, a new release package, native Windows/macOS, original-product parity, or signing/notarization. **Overall V08 and release qualification are not closed.**
