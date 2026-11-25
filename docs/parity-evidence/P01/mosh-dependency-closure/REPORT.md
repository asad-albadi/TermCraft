# Private P01 Linux Mosh closure handoff

Candidate is ready for root review/integration. No MAIN source or packaging files were edited. All seven existing owned-file baseline hashes still match MAIN; four owned paths are new. No parent full gate or release completion is claimed.

## Concrete result

Bundled Linux Mosh now verifies and snapshots its own Perl interpreter/module closure, OpenSSH client/SK/PKCS11 helpers, terminfo and non-glibc native dependencies. It no longer discovers or requires /usr/bin/perl or /usr/bin/ssh. Explicit SystemMosh is unchanged.

The pinned inventory contains **1,834 runtime files / 75,910,466 bytes**, **83 native objects**, from **20 detached-signature-verified official Arch x86_64 packages**. Corresponding upstream source archives, complete Arch packaging recipe archives, signed-package metadata and notices are retained and staged. Native payload bytes are unchanged from authenticated packages. The full staged inventory (including Mosh, notices, source and legacy RDP fixture) has 1,964 regular files / 443,136,993 bytes; see staged-file-map.json. Six release source archives were also checked against the first exact SHA-256 in their Arch recipes; see source-recipe-sha256-checks.json. Git-source recipes retain their complete packaging sources plus independently pinned upstream tag/commit source archives, rather than claiming a byte-for-byte reproducible package rebuild.

## API/launch design

- `RuntimeCatalog::bundled_mosh()` verifies the complete pinned dependency payload and returns the packaged SSH path. System provider discovery is unchanged.
- `prepare_mosh_launch(program, args, cancelled)` verifies original wrapper/client bytes, snapshots dependency bytes through capability-relative no-follow reads, changes the owned script's shebang/@INC to the private Perl/module directories, and replaces only the authenticated SSH command prefix while preserving all quoted authentication/config/trust arguments. `$0 --fake-proxy` therefore recursively invokes the private Perl too.
- `MoshLaunch` retains the private tree and exposes its library, terminfo and SSH helper paths. Unix Process removes inherited Perl/loader injection settings, selects these private paths, and keeps the tree until existing process-group teardown.
- `mosh_dependencies.rs` verifies the installed manifest against its compiled pin, file count/total size bounds, regular files, permissions, architecture and hashes; snapshots the verified bytes in 64 KiB cancellation-aware chunks; and copies only manifest-listed files to 0500/0400 files beneath an owned /tmp mode-0700 tree. Source/notices do not enter the launch image.
- New offline packager input is `--mosh-dependencies CACHE`, required alongside the existing three Mosh archive/source/recipe arguments. Staging validates exact archive/member/source/recipe/signature hashes and writes only enumerated regular archive members, plus audited notices and corresponding sources.

## Evidence (fresh focused checks)

| Check | Result / raw evidence |
|---|---|
| Authentic Mosh-only package accepted before closure fix | RED assertion in red-missing-closure.log |
| Same package now fails without Perl/SSH payload | GREEN in green-missing-closure.log |
| Ordinary runtime catalog / legacy System profile checks | runtime-manifest-final.log: 4 passed, 2 qualified fixtures ignored |
| Actual dependency snapshot cancellation | cancellation-final.log: 1 passed, stops between chunks; 0.02 seconds fixture |
| Offline pin replacement/absent/symlink/output checks | staging-tests.log: 3 passed |
| Actual packaging API stages full closure, private native library inspection | packager-stage-check.log: success |
| Actual clean namespace, installed Perl/SSH and Perl modules absent | clean-run/output-final.log: success; exact root and binds in argv-final.json |
| Empty PATH client, recursive private fake-proxy, strict unknown/changed host-key rejection, genuine SSH public-key bootstrap and encrypted UDP PTY terminal output | same clean log: successful `runtime-mosh-udp` terminal frame after two strict host-key rejections |
| Missing/replaced/symlinked wrapper/client/interpreter/SSH/library/module rejected with no fallback | same clean fixture checks all before successful connection |
| Deliberately invalid inherited PERL5OPT and LD_LIBRARY_PATH | successful clean encrypted PTY frame proves launch selected its private settings |
| All 83 native payload objects resolve only private non-glibc dependencies in clean root | clean-run/loader-audit.json and loader-audit.log: success |
| Normal private application binary | normal-build-final.log and normal-help-final.log: success; normal-binary.json pins 0.1.924 / termcraft-mosh-closure924 |
| Narrow integration preview | patch-dry-run.log: all 11 paths pass |

Clean fixture runs in its own filesystem/user/PID/IPC/UTS namespaces. Its OS files come from the retained previously audited immutable clean image export, copied into a new private root. Only private scratch Perl/SSH copies were removed. No host libraries, credentials or home directory were bound. Network is shared only to use owned ephemeral loopback endpoints; no routes/interfaces/policies were changed. The non-root remote SSH fixture uses --no-ssh-pty to avoid privileged OS login accounting; genuine Mosh client/server PTYs remain exercised. Its separately signed libutempter library is server-only evidence, not a client dependency. See clean-root-provenance.json and clean-run/server-library-provenance.json. All own jobs/servers are gone; no container was created by this worker and existing images were retained.

## Qualification limits

This is a qualified **Arch Linux x86_64 / glibc 2.44** candidate. The actual maximum required glibc symbol across the closure is 2.44 (glibc-symbol-baseline.json). OS glibc loader/libc/libm/libresolv, Linux kernel and /bin/sh remain explicit platform requirements; app graphics/display requirements are separate. This does not establish universal Linux, Windows/macOS, physical security-key devices, arbitrary custom PKCS11/Kerberos plugins, configured external commands, or privileged hostbased SSH integration. User SSH config, known hosts, keys and agent/provider/hardware integrations retain their existing user/OS prerequisites; no setuid helper or policy change is introduced. Corresponding source is shipped; this is not a reproducible-build or general-purpose Perl runtime claim.

Normal binary was built in the debug profile with the required unique identity; a combined release artifact, root full gate and independent review remain parent-owned.

## Payload and integration inputs

- `payload.patch`: **df3a5000522efe1f6ca0b6be56222610e8d7f2a1271d0298fd4d10f51f759a11**, 657,799 bytes.
- `owned-file-map.json`: exact existing MAIN baseline/current and private candidate hashes for all 11 payload paths. Private Cargo identity changes are deliberately excluded from this patch.
- `baseline-sha256.json`: package-release.py source_inputs baseline; snapshot also copied complete tests and FEATURE_PARITY resources, without recursively copying target.
- `source/`: complete private source snapshot; `cache/`: exact pinned offline dependency inputs; `staged/` and `packager-stage/`: retained concrete payloads.
- Compiled dependency manifest SHA-256: **e615650cccd848dfbf457ef3b7df62fc4e691921daaf9156711139e2d3238506**.
- `DESIGN.md`, `RULINGS.md`, `COMMANDS.md`, signature-provenance.json and exact source/native/file inventories document the design, implementation choices and reruns.

Root should apply the narrow patch against verified base hashes, integrate/review it, supply the new cache flag during release staging, and qualify its combined application release. No MAIN change has been applied here.
