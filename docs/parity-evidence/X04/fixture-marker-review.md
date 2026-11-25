# Independent fixture marker publication review

Decision: **ACCEPT** the private fixture publication repair for the actual integration rerun. No blocking findings within the assigned scope.

Reviewed candidate: `target/qualification-games-v03/xdmcp-fixture-atomic/xdmcp_window.py`, SHA-256 `7a9369ece740240662121f40750f665be337266d088def54eb5001717c62a283`.

The actual diff matches `fixture-marker-patch.diff` exactly. Both fixture trees have the same five-file set; the other four files are byte-identical. All five original files match their `inputs/xdmcp-fixture/` entries in the immutable V03 qualified source manifest. The recorded evidence hashes, consumer source hash, and frozen lib-test hash `5c1a93e0bd59830ef5625f917e4ce23543c781bb26933107c36362c5c7ce9d0b` also match their current files.

The fix addresses the observed consumer contract: `xdmcp_engine.rs` polls marker existence and reads once, while the original producer creates/truncates the marker before writing its contents. The recorded actual integration log has an empty observed value at the second-session geometry assertion. The new helper exclusively creates a random owned temporary in the marker directory, writes and closes it before same-filesystem `os.replace`, and removes only that owned temporary in `finally`. This prevents the readiness path from exposing partial contents. No durability guarantee is needed for this live private test handoff. The payload expression, authentication, pixel validation, geometry queries, and client lifetime are unchanged.

I inspected the probe: it compiles the selected fixture’s real imports, helper definition, and actual publication expression from its AST. It substitutes the invocation/X11 query values, rather than implementing a separate publisher. Its write wrapper preserves real file creation, write, close, replace, and cleanup operations; events hold the producer after creation and before payload writing.

Freshly reran all four recorded commands without rewriting their evidence:

- Original characterization: exit 0; target exists with `b''` at the write barrier.
- Original with atomicity expectation: expected exit 1, specifically `publication exposed incomplete marker: b''`.
- Candidate with atomicity expectation: exit 0; target absent at the barrier, then exact full payload and no temporary remains.
- Candidate injected write failure: exit 0; expected `OSError`, no target or temporary remains.

All four subprocesses completed and were reaped; the probes joined their producer threads and removed their private temporary directories. This review started no Cargo jobs, native X11 servers, display managers, or fixture clients. Only this review document was written; source and original evidence were not changed.

Acceptance is limited to this fixture repair and focused publication evidence. It does not assert an actual XDMCP integration pass. The assigned root-owned actual integration rerun remains necessary before final qualification.
