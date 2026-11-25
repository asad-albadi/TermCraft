# X01 independent review

Both original findings below are addressed.

1. **Original finding. Execute the exact bytes that passed verification.** At `src/x11/platform.rs:285` the checksum covers a memory buffer, while line 92 executes the original filesystem inode. An open read descriptor prevents pathname substitution from redirecting execution, but still permits an in-place change between hashing and exec. This matters for the selectable bundle folder, including shared writable installations. Copy the verified bytes into a sealed executable memfd and launch that descriptor. A deterministic regression can modify the original test file after verification and check that the executable descriptor still contains the pinned bytes, without executing any replacement.
2. **Original finding. Bound the readiness connection and honor cancellation.** At `src/x11/platform.rs:414` the Unix socket connect runs in blocking mode. A full listener queue can keep it blocked indefinitely when the child stops accepting connections. The socket deadlines on lines 438–439 only apply after connect returns, and the startup deadline ended when the display number was read. The shutdown join in `src/ui/xserver.rs` then waits indefinitely for this worker. Use a nonblocking connect with a deadline and cancellation checks through readiness, and test against a private temporary listener with a full queue.

The optional Arch release stage uses the pinned archive, selects explicit members and records the provider in the release inventory. The native Windows and macOS candidates and XDMCP remain unqualified. This was a source review with existing evidence. No live X server or host display operation was performed.

## Fix verification

- **Executable integrity — addressed.** The checksum and sealed memfd both use the same byte buffer. The command executes the sealed descriptor. The regression changes the original file and checks both the verified contents and rejection of a write to the sealed descriptor.
- **Readiness and cancellation — addressed.** Connect, write and read use nonblocking sockets and consult the same startup deadline and cancellation flag. The regression fills a private Unix socket queue, cancels startup, and verifies cleanup of the owned child, socket, lock and credential directory.

No additional important findings in the two fixes. The re-review inspected source, regression coverage and recorded test results. It performed no host display operations.
