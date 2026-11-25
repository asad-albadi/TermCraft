# V04 controlled Mosh and serial evidence — partial

Observed 2026-11-25 on Linux x86_64. Two actual pinned Mosh encrypted-UDP cases and three real-PTY production-serial tests passed. V04 remains incomplete. `strict_native_original=0`; physical serial, physical network-adapter roaming, and native Windows/macOS/original runtime remain unavailable. No product source or checklist status was changed.

| Cell | Actual observation | Status / limit |
| --- | --- | --- |
| Mosh source IP and UDP port rebinding | Server authenticated `127.0.0.2:47969` then `127.0.0.3:51102`; 1.2-second bidirectional loss and actual packet reorder | Passed, owned UDP relay in private namespace |
| Mosh private virtual interface/address migration | Created two dummy interfaces; deleted old interface; server authenticated `10.203.0.1:59291` then `10.203.0.2:54902` | Passed, virtual source-interface migration through relay, not physical adapter roaming |
| Persistent Mosh state | Each case retained one command process and journaled `before`, `during_loss`, `reorder_one`, `reorder_two`, `after` exactly once, counters 1–5 | Passed, no duplicated or missing command |
| Mosh cancellation and owned cleanup | Client escape disconnect completed in about 0.0401 seconds; owned server and adopted descendants reaped | Passed, bounded 3-second checks |
| Serial PTY byte transfer and idle close | Actual copied production serial/input-queue sources, original real-PTY fixture tests | Passed |
| Serial queue pressure, synthetic unplug error, all-handle cleanup | Three selected tests executed; 0 failed/ignored/filtered | Passed, PTY simulation only |
| Physical serial loopback/settings/flow/unplug/reconnect | No designated device/adapter/wiring | Unavailable; no hardware was opened |
| Physical network adapter / native Windows/macOS / original Personal 26.6 | No designated supported fixtures | Unavailable; not inferred from Linux evidence |

The relay forwards the unmodified packets between the authenticated pinned `mosh-client` and pinned `mosh-server`; it has no key, decryption or terminal implementation. The test sends input during an actual loss window, discards eight packets, then delivers at least one pair of client datagrams in reverse arrival order. A server-side journal independently proves the command stream; Mosh-rendered output confirms the corresponding terminal acknowledgments. Verbose upstream server logs independently record both authenticated peer addresses. This exercises actual SSP/Mosh recovery while retaining the same client/server processes. It does not invoke TermCraft's UI or SSH startup adapter; existing P01/P08 SSH/startup evidence is complementary. It does not measure UI behavior, physical adapters, external routers or a different platform.

The dummy interface addresses are local to one private Linux network namespace. Kernel local delivery still uses the local route/loopback; this does not claim packets traversed a physical link or moved between independently routed hosts. `network_before`/`network_after` capture the real virtual interface change. Both tests use a private user/network namespace created with `unshare`, whose identity must differ from the captured host namespace. The runner never changes host routes, interfaces, SSH settings, keys, credentials or user files. Runtime sessions use explicit owned commands and private temporary files, with test-only Mosh keys redacted from preserved logs.

## Reproduce

From an arbitrary directory, run with absolute paths (replace `APP` and use a new `OUTPUT` directory):

```sh
python APP/docs/parity-evidence/V04/runner/check_v04.py APP OUTPUT --require-complete
```

The strict command exits **2**, with a `partial` report, after successfully running all five feasible observations; the three unavailable matrix cells prevent whole-V04 success. Without `--require-complete`, exit 0 means only the five explicitly scoped fixtures executed successfully; `summary.json` still says `partial`, `whole_v04_complete=false`. Fixture/assertion/dependency errors return failure and are never converted to an available/pass cell. No empty, ignored or filtered serial run is accepted. Three negative controls prove the validator rejects an unavailable result relabeled as pass, a zero-executed serial run and an ignored serial test.

