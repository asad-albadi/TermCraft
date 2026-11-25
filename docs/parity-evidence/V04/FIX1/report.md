# FIX1 scoped correction — review P2 startup and outer timeout cleanup

The exact original runner reproduced two RED failures: a launcher was alive after its startup wait timed out before `owner` assignment; a detached descendant remained alive after the outer `subprocess.run` timed out. All RED probe processes were then killed/reaped. Original baseline HANDOFF SHA-256: `8cafca33c5809930136a6d097af36dd8fba459107cf882c654ea07866f5f4ca0`.

The candidate introduces a small Linux-only child-ownership helper shared by both runners. Before spawning, each sequential scope becomes a subreaper and captures existing children, which are excluded. Cleanup follows new child ancestry, including detached sessions and adopted descendants, uses generation-pinned pidfds for signals, sends TERM then KILL and reaps owned children within a three-second bound. Failure to finish cleanup remains failure. No unrelated process or arbitrary process group is signaled. Normal SIGTERM raises through scope cleanup. Captured pipes close even on error. Helper commands and zstd input/output use the same ownership wrapper.

Seven focused probes passed (`green-final.json`), without Cargo or repeated Mosh sessions:

1. Exact candidate `run_case`: launcher wait timeout before owner assignment; original `TimeoutExpired` preserved, launcher absent and reaped.
2. Exact candidate `run_case`: launcher exits with detached child before missing owner/handshake parse; original assertion preserved, both processes absent and reaped.
3. Exact candidate `run_case`: Popen startup error; original error preserved and no child created.
4. Outer timeout in an actual private user/network namespace with detached SIGTERM-ignoring descendant; timeout preserved, both processes absent and reaped.
5. Outer nonzero exit with detached descendant; exit 7 preserved, both processes absent and reaped.
6. Outer SIGTERM stack unwinding with a private namespace descendant tree; exit 143 preserved and descendants absent/reaped.
7. Successful owned helper with exact stdin/stdout bytes; return 0.

A pre-existing unrelated sleeper remained alive through all scopes, then the probe explicitly terminated/reaped it. The entire probe itself has an enclosing ownership scope for error cleanup. All probes finished, and all recorded process IDs were checked absent. The focused regression command returned 0.

`cleanup.patch` describes the two edited runner files and two added helper/probe files. `baseline/HANDOFF.json` retains the exact original 33 normalized hashes; original runners and README are retained next to it. Original observed successful-run files have unchanged hashes. The current handoff records original candidates as FIX1 bases, updated candidate hashes and explicit observation/candidate attribution.

This correction verifies the affected failure cleanup paths only. It does not assert that the full Mosh/serial matrix was rerun on the revised runner. The accepted successful 2 Mosh + 3 serial observations remain attributed to their original executable/source hashes. No MAIN product source was edited. Physical serial, physical network adapter and native/original cells remain unavailable; whole V04 remains incomplete and `strict_native_original=0`.
