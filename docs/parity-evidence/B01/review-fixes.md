# B01 scoped fix review

**Approved for the reviewed fixes. All four Important findings are addressed. No new Important issue found in the round-two FTP correction.** This is a scoped fix review, not approval of the separate integrated workspace or native runtime gates. Source paths refer to `target/agent-work/b01/`.

## Addressed findings

- **TFTP ERROR reflection — ADDRESSED.** The listener discards incoming ERROR before parsing, the unknown-transfer-ID path suppresses ERROR replies, and the expected-peer exchange terminates on ERROR without generating another ERROR. The saved protocol fixtures check silence from the listener and foreign-TID worker receive path.
- **Telnet final PTY output truncation — ADDRESSED.** The bridge continues receiving after child exit until the PTY reader channel disconnects, retaining deadline and cancellation checks. The saved fixture verifies all 512 KiB and the final marker after immediate shell exit.
- **FTP active/passive ABOR cancellation — ADDRESSED.** Data acceptance and I/O poll the control connection, recognize fragmented ABOR, discard staged uploads, return 426/226 and preserve the original control session. Round-two fixtures place NOOP both before and after fragmented ABOR during passive acceptance, passive upload and active upload, checking both retained command replies and staging cleanup.
- **Queued ordinary FTP command discards completed upload — ADDRESSED in round two.** The first fix rejected every nonempty pending control buffer before publishing a completed STOR. A live review probe and the subsequent RED fixture reproduced 426/200 with the upload missing. The corrected `src/services/ftp.rs:382` scans complete buffered command frames and removes only ABOR, retaining other commands in order. Publication at line 360 now uses the normal transfer cancellation check after data EOF instead of rejecting ordinary queued bytes. The regression fixture verifies 226, then the queued NOOP's 200, and exact stored payload contents.

## Evidence and scope

The [round-two RED output](review-round2-red.log) reproduces the queued-NOOP failure. The updated [protocol output](protocol-tests.log) records **17 passed, 0 failed**, including completed upload preservation, fragmented cancellation, terminal final output and TFTP ERROR handling. Reviewed the current private FTP implementation and changed integration fixtures against those results. No additional suite rerun was needed for this scoped rereview.

Round one included a narrow live probe of the new FTP regression. Round two was static review plus inspection of supplied RED/GREEN evidence. No production source, Cargo identity or git state was changed. Earlier GUI and component checks remain separate evidence; native runtime and whole-workspace gates remain outside this scoped approval. The previously documented FTPS scope question is unchanged.
