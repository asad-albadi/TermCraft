# Independent P01 Linux Mosh dependency-closure review

Verdict: **ACCEPTED** for the stated Arch Linux x86_64/glibc 2.44 qualification. No actionable correctness, quality or security finding in the complete eleven-path delta. No MAIN edits or review subagents were used. No full gate or final release qualification is claimed.

## Scope and integrity

Reviewed all eleven owned paths against MAIN and authenticated payload.patch SHA-256 df3a5000522efe1f6ca0b6be56222610e8d7f2a1271d0298fd4d10f51f759a11. `verified-file-map.json` records candidate and baseline checks; MAIN pins were rechecked after verification. Review source uses version 0.1.931 and unique normal identity termcraft-mosh-review931; original worker source remained unchanged.

## Findings and design assessment

- The bundled catalog now fails closed without the pinned Perl/OpenSSH closure; explicit SystemMosh discovery is unchanged. Wrapper/client hashes remain authoritative.
- The manifest is compiled into the app and the installed copy must match exactly. Every selected regular file is checked with capability-relative no-follow opens, length/hash validation, setuid/setgid rejection and architecture checks for all declared native images. Inventory count/size bounds and 64 KiB cancellation points limit work. The private mode-0700 launch tree contains only pinned runtime files; files become 0500/0400. A failed validation drops the partial snapshot.
- Rewritten shebang and @INC point into the private closure, including recursive `$0 --fake-proxy`. The authenticated SSH prefix is replaced while its complete previously quoted argument tail is preserved. Private library/terminfo/helper settings override the corresponding ambient values and Perl/loader/OpenSSL injection settings are removed. Upstream source confirms both SSH_SK_HELPER and SSH_PKCS11_HELPER are supported. No authentication/trust arguments or user SSH configuration are rewritten.
- The Process retains the launch tree until after its existing process-group signal/reap teardown. Cancellation before spawn and during authentic closure copying were independently exercised. Private paths are short fixed-/tmp paths, preventing environment-controlled shebang/Perl quoting changes.
- Offline staging accepts only pinned archives/signatures/upstream sources/recipes and enumerated regular archive members. It refuses missing/replaced/symlink inputs and output overwrite. Package construction propagates the mandatory fourth Mosh input and inspects the client against private libraries. Existing composition fixtures remain compatible.
- Independently rechecked all 20 detached archive signatures against the retained Arch keyring, expected signer fingerprints, archive/source/recipe/signature hashes, all 1,834 runtime files totaling 75,910,466 bytes, and all 83 ELF x86_64 headers and complete DT_NEEDED lists. Source/recipe archives and notices/metadata are retained per package; native bytes remain unchanged. Clean loader audit independently resolved all non-glibc dependencies privately, including XS modules and helper images.

## Fresh verification

- `runtime-tests.log`: four ordinary integration tests passed; two named qualification fixtures retained their existing explicit ignore markers and were exercised separately below.
- `missing-closure.log`: authentic wrapper/client without dependencies rejected.
- `staging-unit.log`: all three offline safety checks passed.
- `stage-full.log`: full offline CLI staging passed.
- `packaging-api.log`: actual package-release stage_mosh API passed with private client-library inspection.
- `packaging-compat.log`: existing test_composes_with_xwayland_and_mosh passed (actual pinned Unix cache supplied; no skip).
- `mosh-unit.log`: actual authentic snapshot cancellation passed in 0.01 seconds.
- `mosh-bin-tests.log`: both existing Mosh startup/input/cancellation UI checks passed.
- `normal-help.log`: normal review binary executed successfully.
- `clean-run/argv-final.json.log`: fresh review integration binary ran in clean filesystem/user/PID namespaces without installed Perl/modules/SSH client, with empty child PATH. Strict unknown/changed host keys were rejected, then genuine SSH key authentication produced the encrypted UDP PTY frame `runtime-mosh-udp`. The same fixture exercised missing/replaced/symlinked dependencies and invalid PERL5OPT/LD_LIBRARY_PATH.
- `clean-run/loader-audit-argv.json.log`, `clean-run/loader-audit.json`: all 83 native payload objects passed a fresh clean-root private-library resolution audit.
- `signature-verification.log` and `inventory-audit.json`: fresh signature, byte, architecture and DT_NEEDED checks.

## Qualification boundaries

This accepts the qualified Arch x86_64/glibc 2.44 closure, not Debian/universal Linux or native Windows/macOS. OS loader/glibc/kernel and /bin/sh remain requirements. Physical security-key/provider/agent integration, configured external plugins/commands and privileged hostbased SSH remain explicit user/OS integration limits; no claim of hardware qualification or reproducible rebuild is added. Corresponding source/recipes are shipped and authenticated/pinned as documented. Parent all-target/full release integration remains required.

All review commands completed. Review namespaces exited under --unshare-pid/--die-with-parent and no review-owned server or container remains; unrelated existing jobs were left untouched. `process-audit.txt` retains the final process snapshot.
