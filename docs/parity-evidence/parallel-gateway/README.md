# D04 SSH desktop and Telnet gateway

Linux source-checkout evidence, 2026-11-24. The fixtures use owned ephemeral ports, keys, processes and files.

- `transport-tests.log` records the real SSH byte-carrier, two-hop host-key and authentication, rotated-key rejection, target-channel refusal, cancellation isolation, and VNC framebuffer and pointer tests. It also records the loopback helper relay tests and two RDP TCP-channel attempts under a single authenticated SSH session.
- `cli-lifecycle-tests.log` records the CLI exit regression with a loopback gateway port reserved by a bound socket that accepts no connections. The failed job retires before Telnet starts and the existing terminal remains active.
- `workspace-tests.log` records the embedded Telnet client displaying target output through a gateway tab and the embedded RDP engine completing unknown-certificate approval and NLA through a gateway tab. The tests retain the protocol endpoint identity and observe that closing the tab releases its gateway channels.
- The SSH fixture maps the private protocol name only when it receives the intended direct-tcpip request. The VNC fixture is a deterministic RFB endpoint and the RDP fixture runs separately from the product engine. The Telnet test checks NEW-ENVIRON negotiation for the actual target username and the output supplied by the target.

Commands:

```sh
CARGO_BUILD_JOBS=2 cargo test --offline --test desktop_gateway --test desktop_carrier -- --nocapture
CARGO_BUILD_JOBS=2 cargo test --offline --bin harbor-workspace ui::desktop_gateway::tests -- --nocapture
CARGO_BUILD_JOBS=2 cargo test --offline --bin harbor-workspace ui::cli_dispatch::tests -- --nocapture
```

The SSH worker pairs loopback sockets off the UI thread and drops its temporary listener before publishing the connected stream. VNC and embedded Telnet consume that stream directly. The separate RDP relay listens only on loopback and verifies a randomly generated per-relay token before moving any protocol data. Incomplete and invalid tokens receive no target data and use no retry attempt. The relay permits up to three authenticated target attempts while the helper waits for a certificate decision. All three adapters retain the actual target host and port, and gateway credentials remain bound to the original saved protocol profile and verified SSH hop.

Native Windows and macOS, deployed SSH proxy/agent variants, external RDP and VNC servers behind real gateways, packaging, and original-product comparisons were outside this fixture run.

## SSH password prompt disconnect fixture

The combined integration run recorded Linux `ENOTCONN` on the fixture's second main-thread shutdown in [full-desktop-gate.log](../parallel-integration/full-desktop-gate.log). The first shutdown woke the relay's copy thread, which could shut down its duplicate upstream socket before the main thread reached that same connection. A two-leg loopback reproduction forced that ordering and recorded errno 107 in `ssh-lifecycle-cause.log`. The original focused test passed in isolation, as recorded in `ssh-lifecycle-before.log`.

The corrected fixture confirms that the password prompt is still active, shuts down the upstream socket once, and lets the relay deliver the disconnect to the client. The assertions require prompt retirement, rejection of a late answer, and zero successful authentications. The full targeted SSH lifecycle test binary passed all 15 cases in `ssh-lifecycle-after.log` with `CARGO_BUILD_JOBS=2 cargo test --offline --test ssh_lifecycle -- --nocapture`.
