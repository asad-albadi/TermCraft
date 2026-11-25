# Scoped service UI credential review

Verdict: **addressed; no new Important findings.** The proposed patch is suitable for the parent's merge and merged-source gate.

Reviewed the private `target/agent-work/service-ui-credentials/scoped.patch`, both candidate source files, `base-hashes.json`, the MAIN call sites, the pinned local egui 0.36.2 TextEdit implementation/state, and the recorded RED/GREEN/fmt evidence. Both MAIN base hashes and candidate hashes match the manifest. Regenerating the two-file diff reproduces `scoped.patch` exactly. No production source was changed during this review.

## Addressed

- The original masked FTP, Telnet, and SSH fields used egui TextEdit, whose input handler feeds plaintext strings into its undoer before and after handling input. The shared replacement clears loaded undo history before event handling and the resulting stored state after rendering. This covers prior retained snapshots and snapshots created by the current frame. `clear_undoer` replaces the shared undoer contents, so the state clone returned internally by TextEdit does not preserve an independent surviving undo history.
- The explicit ID uses the same `next_auto_id` plus one-ID advance as egui's original implicit-ID branch. Existing parent card/grid scopes remain in place, with no new field collision or downstream auto-ID shift. `ui.add(TextEdit)` delegates to the same show path used by the wrapper. Widths, password masking, response, focus, selection, and ordinary keyboard/pointer/IME handling are preserved; password undo history is intentionally unavailable.
- The only production changes replace the two service-card password widget sites and add the small shared wrapper. They introduce no logging, saved credential field, secret-copy capture, service configuration change, or protocol change. This addresses accessible UI undo retention; it does not claim allocator-level erasure of egui's temporary strings.
- All three behavioral RED cases exercise the actual fields, pointer focus, fixture typing, an idle snapshot, explicit form zeroization, and retrieval through the real TextEditState undoer. Each failed because the cleared fixture was recoverable. The corresponding GREEN cases additionally establish successful actual-field typing and empty retained undo history.
- Recorded targeted GREEN is 5 passed / 0 failed: the three credential cases, existing B01 actual FTP/TFTP/Telnet listener/restart ownership case, and actual Servers-page SSH pointer Start/authentication rejection and acceptance/SFTP selected-root read-only/Stop and listener-rebind/separate local PTY case. The test-only scrolling stabilization explains the Start/Stop pointer target adjustment and changes no production event handling. Recorded formatting check passed.

## Scope and limits

Existing B01 FTP/Telnet form retention is unchanged and explicitly documented; the tests clear those forms to isolate undo retention. SSH Start already clears its form and the integration test checks that behavior. Neither behavior is a newly introduced finding in this patch.

No repeated test run or broad audit was needed: the reviewed implementation and evidence leave no concrete unresolved uncertainty requiring one. The parent still needs the planned merged-source verification. This review does not claim a full suite, native Windows/macOS UI validation, or the parent release gate.
