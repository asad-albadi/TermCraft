# NFS shared-hook rebase after reviewed Cron integration

Rebased the six shared files against the current MAIN snapshot, preserving the reviewed NFS implementation byte for byte at SHA256 `59034433641a4338067b8ad20063cb098cfdaa881d82e3256b1703bcfaff89de`.

The patch adds only the NFS dependency pins, module/protocol/configuration, listener dispatch/address/serve arms, default port/lifetime and existing managed-server form/card/test. The existing Cron enum, runtime, status, configuration wrappers, scheduler dispatch, model default, UI configuration guard and Servers card remain. Cargo retains chrono 0.4.45 and Win32_System_Registry. The credential password-field function was compared byte for byte with MAIN and remains identical. The Servers gap label now names only the still-unmerged VNC service; parent should reconcile that sentence when merging VNC.

`base-sha256.json` now identifies this MAIN snapshot; `nfs-rebase-base-proof.json` records matching hashes and preservation checks. `shared-hooks.patch` omits the private package version and leaves lockfile reconciliation to the parent. The recorded `patch --dry-run --fuzz=0 -p1` against MAIN succeeds for all six files, with no fuzz or offsets. No MAIN file was edited by this worker.

Targeted validation is recorded in `nfs-rebase-lib-check.log`, `nfs-rebase-lib-tests.log` and `nfs-rebase-ui-test.log`. These use the private 0.1.0-e03-nfs-private package and hashed unit-test artifacts. No broad suite, whole-project formatting, production app build or source implementation rewrite was performed. Prior component and Windows/macOS source proofs concern the byte-identical approved owned module; native execution/original equivalence remains open. Parent performs combined review/gates after applying the narrow patch.

Finished results: library check **exit 0**; targeted NFS library tests **10 passed**, including independent libnfs/hard-link proofs; actual Servers pointer test **1 passed**. All owned commands finished. Final fuzz-0 dry run and all six MAIN baseline hashes were checked again immediately before handoff.
