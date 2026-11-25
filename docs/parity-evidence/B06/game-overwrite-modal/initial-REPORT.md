# Game overwrite confirmation

The two-file payload replaces the game save worker’s external Yes/No dialog with an egui modal. The native file chooser still selects the destination. The worker opens and snapshots an existing target before requesting consent and keeps the handle, directory, bounded contents and metadata until publication or cancellation. Both post-consent checks and new-file no-clobber publication are unchanged.

A generation-tagged question travels over the bounded file-job channel. The GUI answers on a capacity-one channel without blocking. A discarded question or application teardown disconnects the waiting worker. Keep existing file, Escape and backdrop dismissal decline replacement. During the prompt, game controls, board shortcuts and timed ticks are suspended. Root and detached native close invalidate the question through the existing close paths.

The reproduction in ../native-game-file-dialogs/REPORT.md used the packaged b5893bf7 baseline. Its genuine GTK portal accepted the existing file twice in an environment without Zenity. No second confirmation appeared and the file was preserved. baseline-red.json identifies that report and its evidence. No new native portal run is claimed here.

The focused precursor run passed 13 tests. The final ui::games run passed all 64 tests, including visible egui button interaction for Replace and Keep existing file, Escape, root close while consent is pending, target replacement, and an edit with the original inode, length and restored modification time. The chooser was injected for these widget tests. Save file read/write and game history checks used real private temporary files. The backend’s existing portable rename has a residual interval between final validation and rename.

source.patch reconstructs payload from base. The maps cover 707 private build inputs and their original equivalents, including engine, resources, vendor sources and tests. The private Cargo.toml and Cargo.lock select version 0.1.966 and the dedicated executable name. Only the two game UI files belong to the integration payload. Main sources were untouched.

The local commands ran in source/ with the shared app/target directory, two Cargo jobs, locked offline dependencies and a 64 MiB Rust compiler thread stack. games.log records the final test run. normal.log records a rustc SIGSEGV on the first ordinary build. normal-codegen1.log records the retry using one codegen unit for the harbor-workspace package alone.
The normal build completed successfully with the package-specific codegen setting. binary.sha256 pins the resulting executable.
