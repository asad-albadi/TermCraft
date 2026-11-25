# B03 managed SSH/SFTP and Iperf implementation

Private implementation: `target/agent-work/b03`, package version
`0.1.0-b03-private`. Production source was not overwritten. Integration entry points
are in `shared.patch`, original shared-file SHA-256 in `base-hashes.json`, and the
new owned-file copy list in `owned-files.json`. Root must apply only the narrow
shared hunks against its newer P06/P05 source, then perform its merged review/gate.

## Implemented behavior

- B01-owned `start_ssh`/`begin_ssh`, `start_iperf`/`begin_iperf`, ServiceId/status/stop.
  Servers cards use asynchronous startup and display running address, failures,
  stop controls, and static bounded log events. Secrets are transient zeroizing
  fields and are never ordinary persisted JSON or authentication log content.
- Embedded russh 0.65.4 SSH and russh-sftp 4.0.0 SFTP. Explicit per-user password
  and/or OpenSSH public keys, verified signed-key authentication, denied unknown
  users, three auth attempts per connection, eight simultaneous connections and
  four channels per connection. Host key generation occurs in the service worker;
  KeyManager publishes its private Ed25519 bundle atomically and restart reuses it.
- Optional SFTP capability: capability-opened root, no symlink traversal, bounded
  handles/directory pages/packet sizes. Real reads, list/stat, new-file staged
  uploads, rename, remove, mkdir/rmdir. Read-only is default. Upload limit is 64 MiB;
  overwrite, append/random-access write, chmod/times, links and extensions are
  deliberately unsupported. Failed writes invalidate their staging handles. Stop, channel closure and parent disconnect
  cancel actual SFTP request-loop streams and clean unpublished temporary files. A share may not contain
  or overlap the server host-key bundle, preventing key disclosure or mutations.
- Optional PTY shell requires explicit full-current-account authority and absolute
  shell selection. Virtual usernames do not impersonate OS users or confine the
  shell. PTYs are constructed/owned/destroyed on their worker, with bounded input
  and output, resize, idle and total lifetime bounds. SSH exec and environment
  requests are explicitly rejected; forwarding is not enabled.
- Iperf2 2.2.1 and Iperf3 3.19.1 real foreground engines, explicit major version,
  listen address/port, report interval, Iperf2 TCP/UDP selection. Iperf3 clients
  select transport. Loopback default; non-loopback requires explicit opt-in. Child
  output is discarded and stop kills/waits only that owned child. The verified
  packaged provider is default; explicitly selected installed compatibility is
  separately logged and major-version checked. Corrupt packaged bytes cannot
  silently trigger installed fallback. No system sshd config, daemon, or global
  process-kill operation is used.

## P01 qualification and packaging

`runtime/linux-x86_64/` contains two engines, corresponding source tarballs and
complete licenses (~2.6 MiB total). `packaging/iperf/runtime-manifest.json` pins
engine, source and license hashes. RuntimeCatalog checks platform/native image and
hashes; Linux execution uses the existing sealed VerifiedArtifact command.

The payload is qualified **Linux x86_64 on the current GCC 16.2.1 / glibc 2.44 host
ABI**, not a universal Linux artifact. Iperf2 also depends on host libstdc++ and
libgcc_s; Iperf3 links libiperf statically but libc/libm dynamically. The package
manifest records these actual linked-library requirements. No native Windows or
macOS Iperf engine or execution evidence is claimed. Those systems expose an
unavailable packaged provider unless an installed compatibility provider is
explicitly selected. Native Windows/macOS SSH/SFTP/PTY execution was not performed.

`stage-runtime.py` validates exact binary/source/license bytes. The release
packager adds `--iperf-runtime PAYLOAD_ROOT` to include the qualified payload and
runtime dependency inventory. Root should supply this flag for its B03 release;
it does not turn a missing provider into a bundled claim. Source rebuild recipe
and host ABI are recorded in `packaging/iperf/README.md`; rebuilt binaries require
new binary qualification rather than automatic acceptance of new checksums.

Official references consulted:
- https://software.es.net/iperf/invoking.html
- https://iperf2.sourceforge.io/iperf-manpage.html
- Pinned local russh/russh-sftp server Handler/Config/run_stream source and examples.

## Evidence

Private logs (all paths relative to `target/agent-work/b03`):

- `red-api.log`: exact adapter API regression checked against baseline MAIN library
  under separate `0.1.0-b03-red-private` identity; expected compilation failure for
  absent SSH/Iperf configuration and start methods. The earlier `red.log` failed on
  an omitted private asset fixture and is **not** regression evidence.
