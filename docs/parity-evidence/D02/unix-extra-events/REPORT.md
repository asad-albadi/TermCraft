# D02 Unix extra events FIX1

**Supersedes the predecessor candidate's CTS/DSR/RLSD/RING acceptance and its associated feature claims.** Independent review found that TIOCGICOUNT snapshot success does not establish active modem-edge recording. The correction narrows the Linux counter family to **input BREAK (0x40) and line-status ERR (0x80)** only. RXCHAR (0x01) remains. Modem masks are always explicitly UNSUPPORTED, including when native counter snapshots succeed and regardless of flow/carrier settings. This is a private candidate for scoped rereview; MAIN/tracking remain unchanged and D02 remains partial.

## Correction and rationale

Linux 8250 serial8250_set_ier may clear modem-status interrupts when CLOCAL is set and CRTSCTS is clear. serial8250_modem_status updates modem counters only when MSI is enabled; uart_get_icount merely copies snapshots. The previous success gate could therefore promise a wait that never observes physical changes. Its deterministic overridden-counter fixtures did not model native capture activation. That was a production capability error, not a physical reproduction claim.

FIX1 does not attempt to arm MSI, change flow/carrier policy, launch blocking TIOCMIWAIT threads, or use signals. COUNTER_EVENTS is 0xc0 on Linux and zero elsewhere. Extraction now reads only native frame6/overrun7/parity8/brk9, into a four-field snapshot. Modem fields0–3 never participate in event generation. SET rejects modem-only or mixed modem/supported masks before native snapshot/commit; its current mask/origin/wait remain intact. The reset-admission predicate likewise rejects modem masks and accepts only the defined 0xc1 family on Linux. This also stays correct if a later line/handflow configuration disables MSI: the unsupported masks are never accepted.

The accepted receive error/break family uses counters updated by the ordinary receive path. In the reviewed 8250 source, serial8250_read_char records break/parity/framing/overrun before applying read_status_mask or ignore_status_mask. This is distinct from the separately armed modem interrupt path. It is source-supported behavior, not a claim that every driver implements every physical signal/error, or that hardware was tested. Drivers lacking TIOCGICOUNT remain unsupported. Logical CREATE support, fresh SET origins, native query failure, one pending wait, reset/close, opened-descriptor identity, RXCHAR gating and bounded worker behavior remain as in the scoped predecessor.

Modulo-u32 counter comparison cannot distinguish a native counter reset from a change, nor a full counter cycle from no change. Descriptor pinning does not prove driver reset immunity. This limitation is explicit; no reset-detection evidence or universal edge guarantee is claimed.

Primary sources read for FIX1:

