# macOS application lifecycle evidence

The desktop integration tests cover ownership of the recorded bundle path:
removal refuses a moved or replaced application when the bundle identifier is
reused by a different executable. This preserves unrelated LaunchServices
registrations and avoids unregistering another application's bundle.

Native Finder, URL, Services, relocation, and moved `.app` smoke evidence is
still open. It requires a built `.app` with the adjacent Rust launcher and
literal event forwarding exercised on a macOS desktop session.
