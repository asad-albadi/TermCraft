# X04 scoped correction review

Date: 2026-11-25. Scope: the Important payload-loading finding in `review.md` and new breakage introduced by its fix. This is not a new full X04 review or a parity qualification.

Verdict: **ADDRESSED. Approve integration of the narrow partial candidate.** No new Important or Critical finding in this correction. X04 remains Partial; keep 40/60 implementation checkpoints and 0/60 strict native/original parity unchanged.

## Correction assessed

`target/agent-work/x04/src/x11/xdmcp_payload.rs` opens with `O_NONBLOCK | O_NOFOLLOW | O_CLOEXEC` and validates regular-file type and declared size using metadata from the opened file descriptor. FIFO/device/directory inputs are rejected before reading; final-component symlinks are rejected by open. This avoids the previous path-check/open race and FIFO wait. Parent-directory symlink resolution cannot bypass the opened-file type check or exact content pin.

The loader copies at most 16 MiB in 64 KiB chunks to a private memfd and hashes those same bytes. Cancellation and the shared deadline are checked around reading, hashing, writes, digest finalization and sealing. Growth beyond the initial size is also bounded. Exact pinned SHA-256 verification and write/grow/shrink/seal protections remain intact. Failure drops the descriptor rather than executing a partial image.

`src/x11/xdmcp_engine.rs:168` now establishes one eight-second startup deadline before loading. The engine passes that deadline to the loader, checks it immediately before spawn and reuses it during socket/readiness verification. There is no fresh eight-second budget after loading. Existing private authority handling, child ownership, selected-display/Unix peer and TCP/TCP6 listener verification remain in the launcher. The single-file loader introduces no host-display interaction.

The documentation correctly limits this to cooperative work and elimination of nonregular-file blocking. `O_NONBLOCK` cannot preempt arbitrary filesystem/kernel stalls on regular-file operations; the correction makes no absolute syscall-preemption claim.

## Fresh reviewer verification

- Independently recalculated all six unchanged MAIN base hashes and thirteen addition hashes: all match the updated `evidence/handoff.json`.
- Repeated `patch --dry-run --batch -p1` against MAIN: successful for all six shared files, including the Linux-only payload module declaration.
- Ran only the existing private library binary's `x11::xdmcp_payload::tests::` filter, serially: **5 passed, 0 failed, 0 ignored**, 0.05 seconds. This executes actual FIFO/directory/final-component symlink/oversize refusal, cancellation/expired budget before opening, cancellation after a real-file read before copying, deadline crossing during a real-file read, and multi-chunk digest/copy plus wrong-digest refusal. No compilation, host X access, host networking or broad suite was run.
- Inspected owner `evidence/final-review-fix-lib.log`: 16 passes, zero failures and one explicitly ignored live fixture. Inspected `evidence/final-review-fix-real-xdm.log`: the actual isolated pinned Xephyr/xdm monitor/green-pixel, wrong-cookie, two-session ownership/cleanup case passes once. These are owner live-execution results, not a reviewer live rerun. Empty `evidence/review-fix-fmt.log` is the owner's successful formatting evidence.

All reviewer commands finished; no reviewer jobs remain. Existing missing native XDMCP adapters/runtime, clipboard/custom keyboard/hostname/IPv6/chooser/X03/source-engine pre-bind race/clean-machine ABI/original-runtime gaps from the first review remain unchanged. This correction resolves the integration blocker without completing those missing outcomes.
