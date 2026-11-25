# P06 plugin lifecycle

The Tools page has a Plugins section for inspecting bundles, installing or updating a release, granting native execution, disabling, invoking an entrypoint, and removing a plugin. The picker, validation, registry access, process invocation and retirement run on a worker. Each install starts disabled, and an update requires a fresh grant for the new release digest. Nothing runs on inspection, install, upgrade, enable, app startup or terminal startup.

## TermCraft plugin format

A release consists of a `.termcraft-plugin.json` manifest beside its named `.tar.gz` archive. For example, a Linux x86-64 release with an executable called `example` can use these fields. Replace the two digest placeholders with the lowercase SHA-256 of the archive and executable bytes.

```json
{
  "schema": 1,
  "id": "example",
  "version": "1.0.0",
  "host_protocol": 1,
  "platform": "linux",
  "architecture": "x86_64",
  "archive": "example-1.0.0.tar.gz",
  "archive_sha256": "<archive-sha256>",
  "binary_sha256": "<executable-sha256>",
  "executable": "example",
  "entrypoints": ["check"],
  "permissions": ["native-execution"]
}
```

The gzip stream contains one tar archive with two regular entries, `bin/example` and `notices/LICENSE`. Include both paths explicitly when creating the archive. On Windows the manifest uses `windows` and the filename ends in `.exe`. Versions are three unsigned decimal components. A manifest may declare up to twelve lowercase command identifiers. Plugin authors supply one native executable and its notice. Dynamic dependencies must already be available to the operating-system loader. There is no bundled dependency resolver or post-install hook.

The child receives one request on stdin, such as `{"protocol":1,"entrypoint":"check"}` followed by LF. It returns one line of JSON on stdout with the same protocol number and a Boolean `ok` field, then exits. Success requires an exit code of zero and `ok: true`. The host bounds the response to 4 KiB and waits up to five seconds. The host discards stderr and reports structural, protocol, size, timeout and exit errors without copying raw child output into TermCraft logs or workspace JSON. The request contains only the protocol version and declared entrypoint. The plugin runs in its release directory in a separate process with full current-account access. The permission dialog makes that authority explicit. The digests pin reviewed bytes and the user decides whether to trust their source.

The shared archive reader snapshots and hashes compressed input, validates the native executable header, and extracts only allowlisted regular files. Links, absolute paths, traversal, unexpected or duplicate entries, invalid executable digests and oversized archives fail before publication. The registry replaces a staged release atomically. A lease protects each running version during update or removal, and cleanup deletes only matching owned files. Linux hashes into an anonymous executable image and seals it against writes, growth and truncation before executing that snapshot. Holding the installed source descriptor alone does not freeze writable bytes. Linux uses a dedicated process group and reaps its child. Windows launches an isolated helper in its owned Job Object with handle inheritance disabled; the helper duplicates the non-inheritable parent pipe ends into its own process, then launches the plugin with an explicit standard-I/O handle list. The Windows verified executable handle stays live for the full invocation. Only the helper has inheritable plugin handles.

Retirement uses the current P05 identity-aware quarantine helper. It checks owned digests, moves the current occupant into a private directory, verifies its identity and digest again, and deletes only that isolated matching file. Concurrent replacements remain recoverable. Cleanup snapshots retired records under the registry lock, releases that lock before filesystem work, and uses a separate cleanup lock. Every hash observes the request cancellation flag; cancellation defers unfinished cleanup until a later uncancelled request. The collector scans the complete bounded retired snapshot so busy earlier leases cannot starve later releases. Install/remove commit results remain authoritative once committed.

## Evidence

