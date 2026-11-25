# Game save cancellation before publication

The original 966 candidate and its manifest passed checksum verification before this separate private source copy was created. The scoped update changes save.rs. Cargo version 0.1.1966 and the private binary name avoid reusing the previous candidate binary.

Each worker has a fresh atomic publication claim. Invalidating or dropping a job sets that claim. After returning from the chooser and receiving overwrite approval, the worker checks for cancellation. Immediately after staging, sync and final target validation, the worker atomically competes for the claim before executing rename or hard-link. If cancellation won, the worker removes the staging file without publishing. If the worker won, the rename/link operation is committed to proceed and may finish after the GUI invalidation. A later close cannot roll it back. The GUI takes no filesystem lock and waits for no worker. The existing bounded job remains busy until the worker finishes, including when waiting for an uncancelable native chooser. Stale results and questions are discarded, and unanswered prompts release the worker on cancellation or drop.

The two synchronized regressions failed on 966 behavior (red-save.rs contains only the test seam, plumbing and tests). In one, invalidation during the chooser preceded release with a nonexistent destination. In the other, the worker passed positive overwrite consent and final validation, then reached a barrier directly before publication. Both tests awaited worker completion and failed on changed destination bytes or unexpected file creation. Both now pass and additionally check staging cleanup, no remaining prompt/job, retained dirty state and stale notification suppression. The unchanged editor tests cover native-close invalidation and actual egui decisions. Native GTK verification of this build belongs to the separate qualification run.

fix1.patch reconstructs the final two-file payload from the frozen 966 payload. cumulative.patch reconstructs it from the original MAIN baseline in ../base. Both reconstructions are byte-identical. All 707 inputs are mapped. Against 966 the two private Cargo identity files and src/ui/games/save.rs alone differ. The core, codec and compatibility identity remain byte-identical.

Commands ran from source/ with CARGO_TARGET_DIR=<app>/target, CARGO_BUILD_JOBS=2 and RUST_MIN_STACK=67108864.

* cargo test --offline --locked --bin termcraft-game-overwrite1966 invalidated_worker -- --test-threads=2 (red.log and green.log)
* cargo test --offline --locked --bin termcraft-game-overwrite1966 save_ui -- --test-threads=2 (focused.log)
* rustfmt --edition 2024 --check src/ui/games/save.rs src/ui/games/play.rs
* cargo --config 'profile.dev.package.harbor-workspace.codegen-units=1' build --offline --locked --bin termcraft-game-overwrite1966 (normal.log). The conditional test hook changes the publication structure, so this verifies the normal build and supplies the exact executable for the native qualifier.

Results: 2/2 cancellation tests failed at the expected destination assertions before the change, then passed on FIX1. The 13 covering game save UI/file tests also passed. Formatting passed. The first normal build with the 64 MiB compiler stack received SIGSEGV in the compiler while compiling the unchanged library. On the compiler's suggested 128 MiB stack setting, the same command passed with three existing warnings. normal-retry.log and binary.sha256 record the final normal build.

check-baseline.rs independently loaded the genuine archived chooser run's saved.termcraft-game using this normal build’s library, asserted Untangle seed 1 and reproduced all 331 bytes exactly (compat.log). This is a codec compatibility check. The application’s new native chooser/modal interaction still requires the separate native qualification.
