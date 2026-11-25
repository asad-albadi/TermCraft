Extract the ZIP and open harbor-workspace.exe for portable use. Keep
termcraft-rdp-engine.exe beside the main executable.

For a per-user Start menu installation, open PowerShell in the extracted
directory and run the included installer.
  .\install-per-user.ps1 -Action Install

For upgrades, close TermCraft and install the new archive. Each version
gets its own program directory and Start menu shortcut. Close the version
you want to remove and run its installer from the extracted directory.
  .\install-per-user.ps1 -Action Uninstall

The installer checks package files and the shortcut target before removal.
Workspace settings stay in %LOCALAPPDATA%\Harbor when you remove program
files. The distribution is unsigned.

Optional Explorer integration (current user only): after installing or choosing
its final portable location, open TermCraft > Tools > Windows Explorer integration.
Use Status, Install for me, or Remove mine. This adds folder/background open-here
menus, termcraft: bookmark/folder URLs, and .termcraft-link file registration.
Existing or edited registrations are preserved and installation/removal is refused
if ownership differs. No administrator elevation or default-app override is used.

Before moving/removing an integrated version, use Remove mine from that exact
version. Then run its package uninstaller. The supplied uninstaller invokes a native read-only
reference guard before moving or deleting files and refuses removal while this
executable is referenced by owned/edited handlers or inspection fails. Unrelated
handlers are preserved. Upgrade by removing the old version's
integration first, then installing integration from the new version. If a user
changed any entry, TermCraft refuses automatic cleanup; inspect the change before
removing it yourself. An interrupted install may leave partial owned registrations;
status reports incomplete/conflicting registration rather than claiming success.

The Windows registry browser is also under Tools. It starts read-only, shows
access errors explicitly, and supports bounded typed reads. To edit an existing
value, enable editing, select it, preview the target and replacement, then confirm.
Changed values require a fresh read and confirmation. Registry values are not
saved in workspace JSON or written to application logs. Cancellation stops pending
work where possible; a value already written is not undone.

Local folder launches accept native absolute drive/UNC paths or /drives/c/path.
termcraft: URLs and .termcraft-link files use the conservative version-1 bookmark
contract and cannot run commands, carry credentials, or select a workspace.

Windows shell registrations use strict internal activation entry points. Each accepts
exactly one payload and no separate workspace/action options, so shell-expanded
quote characters cannot introduce --data-dir or commands. Ordinary explicit
--open-url/--open-file CLI use retains the existing trusted --data-dir behavior.

Integration cancellation uses the same request token during preflight, native
browsing and between mutations. A cancellation stops subsequent mutations and
reports the actual result. Completed/partial owned changes are preserved; refresh
status before continuing. Cancellation does not silently start cleanup mutations.
