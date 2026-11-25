# Extracted Linux runtime payload qualification

Overall parity remains **66.7% (40/60)**. These checks qualify one unsigned Arch-compatible Linux x86_64 release on this host; they do not establish full parity or native Windows/macOS support.

Release archive: `TermCraft-0.1.0-linux-x86_64-arch-xwayland-mosh-unix.tar.gz`.
SHA-256: `c6f12002f71d008aee83d165f30c93e62960cf2c74c75ba4b3007a173816af57`.
Frozen release source snapshot: `6251fcea2b9f7dbb5c2853016c78d6d262126c60fb4981ed5574231f1a0c20a8`.
Extracted package: `target/package-services-unix-qualified/c6f12002f71d008aee83d165f30c93e62960cf2c74c75ba4b3007a173816af57/TermCraft-0.1.0-linux-x86_64-arch-xwayland-mosh-unix`.

The prior 1007-file manifest verification, empty-PATH Wayland/X11 application startup, and sealed RDP helper checks remain recorded in `services-unix-release-verification.json`. They were not repeated here.

| Extracted payload check | Actual result |
|---|---|
| Xserver live lifecycle | 5 passed: packaged engine/window, display collision, owned stop, bad cookie/TCP disabled, failed startup cleanup. |
| Mosh | Genuine SSH bootstrap and Mosh UDP terminal frame received with empty client PATH; missing/tampered client/wrapper rejected. |
| Portable Unix 1.31.0 | Extracted archive installed through pinned PackageManager; actual BusyBox ash PTY and pipeline/file output passed. |
| Portable Unix 1.35.0 | Same installation, private child environment, actual PTY and pipeline/file checks passed. |
| Iperf 2 TCP | Actual extracted client reached ServiceSupervisor bundled server: 256 KiB, 2.04 Mbit/s. |
| Iperf 2 UDP | Actual traffic and server report: 132 KiB, 93 sent datagrams, 0/92 lost in server report. |
| Iperf 3 TCP | Actual sender and receiver reports: 128 KiB, 1.05 Mbit/s. |
| Iperf 3 UDP | Actual sender and receiver reports: 128 KiB, four datagrams, 0% loss. |

Xserver ran only through `tests/support/run_xserver_isolated.sh`: bwrap unshared network/PID namespaces, private `/tmp/.X11-unix`, and headless Cage. The fixture verifies distinct host versus private namespace/socket identities. Its display `:0` collision fixture is private; no host display was started or stopped. Test executable: `target/debug/deps/xserver_lifecycle-89bf74f0ef0f0612 --ignored --nocapture --test-threads=1`. The final compositor broken-pipe message is fixture teardown after five successful checks.

Mosh used `target/debug/deps/runtime_manifest-da99e8eacc1f7e49 --exact staged_runtime_runs_with_empty_path --ignored --nocapture`, with `TERMCRAFT_STAGED_RUNTIME` pointing at the extracted root and `TERMCRAFT_MOSH_SERVER` pointing at the pinned `target/runtime-inputs/usr/bin/mosh-server`. The existing test copies the verified extracted Mosh wrapper/client and RDP helper to a private temporary sibling package before placing its test executable there. This permits current-executable discovery and destructive missing/tampered-byte checks without modifying the release extraction. The received `runtime-mosh-udp` terminal frame is the success condition. SSH uses an owned loopback fixture and disposable keys; remote server PATH includes its explicit fixture and `/usr/bin` while client PATH is empty. This qualifies extracted runtime bytes through current root APIs; it is not a GUI Mosh connect test.

Unix and Iperf used a standalone explicit-root API probe, source saved as `runtime-payload-payload_probe.rs`, compiled against the normal root library with `CARGO_BUILD_JOBS=2 cargo build --lib --offline --locked --message-format=json`, followed by `rustc` with the produced root rlib and native link directories. The probe was fixed before the parent resumed production merges. Build record and binary hashes appear in the verification JSON; full temporary build output and executable remain under `target/qualification-evidence`. No product source or root test source was changed. Existing Iperf tests hardcode the source root, so those tests are not claimed as extracted-release evidence.

The probe ran with an empty parent PATH. Each Unix archive was read from the explicit extracted root's `unix-packages` directory, authenticated against PackageIndex's archive and executable hashes, installed by PackageManager into a private directory, and launched by UnixEnvironment. `test -t 0` and `test -t 1` proved a real PTY; a command-generated `real-pty` marker and `portablebeta` pipeline result were observed. HOME and PATH were asserted to equal the private home and installed package bin directory. The parent HOME/PATH stayed unchanged, and both installed releases were removed after their terminals exited. No Windows BusyBox executable was run.

Iperf2 and Iperf3 providers resolved as Bundled beneath the explicit extraction. ServiceSupervisor's packaged launch verifies and seals the selected executable; its provider log was asserted. Clients were the extracted executables. Each version transferred real traffic to its owned loopback TCP and UDP server, returned success and throughput, then stopped through ServiceSupervisor with socket reuse verified. These are one-second connectivity checks, not performance benchmarks.

Logs: `runtime-payload-xserver-live.log`, `runtime-payload-mosh-live.log`, and `runtime-payload-unix-iperf-live.log`. Commands/build provenance: `runtime-payload-probe-link-command.json`, `runtime-payload-probe-build.log`, `runtime-payload-lib-build.stderr`, and `runtime-payload-build-provenance.json`.

The archive was rehashed before copying into `dist/snapshots/c6f12002f71d008aee83d165f30c93e62960cf2c74c75ba4b3007a173816af57/`, and the copied archive was rehashed to the same digest. Its verification JSON preserves this bounded qualification.

Remaining gaps include native Windows, native macOS, clean Linux dependency closure across distributions, exact musl source closure, installed Chromium/browser closure, and signing. System Perl/OpenSSH and declared shared libraries remain required for this Linux qualification.
