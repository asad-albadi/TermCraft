# Packaged Chromium runtime

The optional Linux x86_64 package carries Chromium **158.0.8127.88** from the
signed Arch Linux package published on 2026-11-24. The complete direct engine
resource tree, sandbox/crash helpers, locale packs, ICU/V8 data and graphics
providers are pinned in `runtime-manifest.json`. A separately signed, pinned
simdutf 9.3.1 shared library supplies the engine's new ABI privately. This is
Chromium, with no Google Chrome or proprietary Widevine payload.

New Browser session forms select **Packaged Chromium**. The session also offers
**Installed Chromium (compatibility)**, and **Default system browser** remains
the explicit external alternative. Existing profiles with no runtime field and
`browser::Session::start(Config)` retain their installed Chromium behavior. The
new `start_with_runtime` API preserves existing `Config` struct literals. A
missing, wrong-platform, altered or incomplete bundle reports an error; it never
selects an installed browser automatically.

The runtime catalog checks the compiled platform/version pin, installed manifest,
SHA-256 and size of every resource, native headers and executable permissions,
and refuses symlinked resources. Unix copies all resources into an owned mode0700 runtime directory while hashing
the same bytes, then launches that verified private snapshot and retains it until
process cleanup. Engine files are mode0500 and data mode0400. Resource verification
checks cancellation before and after each 64KiB read, so closing a tab during
verification does not wait for the entire bundle. Browser work stays off the UI
thread. Launch retains the existing private profile and anonymous DevTools pipes. Only the pinned
private library directory is added to its loader path; inherited loader injection
variables are removed. No DevTools TCP listener is enabled. Normal certificate
verification and Chromium namespace/seccomp sandbox remain enabled. The sandbox
helper is installed without setuid permissions: the target OS must allow Chromium's
standard unprivileged namespace sandbox. There is no unsafe launch fallback.

## Offline staging

### Windows x64

`runtime-manifest-windows.json` records the pinned Chromium source and the
current approval state. The Windows product and release package both require
approval in that repository manifest. The Windows staging input, once approved,
is a native Chromium ZIP with the exact pinned SHA-256. Its regular files must
match the approved inventory by path, size and SHA-256, including
`THIRD-PARTY-NOTICES.html`. Packaging verifies the PE architecture and records
named imports for each pinned native image. The installed browser compatibility
workflow uses a separate explicit provider.

### Linux x86_64

Supply this explicit directory to `scripts/package-release.py --browser-inputs`:

- The exact Chromium archive named in the manifest and its detached `.sig`.
- Each exact dependency archive and `.sig` listed in `dependencies`.
- `THIRD-PARTY-NOTICES.html` with the pinned digest (the captured complete
  `chrome://credits` document for this engine is supplied beside the manifest).
- `CHROMIUM-SOURCE-RECIPE.tar.gz` with the pinned digest. Its PKGBUILD digest
  matches the signed package's `.BUILDINFO`, and the archive contains all Arch
  patches/build scripts at the version tag.

The stage script verifies all explicit inputs before publishing the tree. It
extracts only listed regular members into a temporary directory, strips privileged
bits, and refuses an existing destination. It makes no network requests and
never discovers an engine from the host. Package construction records native
loader/library requirements for the engine, helpers and each private shared
library, hashes every packaged file, and retains the six existing application
assets and the existing Mosh, Iperf, Unix and native macOS launcher options.

The engine is **not** a statically portable Linux dependency closure. It targets
the Arch Linux x86_64 ABI and requires the system libraries/fonts/trust store in
`SOURCE-PKGINFO` and the package manifest's `linked_libraries`. The build inventory
is `SOURCE-BUILDINFO`; the original license and complete component notices are
included. The exact official Chromium source archive URL and SHA-256 are recorded
from the signed-build-matched recipe; the multi-gigabyte upstream source archive
is not duplicated inside the browser payload. No host packages are installed.

## Updates and qualification

Release maintainers own browser security updates. Before each shipping release,
check the current supported Chromium security release and distribution advisories,
refresh the signed archive/dependency pins and notices, verify the enrolled distro
signing key, match the recipe to signed `.BUILDINFO`, and repeat relocation,
resource-mutation, sandbox, HTTP/authentication/TLS/input/resize and cleanup checks.
Ship the changed engine through an application release; Chromium's background
component updater is disabled for reproducibility. If qualification fails, keep
the new bundle unavailable; rollback requires a deliberate reviewed package pin
and must not silently re-enable a vulnerable engine.

This candidate has Linux workflow evidence. A clean machine with no installed
Chromium and the clean OS/ABI matrix remain separate qualification requirements.
Windows and macOS bundled selections are explicitly unavailable until an audited
source-built native payload/complete resources/signatures are pinned. Existing
Windows known-folder discovery and private helper/handle/job/image checks remain
unchanged for installed compatibility mode. The Windows helper accepts no arbitrary
engine path. Existing macOS installed app discovery/native pipe transport likewise
remain available. Cross-target source checks do not establish native execution,
Windows sandbox token/job behavior, macOS signing/notarization, or native TLS trust.

Primary references:
[Arch package](https://archlinux.org/packages/extra/x86_64/chromium/),
[exact recipe](https://gitlab.archlinux.org/archlinux/packaging/packages/chromium/-/blob/158.0.8127.88-1/PKGBUILD),
[upstream source](https://chromium.googlesource.com/chromium/src/+/refs/tags/158.0.8127.88),
[Linux sandbox implementation](https://chromium.googlesource.com/chromium/src/+/main/sandbox/linux),
[headless CLI including internal-page access](https://developer.chrome.com/docs/automation-and-testing/headless-cli).
