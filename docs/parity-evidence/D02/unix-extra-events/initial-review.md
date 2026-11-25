# Independent review: D02 Unix extra events

SPEC verdict: **FAIL — one P1 correctness issue in the proposed modem-event capability gate.**

QUALITY verdict: **CHANGES REQUIRED**. The packet is internally consistent and the counter mapping/lifecycle work is otherwise disciplined, but accepting a native counter query as proof of active modem-edge capture is insufficient. Do not merge this packet unchanged.

The accepted narrower scope was reviewed: GET_COMMSTATUS and GET_PROPERTIES remain honestly UNSUPPORTED; TXEMPTY remains deferred. Their absence is not a defect against this candidate's approved scope. D02 overall remains partial.

## P1: Arm or establish native modem-edge capture before accepting modem masks

Location: `payload/engines/rdp/src/serial.rs:760–774` (the CREATE `stats_baseline.is_some()` gate and fresh `event_counters` query); related read-only snapshot helper at lines 113–145 and polling at lines 78–94.

A successful TIOCGICOUNT only proves that cumulative counters can be read. It does not establish that the driver is recording modem transitions. On Linux's common 8250 path, CLOCAL with CRTSCTS clear disables modem-status interrupts; the counter update path requires those interrupts to be enabled. The serial core's default settings include CLOCAL, and this implementation preserves that flag. Consequently CREATE and SET succeed for CTS/DSR/RLSD/RING while actual modem transitions can leave the counters unchanged and WAIT pending indefinitely. Polling the snapshot does not activate observation. This also matters after subsequent termios/handflow changes disable recording.

Primary upstream evidence inspected on 2026-11-25:

- [8250 driver](https://github.com/torvalds/linux/blob/master/drivers/tty/serial/8250/8250_port.c): `serial8250_set_ier` clears MSI and conditionally enables it through UART_ENABLE_MS; `serial8250_modem_status` requires MSI before updating modem counters.
- [serial core header](https://github.com/torvalds/linux/blob/master/include/linux/serial_core.h): UART_ENABLE_MS depends on hardware PPS, CRTSCTS, or CLOCAL being clear.
- [serial core](https://github.com/torvalds/linux/blob/master/drivers/tty/serial/serial_core.c): `uart_get_icount` copies counters only; `uart_wait_modem_status` explicitly calls `uart_enable_ms`. Default UART termios sets CLOCAL.

This finding is a source-based inference about an established driver path, not a claim of physical hardware reproduction. The fixtures replace TIOCGICOUNT results directly and therefore cannot detect whether real transitions reach those counters. Establish and maintain native capture using a bounded, cancellable mechanism with correct close/restoration semantics, or keep these modem masks UNSUPPORTED unless an explicit provider proves that capture is active. Do not enable CRTSCTS or clear CLOCAL merely to force interrupts: those choices alter flow/carrier behavior. Add focused coverage for capture activation and later configuration changes; relabeling the fixture alone cannot fix the runtime behavior.

The protocol requires actual events to complete waits: [WaitCommEvent](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-waitcommevent) defines the selected modem events, and [WAIT_ON_MASK](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ni-ntddser-ioctl_serial_wait_on_mask) requires events since SET or the last successful wait to be retained.

## Reviewed correct aspects

- Linux UAPI field positions match the installed `/usr/include/linux/serial.h`: CTS0, DSR1, ring2, DCD3, framing6, hardware overrun7, parity8, break9. ERR excludes BREAK, RX/TX, software buffer overrun and reserved slots. The six-field statistics selection remains unchanged. Signed native int bit patterns are preserved through u32 conversion.
- Counter deltas detect a u32 wrap; equal values after a complete counter cycle are necessarily indistinguishable. A native counter reset is also indistinguishable from a modulo delta and would be reported as a change; the packet has no reset-detection evidence. Native descriptor pinning avoids a logical reopen resetting the observed device, but is not a universal guarantee against native driver resets.
- SET validates input length/output bound/access before mutation, rejects undefined versus unsupported bits distinctly, queries every required origin first, then commits mask/origin/latches. Failure preserves the old pending wait and state. Successful SET retires the previous wait with zero; CLOSE cancels its pending wait once. The existing request/handle matching still rejects stale controls and cancels queued transfers tied to a closed handle.
- Event observation returns actual query failures through the existing tty_status mapping (ENOTTY => UNSUPPORTED; EIO/ENODEV/ENXIO => DISCONNECTED), retires the wait, and does not synthesize success from a failed query.
- RXCHAR observation/consumption/purge capture is gated on RXCHAR itself, preventing real reads from fabricating RXCHAR for counter-only masks. Coalesced counter bits are masked and cleared once delivered. Counters retain changes between waits because the next observation compares against the previous successful snapshot.
- Admission recognizes exactly RXCHAR plus the Linux counter family, with exact body shape and MAX_IO output cap. Worker capability/access/handle checks remain authoritative. The extra admission slot is bounded; unsupported PTY or stale/readonly requests cannot mutate masks. The existing CLOSE reserve remains available.
- Non-Linux counter masks fail the support test; PTY counter masks fail unless the test-only override was already active at CREATE. There is no changed Windows source or fabricated generic POSIX properties/status support.

## Evidence and artifact linkage

Independently verified every HANDOFF.json SHA256, every candidate-hashes.json SHA256 (including executable artifacts), and exact equality between the product payload and the compiled private source serial module. Reverse-applying scoped.patch to the payload reconstructed the serial.rs baseline hash exactly. Patch scope is one product file; private package/binary/PCSC fixture identity edits are outside the payload. The main-preservation file is intact as supplied; this review made no MAIN/product edits.

Inspected `green-helper-full.log`: private package/binary identity matches `termcraft-rdp-unix-events982`, and the reported 81 unit plus 10 PCSC integration tests passed with zero failures. Inspected the five new tests and existing wire/access/stale/close/hangup/saturated-channel tests in their actual compiled source. No broad helper rerun was performed; it would not exercise the missing driver activation path.

Evidence labels are accurate: successful modem/error events use controlled, thread-local native-counter fixtures over real PTY descriptors and actual production request/completion code. The unoverridden PTY rejection and PTY input/read flow are actual kernel observations. The pure new admission test is not mislabeled as hardware success or a new saturated worker test. No physical UART/USB or Windows/macOS event behavior is established.

No additional blocking packet, memory-safety, wire-bound, stale-reply, reset-admission or lifecycle issue was identified by inspection. Full D02 completion and the parent's combined source/release gate are outside this review's success claim.