- [8250 receive/error, MSI and configuration implementation](https://github.com/torvalds/linux/blob/master/drivers/tty/serial/8250/8250_port.c): serial8250_read_char, serial8250_modem_status, serial8250_set_ier.
- [Serial core](https://github.com/torvalds/linux/blob/master/drivers/tty/serial/serial_core.c): uart_get_icount versus uart_wait_modem_status/uart_enable_ms.
- [Serial core header](https://github.com/torvalds/linux/blob/master/include/linux/serial_core.h): UART_ENABLE_MS.
- [Microsoft WaitCommEvent](https://learn.microsoft.com/en-us/windows/win32/api/winbase/nf-winbase-waitcommevent): input BREAK and ERR (framing, hardware overrun and receive parity) meanings. Software queue overrun, RX/TX, modem and reserved fields remain excluded from ERR.

## Verification and limits

All cargo commands used CARGO_BUILD_JOBS=2 and absolute CARGO_TARGET_DIR `/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/target/agent-work/d02-unix-extra-events/build`. FIX1 private package AND binary are `termcraft-rdp-unix-events983`, version0.1.983. Its private PCSC CARGO_BIN_EXE fixture reference matches. Root/normal product manifest and fixtures are not part of the payload. No broad app build or native/physical endpoint was used.

Run from app/ with the above environment:

1. `cargo test --offline --manifest-path target/agent-work/d02-unix-extra-events/FIX1/source/engines/rdp/Cargo.toml unix_modem_snapshot -- --test-threads=1`: **RED exit101**,0pass/1fail (`red.log`). The predecessor implementation accepted a modem mask after a successful simulated native snapshot; the new policy test expected UNSUPPORTED. This is a deterministic native-boundary policy reproduction, not physical MSI failure reproduction.
2. Same manifest with `cargo test --offline ... unix_ -- --test-threads=2`: initial narrowed-family GREEN13passed/0failed (`green-targeted.log`). These selected tests include existing real channel saturation/reset/close behavior; targeted-only selection filters other tests and makes no whole-suite claim.
3. `cargo fmt --manifest-path ...`, then `cargo fmt --check --manifest-path ...`: final check exit0 (`fmt.log`, `fmt-check.log`).
4. `cargo test --offline --manifest-path ... -- --test-threads=2`: final **82 unit +10 PCSC integration tests passed;0failed,0ignored,0filtered**, exit0 (`green-full.log`). This final run includes the expanded new policy test and narrower existing event fixtures.

The new policy test uses a successful overridden TIOCGICOUNT snapshot on a real test-owned PTY. It checks all modem bits at four CLOCAL/CRTSCTS combinations, and verifies rejected requests leave native c_cflag unchanged. This tests the absence of unsupported promises and forced arming; it does not show physical transitions. Existing event cases are now scoped to real supported BREAK/ERR field mapping: fresh origin, independent and combined bits, wrap, between-wait retention, real protocol reads without fabricated RXCHAR, native query failure and close. Modem snapshot changes and modem/supported mixed-mask rejection cannot satisfy or reset an accepted wait. Unoverridden real PTY refusal remains actual kernel evidence. Successful BREAK/ERR observations remain controlled native-boundary fixtures over real production request/completion code, not physical line-error evidence.

macOS/other Unix retain RXCHAR-only behavior; Windows source remains untouched. Physical serial flow/errors/modem transitions, native RDP interoperability and original comparison remain unexecuted. Root integrated source/release gate and independent FIX1 rereview remain pending.

## Remaining architectural unsupported controls

GET_COMMSTATUS and GET_PROPERTIES remain unchanged explicit UNSUPPORTED; they are not completed by FIX1 or by the predecessor. Generic POSIX cumulative counters/termios/queue-length queries do not provide SERIAL_STATUS's complete exact hold/EOF/immediate state and native accumulated-error acknowledgment. Faking zero unknown flags or only advancing a private error baseline would not implement that contract. SERIAL_COMMPROP requires actual settable capabilities and queue maxima; zero queue maxima means unlimited, not unknown, so compile-time baud tables/FIFO sizes/zero placeholders cannot truthfully supply it. See the preserved predecessor report's primary [SERIAL_STATUS](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ns-ntddser-_serial_status) and [SERIAL_COMMPROP](https://learn.microsoft.com/en-us/windows-hardware/drivers/ddi/ntddser/ns-ntddser-_serial_commprop) rulings. These are architectural/native-provider gaps, not hardware tests presumed to complete them. TXEMPTY and other unimplemented event families remain unsupported. No unsafe POSIX file mutation work was added.

## Packet and preservation

`payload/engines/rdp/src/serial.rs` is the sole product file and exactly equals the final compiled private source. `cumulative.patch` applies to the MAIN baseline recorded in `baseline-hashes.json`; `delta.patch` applies only to the preserved predecessor payload. Merge the cumulative payload/patch once, not both. Private package/lock/fixture identity changes stay outside both patches.

`predecessor-hashes.json` pins predecessor seals. `predecessor-preservation.json` verifies every old HANDOFF file remains unchanged. Previous source, payload, reports, logs and seals were not rewritten. `main-preservation.json` rechecks every original MAIN baseline including serial.rs/devices.rs/devices_tests.rs. `candidate-hashes.json` records final sources, private build identity and exact emitted helper/test executable hashes. Its executable paths are relative to FIX1 and begin ../build. `HANDOFF.json` seals this delta/cumulative packet. No checklist percentages or queue wording changed.
