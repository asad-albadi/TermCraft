TermCraft for Apple Silicon macOS

This archive is unsigned and not notarized. Extract it with Finder or
ditto to preserve executable permissions. Move the entire TermCraft.app
bundle to Applications or another folder you control, then open it in
Finder. Keep the bundle intact when moving or copying it.

Optional per-user registration requires Python 3. In Terminal, set APP to
the actual location of the extracted or installed bundle, for example:

  APP="$HOME/Applications/TermCraft.app"
  python3 "$APP/Contents/Resources/desktop-integration.py" install
  python3 "$APP/Contents/Resources/desktop-integration.py" status

This registers the bundle with LaunchServices without setting your default
handlers. Close TermCraft before replacing it or moving it. If you used
optional registration, unregister before moving or deleting the app:

  python3 "$APP/Contents/Resources/desktop-integration.py" uninstall

After a move or upgrade, update APP to the new bundle path and run install
again if you want to register it. Uninstall removes the registration
tracked by TermCraft, not the app bundle. To remove the program, move the
bundle to the Trash after unregistering it.

Workspace data is stored outside the app in
~/Library/Application Support/Harbor by default. Moving, replacing, or
removing the app does not remove this data. The optional registration
receipt is stored separately in ~/Library/Application Support/TermCraft.

See LICENSE, PRIVACY.md, CREDITS.md and PACKAGE-MANIFEST.json in this folder
for project policies and package contents.