- `final-targeted.log`: `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=<main>/target cargo test
  --offline --test service_adapters -- --nocapture`: 10 passed, 0 failed. Actual SSH
  allow/deny/password/signed-key handshakes, SHA256 server fingerprint restart,
  SFTP transfers/mutations/read-only/escape/partial-stop cleanup, opt-in PTY shell,
  external `/usr/bin/sftp` batch client, both Iperf major-version TCP and UDP client
  traffic, port release, missing/tampered providers and safe listen defaults.
- `supervisor-regression.log`: existing `cargo test --offline --test services --
  --nocapture`: 17 passed, 0 failed.
- `packaging.log`: Python unittest `packaging_iperf.py`: 3 passed, 0 failed. Includes
  actual release archive creation with both engines/source/license/library records,
  binary/source/license tamper rejection and no overwrite of staged files.
- `full-fmt.log`: `cargo fmt --all -- --check` passed (empty output, exit 0).
- `iperf2-{configure,build}.log`, `iperf3-{configure,build}.log`: official source
  engine builds. No system package or daemon installation occurred.

The full merged Rust/native release gate belongs to root after integration; it
was intentionally not rerun from a stale private snapshot. No progress/checklist
row was changed by this worker. A fresh reviewer is still required.

## Decisions and practical limits

- New-file-only, sequential SFTP uploads retain B01 conflict safety; clients asking
  for combined read/write, append, overwrite, links or metadata mutation receive
  unsupported/failure. Cost: such advanced workflows need a separate extension.
- A shared folder must be separate from host-key storage. Cost: sharing an entire
  home/data ancestor containing that key bundle requires choosing a narrower folder.
- SSH shell access is optional full authority under the workspace OS account.
  Cost: it is not OS account impersonation or an SFTP-root-constrained terminal.
- Linux-only Iperf qualification is stated explicitly. Cost: other host ABIs/native
  platforms require independently qualified engines or installed compatibility.
- Server-side Iperf output is discarded to keep supervision bounded; clients
  display measurement results. Cost: Servers does not retain throughput history.

Final worker cleanup: all own command sessions ended and no adapter/Iperf server
process remained. The main unversioned application binary was restored to its
original SHA-256 `8d06f43a7a9626c1991dc7922736dc751dff0094f68b6f5d3f5edc9bdc6c46e3`.


## Important review fix — actual SFTP worker ownership

The fresh review correctly found that pinned russh-sftp 4.0.0 `server::run` only
schedules its real loop. The original wrapper JoinHandle therefore did not own
actual work. The fix changes only private `src/services/sshd.rs`; packaging, UI,
shared integration patch and all other reviewed owned files retain identical bytes.

The parent SSH handler now keeps per-ChannelId controls counted until a lease
inside the **actual library-loop handler** is destroyed. That lease is declared
after open handles so staging and file handles drop before admission is released.
A stream wrapper polled directly by the real request loop registers cancellation
and the real 3600-second deadline. Session Drop/channel close/EOF wakes cancellation,
and an idle read is awakened by its own deadline. Neither loop lifetime nor channel
admission relies on completion of the scheduling `run` future.

Focused evidence in `target/agent-work/b03`:

- `ownership-red.log`: live retained-four-SFTP/fifth-channel regression failed
  against the reviewed implementation before the fix (one failure, expected).
- `ownership-green.log`: 3 focused live unit tests pass: four retained channels
  deny a fifth, six close/reopen cycles retain that bound; parent SSH disconnect
  releases unfinished upload staging while client objects remain alive and the
  service remains Running; the actual pinned spawned SFTP loop obeys cancellation
  and a short test deadline, releases its handler lease and staging handles.
- `ownership-targeted.log`: existing 10 adapter and 17 supervisor tests pass after
  the fix, with real OpenSSH SFTP and both Iperf 2/3 TCP/UDP interoperability retained.
- `ownership-fmt.log`: full private formatting check passes (empty output).

`review-fix-base-hashes.json` records the originally reviewed SSH file hash;
`handoff-hashes.json` is refreshed with the fixed file. `shared.patch` remains
unchanged. All private app-producing builds ended and the **current root P06 gate
binary** was restored byte-for-byte to SHA-256
`bde7758c14bcb0f840427c235bd909195ea993a86ad052f100e7858297811b96`.
Native Windows/macOS gaps remain unchanged; no native execution claim is added.
Root supplies the scoped independent re-review and merged gate.
