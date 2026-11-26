# Apple Silicon bundled Chromium qualification

PR #5, native arm64 on macOS 26.6.2 (25G83). This is a local **qualification
fixture**, not a distribution payload. Default builds and the shared release
packager still do not enroll Chromium for macOS.

## Exact input and source

- The official Mac_Arm snapshot at position **1755275** is Chromium
  **160.0.8185.0**. The Google Cloud object generation is `1795663503237994`
  and the archive size is 185,523,327 bytes. SHA-256 is
  `994053eadf27c3d03558b81ad67ec7bbed4943f6b7dd0cce75408cb23bfe70fc`.
  The local archive also matched the storage object's MD5
  `cJIV9qVbDA42W0tJH/jxuA==`. [Original archive](https://storage.googleapis.com/chromium-browser-snapshots/Mac_Arm/1755275/chrome-mac.zip)
- The snapshot's upstream revision record has SHA-256
  `d11bc51943ad5eba7179dd4685e6a9b57dff9c626bd3838c049c79230ab7686c`.
  Its `got_revision` is `2de5612046d43b6f874320ba0e5357146cb225c5`.
  Gitiles confirms the commit position and `chrome/VERSION`. The Chromium
  BSD license was taken from that commit. CDP reports the exact version but
  returns an all-zero revision. The source match therefore comes from the
  snapshot's upstream build record, not a revision embedded in the binary.
  This is not a claim of an independent reproducible build or vendor signature. [Revision record](https://storage.googleapis.com/chromium-browser-snapshots/Mac_Arm/1755275/REVISIONS)
  [Exact source](https://chromium.googlesource.com/chromium/src/+/2de5612046d43b6f874320ba0e5357146cb225c5)
- The original app failed `codesign --verify --deep --strict` with
  `code has no resources but signature indicates they must be present`.
  Offline staging produced six bundle resource seals and passed strict
  verification. The separately sealed trees matched byte for byte. All 12
  thin arm64 Mach-O images retain identical file-backed Mach-O sections
  and all 319 original non-Mach-O resources retain identical bytes. The
  original executable permission on the unused `install.sh` is not restored.
  The fixture pin contains 338 files including the Chromium license and five
  exact relative links. The links point to `Versions/Current/{Chromium
  Framework,Helpers,Libraries,Resources}` and from `Versions/Current` to
  `160.0.8185.0`. Its manifest SHA-256 is
  `e3d8c3c024fd39329315e007886beaab4f2e88e6e2bd27ab8a95a69befd1c66f`.
- `otool` reported minimum OS 13.0 for all 12 native images and no
  `LC_RPATH`. Other than the system libraries and frameworks, dependencies
  load from inside the bundle through `@executable_path`. The graphics
  libraries' `@rpath/libvk_swiftshader.dylib` and `./libvulkan.dylib`
  entries are library IDs, not load dependencies. This inspection does not
  establish runtime support on every macOS version or GPU.

## Offline packaging contract

Run `python3 -I -B packaging/browser/stage-macos.py "$resources" "$inputs"`
on native Apple Silicon. The only required input file is the exact
`chrome-mac.zip`. The stager copies and hashes that file before reading
it, then publishes a new `$resources/browser/macos-aarch64` with
`Chromium.app`, `BROWSER-MANIFEST.json` and `LICENSE`. It keeps original
archive extraction and generated ad-hoc signatures separate from the trust pin
by checking every resulting file and link before publishing. It does not
download inputs, accept alternative browser paths or replace an existing output.

An app ZIP must store those five framework links as Unix symlink entries with
the literal target text as entry data. Keep file SHA-256s and the exact link
targets distinct in the inventory, or use explicitly typed symlink records
whose SHA-256 covers the raw target bytes. Do not inventory links by hashing
the contents of their destinations. After extraction, verify all five texts and
run `codesign --verify --deep --strict` on the nested app. The outer release
signing process must not silently re-sign the nested runtime because that
changes the compiled file pins.

Compile TermCraft and the browser tests with
`--features macos-chromium-qualification`. Put the browser test executable
and library test executable in an isolated copy of
`TermCraft.app/Contents/MacOS` so the real runtime catalog resolves the
sibling `Resources`. Run the browser session harness with
`TERMCRAFT_TEST_BUNDLED_BROWSER=1` and the ignored
`macos_qualification`, `bundled_renderer_is_seatbelt` and
`close_during_whole_resource` library tests. Set
`TERMCRAFT_TEST_BROWSER_ROOT` to the absolute Resources path for the first.
Use a disposable package since the trust test changes and restores fixture
files. Set a short, absolute `TMPDIR` on a volume with room for private
copies, for example `mktemp -d /Volumes/Dock/b.XXXXXX` on the test host. A
session owns both its profile and its approximately 400 MB browser snapshot.
The stager and ZIP assertions are in `tests/packaging_browser_macos.py`.

The shared packager's qualification ZIP probe had SHA-256
`f1026cb660c03bd30b8dd66c89e301ea1adeb1dd88d1d604f1092ad1d89fd8b2`.
After relocation, all 339 browser file hashes and five symlink targets matched
its package manifest and the nested app passed strict verification. This is
separate from the production package and its distribution signature.

The fixture requests PNG frames. In a controlled CDP capture from the same
build, the JPEG at SHA-256
`9b0e639981baf35b32613c76fcf75b5ad4e85c8a8ab50ae570493c9c4211e59c`
decoded pixel (100,100) as RGB (254,0,0) in macOS and (0,211,0) in
TermCraft's Rust decoder. PNG avoids that incorrect rendering without
changing the existing installed-browser or Linux capture format.
The bundled macOS launch places the Crashpad database in the disposable
profile. The ordinary crash reporting flags and
`--disable-crashpad-for-testing` still started macOS handlers against the
real home. The exact Chromium source initializes Crashpad on macOS regardless
of that switch and honors `BREAKPAD_DUMP_LOCATION` for the database.
[Exact crash reporter source](https://chromium.googlesource.com/chromium/src/+/2de5612046d43b6f874320ba0e5357146cb225c5/chrome/app/chrome_crash_reporter_client.cc)

The relocated native tests confirmed that the exact links survive snapshot
creation, the snapshot still passes strict verification, and changed links
and same-length file mutations fail closed. Closing during verification is
chunk-cancellable. A renderer returned Seatbelt status 1 while the browser
returned 0. Closing the tested browser removed all nine observed owned browser
processes, including the private Crashpad handlers, as well as the profile and
snapshot. A peer Chromium session stayed responsive.

The pinned snapshot reported a successful bottom-right click at 300×200 and
DPR 1 after history navigation, with `pageshow.persisted=false`. The same test
still requires the history transitions, frame pixels, matching viewport and
actual browser input for both dimensions. It does not require a back-forward
cache hit from this snapshot. The installed-browser and Linux expectations
are unchanged.

The relocated render, text submission, history and both resize/click checks
passed when run in isolation with the final binary. A combined workflow run
passed settings, site authentication/TLS and proxy consent but timed out
waiting for one resized frame. Report this as a repeatability gap, not a
blanket pass for the combined run.

## Remaining distribution blockers

This snapshot's 447-byte `chrome://credits` output explicitly contains sample
text, not corresponding third-party notices. Its SHA-256 is
`d593c51cceb2db6655e9fc5d65aaf60545dfda5c58c4f6dae14ded7efcb180dd`.
The relocated, sealed browser also returned the placeholder over anonymous
CDP pipes. After `Page.navigate` to `chrome://credits/`, `Runtime.evaluate`
returned a 454-byte DOM with SHA-256
`6496eba6f55d819f90d157dd1de2d202db1d397ae8b4f75ef82239055ad57bef`.
That DOM instructs build maintainers to set `generate_about_credits=true`.
CDP can capture built-in credits from a build that actually generates them.
It cannot recover notices omitted from this snapshot. Use matching source and
build inputs to generate and audit the notices, or obtain a candidate that
already embeds the complete notice inventory. Do not substitute the Linux credits or redistribute the fixture. The local
ad-hoc resource seal is not publisher trust. Gatekeeper rejected the sealed
app and the host had no valid signing identities. Chromium upstream describes
snapshots as best-effort rather than stable builds. The separately notarized
ungoogled macOS release checked here was 157.0.8103.78-1.1, behind the
current 158.0.8127.88/.89 stable security baseline, and not the same source or
binary. [Chromium snapshot documentation](https://new.chromium.org/getting-involved/download-chromium/)
[Notarized alternative](https://github.com/ungoogled-software/ungoogled-chromium-macos/releases/tag/157.0.8103.78-1.1)
[Stable baseline](https://chromereleases.googleblog.com/2026/11/stable-channel-update-for-desktop_01734282054.html)

To enroll a release candidate, obtain a maintained arm64 Chromium build with
complete source-matched notices and a reviewed signing/notarization identity.
Then pin its final signed files, replay the native tests against the final
shared package and qualify the supported OS baseline. Do not enable
`--browser-inputs` macOS releases with this fixture.

The only local SDK was 26.2. This exact revision pins 27.0 for the official
arm64 SDK and 26.5 for the universal fallback. This run did not independently
rebuild the browser. [Exact upstream SDK configuration](https://chromium.googlesource.com/chromium/src/+/2de5612046d43b6f874320ba0e5357146cb225c5/build/config/mac/mac_sdk.gni)