Required tools are Linux `unshare` user/network namespace support, `ip` including dummy interfaces, Python 3 with PTY support, `zstd`, `ldd`, and the existing offline Rust dependencies. Missing capabilities fail the runner. The pinned binary/source archives and signature are existing inputs under `APP/target/runtime-inputs/`. The package SHA-256 must match `packaging/runtime-manifest.json`; executable members are extracted directly from that exact authenticated input, not selected from PATH. The Mosh source and GPL license hashes are independently checked; the recorded signer fingerprint references the previously verified package input, not a newly performed signature verification.

The serial harness copies the exact production `src/serial.rs`, settings, input queue and original PTY fixture, and copies the three tests starting at the original `serial_fixture` module declaration. Minimal module wiring selects `InputQueue` without compiling unrelated UI/transport code. It uses isolated package `termcraft-v04-private` version `0.1.904`, shared target artifacts, `CARGO_BUILD_JOBS=2`, and touches its crate root before build. Source drift fails against `runner/serial-base.json`. This is selected-source production serial evidence, not a full application regression run.

## Provenance and observations

- `observed/summary.json`: exact partial outcome.
- `observed/strict-run.exit`: observed strict exit 2.
- `observed/serial.log`: executed 3/3 tests, no ignored/filtered tests.
- `observed/serial-provenance.json`: exact MAIN source, Cargo lock, Rust compiler and actual emitted test executable hashes.
- `observed/serial-source/`: exact copied serial fixture source and minimal generated harness.
- `observed/mosh/provenance.json`: package/source/signature/license/executable/runner hashes, Mosh version, Linux identity, all actually linked native-library hashes.
- `observed/mosh/results.json`: source-address observations, packet loss/reordering counters, per-command persistent PID/counter journal, cancellation timings and cleanup result.
- `observed/mosh/*.server.log`: upstream authenticated peer transitions and protocol state logs; test keys redacted.
- `observed/mosh/*.terminal.bin` and `*.journal.jsonl`: actual PTY output and independent server command history.
- `physical-fixture-request.json`: explicitly incomplete future hardware observation fields, not an available/pass result.
- `HANDOFF.json`: normalized unchanged-source base/candidate hashes and hashes for every added evidence file.

No P04 implementation defect was observed in these three PTY tests. This evidence leaves V04 open until designated physical serial and physical roaming/native requirements are actually observed and reviewed.

## FIX1 — bounded failure cleanup (2026-11-25)

Independent review found that startup timeout before server-owner parsing could leave the launcher alive, and the outer timeout could leave detached descendants alive. `FIX1/red.json` reproduces both defects against the exact original runner; both probes were explicitly killed/reaped after observing RED.

The revised runner scopes every launched command using child-subreaper ownership, excludes pre-existing children, follows only owned descendants/adopted children and signals through generation-pinned Linux pidfds. Cleanup sends TERM then KILL, reaps owned children and fails if the three-second cleanup bound is exceeded. Ordinary termination unwinds these scopes. This includes launcher timeout before owner assignment, detached-server startup parse errors, outer namespace timeout, and helper commands. This requires Linux pidfd support and Python `os.pidfd_open` / `signal.pidfd_send_signal`. The sequential fixture runner must not start concurrent unrelated child work within a scope.

`runner/test_cleanup.py` ran seven focused probes successfully, including the actual `run_case` function extracted from the candidate source. The probes verify startup failures, timeout and detached descendants in a real private network namespace, nonzero exit, termination unwinding, unchanged pre-existing child survival, and exact stdin/stdout helper bytes. These probes create only owned sleeper processes; no Mosh/serial matrix was repeated after the cleanup-only edit. The original successful Mosh and serial observations remain unchanged and describe the original runner hash. `FIX1/baseline/HANDOFF.json` preserves the exact original 33 hash records and `FIX1/baseline/` retains both original runner files. The updated handoff distinguishes this cleanup-tested candidate from the original successful-observation baseline. V04 remains partial with `strict_native_original=0`.

```sh
PYTHONDONTWRITEBYTECODE=1 python APP/docs/parity-evidence/V04/runner/test_cleanup.py
```
