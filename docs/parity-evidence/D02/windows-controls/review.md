# Independent Windows serial controls review

Verdict: **ACCEPTED**. No actionable correctness or scope findings in the submitted 11-file increment.

Reviewed package: `../d02-windows-control-completion/`, including its report, patch, payload, baseline manifest, ownership manifest, private identity, actual production serial implementation, pure planner, generated component boundaries, and completion helpers. Review writes stayed under this directory; no MAIN or submitted candidate/payload sources were modified. No review subagents were created.

## Integrity and scope

- All 11 submitted payload hashes match the ownership manifest; all candidate hashes match their declared hashes. Payload contains precisely those 11 paths.
- Every existing owned MAIN/base path matches its declared original baseline; both new production/test paths are absent from MAIN as expected.
- Applying integration.patch to a private base copy reconstructs every payload byte exactly, including the helper Cargo.toml with only the intended Windows feature addition and original integration identity.
- Outside owned paths, the original candidate differs from its source baseline only in Cargo.toml, Cargo.lock and engines/rdp/Cargo.lock for declared private identities.
- Wider MAIN baseline audit checked 710 files: 707 match; src/ui/session_confirmations.rs, src/ui/games.rs and src/ui/games/play.rs differ outside this increment. These are not attributed to this worker. See full-baseline-check.json. This does not weaken the exact owned-source baseline match.

## Spec and implementation review

The new allowlist uses the MS-RDPESP published IOCTL values. It enforces exact request body and input lengths, the 64 KiB bound, minimum output sizes, fixed native allocations for fixed getters, bounded caller capacity for properties extensions, valid nonzero purge flags and defined wait-mask bits (including zero). Unsupported IOCTLs remain denied. Native ABI size assertions cover SERIAL_STATUS=20, SERIAL_COMMPROP=64 and SERIALPERF_STATS=24.

Authority is preserved for existing controls. SET_WAIT_MASK and PURGE require write authority. GET_COMMSTATUS also requires write authority because returning SERIAL_STATUS consumes accumulated driver error history. GET_WAIT_MASK, GET_PROPERTIES and GET_STATS remain queries; WAIT_ON_MASK additionally requires read authority. Stale remote handles still fail before dispatch, and read-only mappings cannot obtain mutation authority.

There is at most one outstanding wait per device/remote handle, with the existing single handle per port. SET_WAIT_MASK is allowed while a wait is pending; no artificial wait result is generated. A driver-produced successful zero event mask is returned as four zero output bytes. Unsupported native event semantics stay with the driver.

The original wait mask is captured lazily before the first SET_WAIT_MASK and restored on Handle drop. Remote close retires all pending native operations before dropping that handle; remote reopen reuses the pinned endpoint and captures state afresh. Serial drop similarly clears pending operations before port fields drop. Existing DCB/timeouts restoration remains intact. No namespace reopen, worker, queue expansion, Linux functional change or arbitrary IOCTL forwarding was added. The existing worker admission limit remains 16 ordinary requests plus a reserved close slot.

PURGE normalizes successful native Information counts 0 or 4 to an empty wire output only after GetOverlappedResult reaches terminal completion. Unexpected counts fail. It does not retire other native operations. Their Vec buffers, boxed OVERLAPPED, event and Arc<File> remain owned until their own terminal result. Both ordinary and cancellation completion paths apply the adapter; native failures and cancellation races preserve actual successful read/write/wait data and counts through the unchanged production completion helpers.

## Independent verification

Private review source identity uses version 0.1.928 and normal application/helper binary names termcraft-winserial-review928 / termcraft-rdp-winserial-review928; no application or helper binary was built. Focused component package names are uniquely suffixed review928. All Cargo invocations used --offline --locked -j 2 and private target directories.

1. Regenerated the pure component from actual private production inputs, then independently ran its tests: **15 passed, 0 failed**. See pure-test.log.
2. Regenerated the native component from actual private production inputs, then independently ran cargo check --tests --target x86_64-pc-windows-msvc: **passed**. See windows-check.log. The unchanged generated-drive unused metadata warning is retained.
3. Compared regenerated pure completion helpers with the payload extraction boundary by source inspection; the generator extracts the unchanged production functions and imports the actual serial_completion.rs and serial_windows_control.rs.

Both jobs completed with exit code 0 and were reaped. The known Linux native-component generator fixture/dependency limitation was not misclassified as a product regression. No parent full gate, unrelated Linux PURGE, printer or smartcard review was run.

No Windows runtime, COM hardware, RDP interoperability, full native helper link or application runtime claim is made. Native restoration ordering and pending-buffer ownership have source/typecheck evidence, not Windows execution evidence.

## Primary sources independently consulted

- [MS-RDPESP control request and values](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpesp/824cc217-febd-4a2e-a95f-5b5bdc32848f)
- [PURGE native buffer/count and lifetime contract](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ni-ntddser-ioctl_serial_purge)
- [SET_WAIT_MASK flags and pending wait completion](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ni-ntddser-ioctl_serial_set_wait_mask)
- [WAIT_ON_MASK completion behavior](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ni-ntddser-ioctl_serial_wait_on_mask)
- [GET_COMMSTATUS native buffer](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ni-ntddser-ioctl_serial_get_commstatus)
- [SERIAL_STATUS error-history consumption](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ns-ntddser-_serial_status)
- [SERIAL_COMMPROP trailing provider data](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ns-ntddser-_serial_commprop)
- [GET_STATS native structure and query behavior](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ni-ntddser-ioctl_serial_get_stats)
