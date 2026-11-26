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

The AppKit Services provider accepted a real folder pasteboard and produced
literal `--data-dir` and `--cwd` arguments. It rejected a mixed folder/file
selection before dispatching any item. `NSPerformService` returned false from
the command-line harness, and Finder's Services menu invocation remains open.
