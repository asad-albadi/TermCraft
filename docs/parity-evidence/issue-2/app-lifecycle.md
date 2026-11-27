# macOS application lifecycle evidence

The desktop integration tests cover ownership of the recorded bundle path:
removal refuses a moved or replaced application when the bundle identifier is
reused by a different executable. This preserves unrelated LaunchServices
registrations and avoids unregistering another application's bundle.

On macOS arm64 26.6.2, the native LaunchServices probe registered an owned
bundle, moved it to a path containing spaces and an apostrophe, registered
that path, then unregistered both owned paths. The unrelated registered
paths present at the start were still present afterward. The test uncovered
the native registration dump's hexadecimal suffixes and automatic removal
of an old path when a registered bundle moves.

Targeted `open -n -a BUNDLE ITEM --args --data-dir WORKSPACE` invocations sent
a `.termcraft-link` file and an encoded `termcraft://session/` URL to the
relocated launcher. Both items resolved a saved ID containing an apostrophe,
spaces and a percent sign, and the packaged helper sent a TPKT header to an
owned loopback endpoint.

A same-version replacement moved the old application aside, extracted a
different locked build, re-registered, launched, relocated and removed the
replacement. A workspace outside the app preserved its saved font setting,
the encrypted vault could still be unlocked, and the recovery file remained
byte-identical. The source revisions differed and both reported version
0.1.0.

An isolated 0.1.0 → 0.1.1 upgrade used separately built XQuartz-enabled archives
from `fe4fa0c115ea56a4469ebcf59a26d201791d27d3`. The test-only 0.1.1 source
snapshot changed the application version in Cargo.toml and Cargo.lock. Both
versions matched their bundle plist and package manifest. Their ZIP SHA-256
values were `1996095ad488279fd922fcb03663daaa05ed7bd46ee8b2bb13118517a7344cb9`
and `3f114af3ce928a1100617e4cf61b7cc6ceb260d9db05c73e3563ffb910a270b3`.
The previous app was moved aside during the upgrade. The new app was registered,
launched, moved to a name with a space, percent sign and apostrophe,
re-registered, launched with an empty PATH and removed. Font size 21, vault
bytes and recovery bytes survived. The vault unlocked with the test password
after each launch, and unrelated LaunchServices paths remained registered.

The Sonoma 14.8.7 replay registered the installed bundle and invoked
`NSPerformService("TermCraft/Open terminal here", ...)`. AppKit requires the
full registered name for programmatic invocation. The original call with only
the visible menu label returned false before reaching the provider.

The modern file URL pasteboard opened a terminal at the literal selected
folder. The original launcher failed the same test with a legacy
`NSFilenamesPboardType` payload. The patched launcher opened both folders,
including the apostrophe, percent sign, and spaces, in the existing isolated
workspace. Installation and removal refreshed Services using AppKit. The
fixture observed the two working directories through `lsof` and confirmed
the bundle was unregistered on removal.

Run `python3 scripts/check-macos-services.py /path/to/extracted/TermCraft.app`
in an isolated native macOS desktop login with Command Line Tools. The verified
launcher-only repack reused the previously qualified GUI and RDP helper. Its ZIP
SHA-256 was `ca8ffc6a4aca62087a3ba35db5712dc3a127434c3dadc7c68b47b8b8f34f2926`.
The remaining Finder check is the visible context-menu interaction.
