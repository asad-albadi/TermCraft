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
0.1.0. A real version-changing upgrade and Finder's Services menu invocation
still require dedicated evidence.
