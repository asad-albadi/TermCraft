# P02 FTP folder review

## Important — A single-file transfer can become recursive

The FTP browser distinguishes “Upload…” from “Upload folder…” and chooses the matching picker at src/ui/ftp.rs:350–354. The confirmation retains that choice, then drops it when constructing the operation at src/ui/ftp.rs:691–705. At src/protocols/ftp_actor/operations.rs:91–96, the worker treats any directory currently at the selected path as permission to recurse.

To observe the issue, choose a regular file for upload and leave the confirmation open. Rename that file, create a directory containing two ordinary files at its previous path, then confirm upload to a fresh remote name. The worker stages and sends both files. The equivalent download path at src/protocols/ftp_actor/operations.rs:148–154 switches to a folder download if the remote file becomes a directory before the worker reads the listing. This follows directly from the code. The recorded fixture suite does not exercise either change of type.

Carry the selected kind through the queue and reject a changed source before staging. Cover both directions through the graphical command conversion and worker dispatch.

## Prior findings and evidence

The single-file Windows upload opens the reparse point and checks attributes on the held handle before reading. The Windows source probe passed. Native Windows runtime behavior remains unverified. The graphical close regression passes in folder-ui.log, including the close guard for a newly dirty editor. Keep editing clears the editor close request. The binary download compares received bytes with the listed size before publication. The short-transfer fixture in folder-transfers.log confirms preservation of the existing local file.

The recorded folder tests exercise fresh publication, entry-by-entry merges and unrelated entries, retained staging after download cancellation, ambiguous children and exact recursive-delete confirmation. Folder traversal uses bounded counts, depth and path bytes. Remote directory operations require machine-readable MLSD listings. FTP rechecks and rename do not offer a transaction or an anchored remote directory handle against concurrent server changes. The loopback fixtures and source probe do not establish live-server concurrency behavior. This review inspected source and recorded test results without independently running the suite.

## Fix for the transfer kind finding

FTP confirmation now queues separate file and folder operations. The upload worker verifies the opened source is a regular file before checking the remote destination or creating staging. The download worker checks the current MLSD entry type before creating staging or starting a data transfer. The explicit folder operations require a directory source. SFTP and SCP keep their existing transfer commands and behavior.

Both file-to-directory substitutions reproduced before the patch. The final loopback fixtures verified that neither transfer read the substituted directory's children and that each existing destination retained its original bytes. The transport and workspace suites passed 22 tests using the production library and original fixtures via an isolated Cargo harness during concurrent macro UI changes. [Failing regressions](folder-kind-review-red.log) · [Passing transport and workspace checks](folder-kind-review-green.log)

The FTP GUI filter passed all three cases after the operation conversion changed, including the new test for all four file and folder commands. After tightening the upload worker's validation order, its GUI rerun was blocked by missing methods and fields in concurrent macro UI work. The final actor and transport fixtures passed independently. [Final GUI build diagnostic](folder-kind-review-ui.log)

## Independent re-review verdict

The Important finding is resolved. The confirmation preserves the picker choice in all four queued commands. Only explicit folder commands enter a recursive worker. File upload checks the opened handle, and file download checks the current listing before staging. The two substitution tests failed with the old implementation and passed with the fix. Their assertions cover the operation outcome, retained destination bytes, absence of a recovery path, and absence of uploaded payloads or downloaded child content. No additional Important or Critical issue surfaced in the transfer-kind change. This verdict uses source inspection and the recorded fixture runs. The final integrated GUI rerun remains pending.
