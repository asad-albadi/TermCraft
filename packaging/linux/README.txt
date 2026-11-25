Extract the archive and run ./harbor-workspace. Keep packaged helpers and
runtime manifests beside the application. See PACKAGE-MANIFEST.json for
platform dependencies and package file hashes.

Optional per-user desktop integration requires Python 3:
  python3 desktop-integration.py install
  python3 desktop-integration.py status
  python3 desktop-integration.py uninstall

Install copies this complete release into the XDG user data directory, then
adds the menu, icon, directory open-here and safe bookmark URL/file handlers.
The installed payload survives moving or deleting the extracted archive.
Close TermCraft before upgrading; install from the new extracted release.
Uninstall removes only unchanged owned files and preserves user defaults,
unrelated files, and workspace settings in ${XDG_CONFIG_HOME:-$HOME/.config}/harbor.
Native helpers packaged in -arch-xwayland archives retain their Arch Linux
x86_64 ABI requirement.
