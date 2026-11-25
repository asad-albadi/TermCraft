# P01 Linux Mosh dependency closure (private candidate 0.1.924)

Approved P01 intent: bundled Linux Mosh must bootstrap SSH and run its Perl wrapper on a clean supported machine without installed Perl/SSH. Existing OpenSSH configuration, authentication, known-host decisions, ProxyCommand, and explicit SystemMosh remain compatible. No host packages or policy settings change.

## Selected design before implementation

Use independently pinned official Arch x86_64 Perl and OpenSSH binary packages, verify detached signatures with retained Arch keyring, and retain exact binary/source/license/Arch recipe inventories. Bundle both complete pure Perl core trees plus the four native XS modules (IO, POSIX, Socket, Fcntl) used by the wrapper and its DNS/address-family branches; unrelated Perl XS extension APIs are outside this internal interpreter interface. Bundle all non-glibc native dependencies of Perl/XS, OpenSSH, and Mosh client; glibc loader/libc/libm/libresolv remain an explicitly qualified OS ABI baseline, independent of the app's graphics ABI. Release remains Arch Linux x86_64 qualified, not universal Linux or Windows/macOS.

The compiled closure manifest lists every staged file, digest, mode and provenance. Launch opens and verifies each file without following resource symlinks, copies only those verified bytes into an owned mode-0700 tree, and retains it until the existing process-group cleanup completes. Native payload bytes remain unchanged from signed packages. Launch clears inherited loader/Perl injection variables before setting only the private LD_LIBRARY_PATH and private module paths; private LD_LIBRARY_PATH overrides Perl’s compiled OS RUNPATH. Files are 0500/0400. Explicit installed-tool choice does not enter this path.

The original Mosh script remains a pinned artifact. Its private launch copy receives a private Perl shebang and explicit private @INC before any modules load; therefore OpenSSH's recursive `$0 --fake-proxy` uses the same private interpreter/module closure. The --ssh and --client options must select verified package paths and are rewritten to private SSH/client images. No ambient lookup fallback is allowed on missing/changed files. The change preserves SSH arguments and user configuration/trust/authentication behavior. OpenSSH's /bin/sh use for configured ProxyCommand is a declared OS shell baseline.

Packaging takes an explicit pinned dependency-cache directory, rejects missing/different inputs and unlisted extraction, retains notices/corresponding source/recipes, and produces an exact staged-file map. No retained rootfs file is redistributed merely because it works.

## Work and evidence

1. Inventory authenticated upstream packages and exact source/build-recipe inputs; capture signer and dependency graph.
2. Add focused RED checks for absent closure/no-system fallback, source replacement, recursive proxy closure, injection environment, and cancellation.
3. Implement manifest verification/private launch and staging; run focused GREEN and normal private app build using jobs=2, --offline --locked and shared app/target with unique binary identity.
4. Run clean namespace without Perl/SSH installed using private scratch root only; actual owned SSH+Mosh server bootstrap, encrypted UDP/PTTY, unknown/changed host key rejection and cancellation. Preserve raw output and provenance.
5. Deliver narrow payload patch, baseline/candidate exact maps and limitations for root integration/review. No MAIN or full gate changes.