- The isolated Linux gate ran `cargo test --offline --test plugins --test package_manifest --test package_manager -- --include-ignored --test-threads=1`. All seven plugin cases, all three real Portable Unix package cases and the Rust packaging contract test passed. The packaging contract reported one skipped Xwayland archive case. The formatter check passed. The run is recorded in `target/p06-verified-targeted.log`. The isolated crate used a distinct temporary Cargo version to keep workspace artifacts separate in the shared build directory. The handoff does not change the application version.
- Linux x86-64 native fixtures cover a crashing plugin alongside an active production PTY, no pre-grant or disabled invocation, explicit entrypoint execution, stale permission rejection, an altered installed executable, failed and cancelled updates, lease-deferred removal, retention of modified and user-created files, traversal, absolute paths, symlink and hardlink entries, oversized entry rejection, wrong architecture, malformed and wrong-version IPC, oversized IPC, cancellation and the execution deadline. The fixture is compiled locally from C source in `tests/plugins.rs`.
- The x86-64 Windows plugin host and package primitives passed a component `cargo check --offline --target x86_64-pc-windows-msvc`. The Windows launcher and UI still need a native end-to-end run.
- The official Exif release at [Exif.mxt3](https://mobaxterm.mobatek.net/plugins/Exif.mxt3) has SHA-256 `7324ba44ab471ca5948fcc0d6c977ce536a26ec31c30744a35e48e6838187058`. Download and read-only inspection with `file`, Python `zipfile`, and `objdump -p` found a ZIP with 134 entries, including 32-bit PE `bin/exif.exe`, `bin/cygexif-12.dll`, locales and LGPL 2.1 notices in the bundled documentation. Direct imports of `exif.exe` include `cygwin1.dll`, `cygintl-8.dll` and `cygpopt-0.dll`, which are absent from this ZIP. The official catalog instructs users to put its plugins next to the MobaXterm executable. This fixture establishes one container and dependency shape. The Personal 26.6 loader behavior, runtime dependency closure, representative compatibility and redistribution review remain open. TermCraft currently accepts the independently defined format above. [Official plugin catalog](https://mobaxterm.mobatek.net/plugins.html)

## Premerge findings follow-up (2026-11-24)

Progress remains 58.3% (35/60 implementation checkpoints), pending the parent integration/review checkpoint. The four Important findings in `review.md` have implementation changes in the private P06 checkout. `target/p06-review-red.log` records both original cleanup regressions failing: concurrent enable returned Busy and the replacement filename disappeared. `target/p06-image-red.log` records a real locally compiled ELF replacing the verified installed inode and changing the executed response. The corresponding four Linux regressions passed in `target/p06-review-green.log`; the image regression also verifies writes to the sealed image fail with EPERM.

The Windows native-only regression creates an overlapping ordinary piped child while plugin launch is prepared, verifies all parent endpoints remain non-inheritable, and requires plugin stdout EOF while the unrelated child remains alive. The actual packages/plugins modules and this test passed `cargo check --offline --tests --target x86_64-pc-windows-msvc` in the small Windows component crate (`target/p06-windows-fixes-check.log`). This environment has no native Windows runtime; neither native Windows RED/GREEN nor native qualification is claimed.

The shared handoff patch is based on MAIN after the P05 pinned-license, cancellation, quarantine and complete-retired-snapshot fixes and B01 services integration. `target/p06-handoff/base-helper-compatibility.json` records exact base hashes. P06 preserves the P05 notice pin, uses its cleanup helper through visibility-only exports, and adds the generalized archive API without restoring the older receipt-based ownership or prefix-limited collector. The patch includes plugin-only UI/lib hooks and the Windows helper dispatch; it excludes the private Cargo version. The package index and pinned manifest equal the authoritative base.

Final targeted follow-up: `cargo test --offline --test plugins --test package_manager -- --include-ignored --test-threads=1` passed all seven plugin cases and all three Portable Unix cases, including production PTY isolation (`target/p06-fixed-targeted.log`). `cargo test --offline --lib plugins:: -- --test-threads=1` passed all four Linux lifecycle/image regressions. `cargo test --offline --lib packages:: -- --include-ignored --test-threads=1` passed both archive/header checks and all three real P05 retirement cases, including the ninth-release fairness regression (`target/p06-package-helper-green.log`). The registry-level ELF regression was separately shown RED by bypassing only frozen-image selection, then GREEN with it restored (`target/p06-registry-image-red.log`). Final formatting passed. The integration build emitted three existing dead-code warning groups in unchanged UI modules. No broad private UI suite was repeated.

Every common source file omitted from the handoff equals current MAIN by SHA-256 (`target/p06-handoff/common-source-compatibility.json`). The six common differences are exactly the six files in the shared patch. Private timestamp-preserving baseline copies initially allowed stale Cargo reuse; refreshing private source timestamps forced recompilation before the final passing targeted run. The shared executable was saved and restored around private integration, but concurrent private writers mean it is not a qualified MAIN artifact; the parent will rebuild MAIN before its next whole-app gate.
