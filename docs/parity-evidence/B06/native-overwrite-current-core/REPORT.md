# Genuine Debian core release: configured GTK game-file qualification

PASS for the tested scenarios. This is the genuine Debian 12 core release running in the private configured Arch GTK fixture. It does not claim an Arch release artifact or a stock Debian desktop test.

Archive SHA-256: 73ebdf5d695ad2d3a84c3522aacbc1b13046619378ea25ed601055523ac45ae9
Source snapshot SHA-256: d6e6c3aca99a068aedc0479cb376a7d0927a4b2847ef3b0aed8e7d6d88a2e7d9
Application SHA-256: 26b82706c068c8de1263f60b0448869adb0dcc501cb6b078ccdcbd30cba58de5

The runner independently checked archive digest, source snapshot in the manifest, manifest application digest, and exact application/manifest bytes against the archive. Parent supplied the strict extraction and complete 1074-file manifest verification. package-identities.txt records exact archive and extraction paths; inputs.sha256 records the fixture inputs.

Observed and checked:
- Genuine native GTK Save/Open dialogs activated through the private D-Bus portal; Zenity absent. GTK Replace was followed by the app-owned Replace saved game modal (07/10/13/19 screenshots). Actual app consent was visually inspected before coordinate selection.
- Keep existing file preserved the 331-byte legacy file and left the current dirty game intact (08).
- A second writer replaced the target inode while app consent was pending. Clicking Replace file rejected the changed target; both replacement contents and original renamed file were preserved (11).
- Clicking actual app Replace file for the unchanged target saved the exact expected 345-byte chronological Move, Undo, Redo file, SHA-256 cc81281280fbbbe3e4ae8ac62f513a7acf964b677f0d488bb9721620b9baa5eb (14 and approved.termcraft-game).
- Old 331-byte save loaded; Undo and Redo worked. The replaced save loaded after New game; Undo/Redo again worked (03–05 and 15–18).
- Save chooser cancellation preserved all disk bytes. Keep playing in the dirty Load guard and subsequent native Load chooser cancellation retained the dirty board. Invalid file selection after explicit discard consent produced a visible malformed-save message, preserved all disk bytes and retained the dirty board and full prior chronology (21–28).
- A real WM_DELETE_WINDOW during app overwrite consent canceled the pending publication before the worker won its publication claim, preserved the close target, and displayed the unsaved-game guard. Keep playing resumed the same game (19/20/29).
- A final new-path save serialized exactly Move,Undo,Redo,Undo,Redo with unchanged final-state integrity (30, final-state.termcraft-game, chronology-verification.json), proving canceled and invalid loads plus native-close cancellation retained history. Clean subsequent native WM_DELETE_WINDOW closed the application (31).
- No .termcraft-* staging files remained at checked checkpoints. Per-checkpoint SHA-256 records, command traces and screenshots are included.

Isolation and cleanup: bwrap unshared all namespaces with private home/tmp/run, private authenticated Xvfb display, private D-Bus session, no host screen, input, clipboard, home or session bus. Overlay libraries and portal package provenance were recorded. The runner exited zero; cleanup marker exists; /proc scanning found no remaining PIDs in the owned PID namespace (post-exit-namespace-check.json). Disposable derivative-root was removed. Prior sealed evidence was untouched. No product source edits occurred.

Harness notes: step-09 returned 127 after completing the cancellation because cmp was absent in the fixture base. Host cmp and step-10 digest comparison verified identical before/after records successfully. All other completed numbered steps returned zero. Two screenshots captured a transient black root after native chooser dismissal; an owned pointer movement triggered repaint and 27 visibly reconfirmed the unchanged pending dirty guard. This is recorded as fixture redraw timing, not an app crash. The portal logged expected unavailable FUSE/audio/RealtimeKit services; actual file chooser operation succeeded.

Scope limit: this native test proves cancellation while the overwrite modal holds the worker before publication. It does not claim deterministic interruption after the worker has already won its publication claim; that distinct race requires controlled synchronization and is covered separately by source tests. No missing-after-claim guarantee is inferred.
