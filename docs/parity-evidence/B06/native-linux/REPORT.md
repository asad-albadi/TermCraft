# Native Linux GUI qualification — B06 captured source epoch

This is native X11 GUI code/binary qualification in owned Xvfb inside a clean exported Linux rootfs, with private user/PID/network/IPC namespaces and private HOME/tmp/store. It is not a release-package, physical-desktop, Windows or macOS qualification. No product source edits, instrumentation, Cargo builds, external accounts, user files or host display/input settings were used.

Identity: frozen `target/qualification-native-games/harbor-workspace`, SHA256 `ed7e7946f8c13a3f1e52c1bc4b677ca0995754ce4964b2b4c3cd8d5c8d44eb3d`, 610959304 bytes. The normal binary was copied after root's successful `build-cache-scp/reproduce.log` build, original mtime 2026-11-25T01:20:16.387522Z. `frozen-binary.json` links the exact 691-file source capture (capture SHA256 `9280efb23cf885b96fbb84be790bc7d6773aaed8a4457c8650f3894d658c9608`). Subsequent Windows serial source changes are outside this captured epoch; `source-epoch-check.json` records five changed captured paths, and root separately added `engines/rdp/src/serial_windows_control.rs`. This run makes no whole-current-MAIN claim.

## Observed GUI coverage

`001-menu.png` shows all 13 source catalog entries and idle disabled by default. `matrix.json` pairs actual native boards/actions. Raw exact input commands and their output are `action-NNN.sh/.log`; full root screenshots are retained. Source-based coordinates were visually checked; actions are real X11 input to the one test-owned app.

- Soliterm: Column 1 bottom 3♥ moved to Reserve 1 (`003`), Undo restored it (`004`). The fast Redo attempt in `005` did not establish a return transfer, so it is not used as Redo proof.
- Termwalk: tile rotation changed the board (`008/009`). Ctris: move/rotation/drop advanced Next 0→1 and locked the first piece (`010/011`).
- Nimesweeper: first reveal shows 1 (`014/015`). Ninvaders: Start/Fire/Right removed Press Start and changed state (`016/017`). Greed: Right erased 7 squares (`018/019`).
- Netwalk: first tile rotation and unsaved state (`020/021`). Mines: flag count 0→1 (`022/023`). Solo: editable digit and visible conflict marking (`024/025`). Guess: first peg 0→1 (`041/042`). Untangle: actual vertex drag (68,516)→(110,540) (`052/053`).
- Advanced Solo: first Advanced request at seed 2 failed boundedly; advancing via Trivial seed 2 then requesting Advanced produced actual Advanced seed 3 (`059`), and confirmed Solve produced Won/Assisted (`061`).
- Jigsaw: native New seed 2 produced irregular regions (`032`); confirmed Solve→Won/Assisted (`033`), Undo→Playing (`034`), Redo→Won (`035`); dirty Reset dialog and reset board (`036/037`).
- Killer: proper no-symmetry/Trivial settings produced seed 3 with visible sum cages (`039`), confirmed Solve→Won/Assisted (`040`).
- Map: default Normal board (`043`); 75 regions/Unreasonable seed 2 generated (`062`), painting a blank region changed it orange (`063`), confirmed Solve produced Won/Assisted (`064`). A separate 30-region/Unreasonable seed-2 attempt failed boundedly with current solved Normal seed 1 kept (`050/051`).
- Dirty Escape close offered Discard/Keep playing (`006`), and Keep playing preserved the game (`007`). Solve confirmation (`026`) was canceled before other actions. New and Reset dirty guards were actually traversed.

## Actionable finding: New retries the same failed seed

`action-028.sh` requests Advanced Solo three times, two seconds apart. `056/057/058` each show the same clean Trivial seed-1 board, Advanced draft, 3×3 boxes, half-turn symmetry, X/Jigsaw/Killer off, and bounded generation-failure message. `perform_file_action(New)` derives the next seed from the retained current session. Since a failure preserves that session, New repeatedly requests seed 2; the UI's suggestion to try another seed has no direct working path through repeated New. The successful seed-3 followup proves this is distinct from total Advanced unavailability. Root was notified; this immutable run does not include a later fix.

## Save/Load scope

Actual Save click on the dragged Untangle board (`054`) and Load click on a clean Solo board (`060`) returned without any native chooser appearing in this portal-free namespace. No game-file Save/Load roundtrip or overwrite consent is claimed here. The explicit native probes are bounded, no test/user save file was selected, and independent codec/UI tests are separate evidence.

## Harness lifecycle and idle attempts

The first long-lived execution session ended with status 143 during action 038, after `079` and before `080`, without the script's cleanup marker. Host inventory showed no remaining owned Xvfb/app/bwrap. The application log had no failure message. This is recorded as an interrupted harness, not an application-crash claim. The remaining focused native checks use a new private namespace and fresh process with the same frozen executable; `session2.sh` starts at action 039 and preserves the first run's artifacts.

The first numeric-field configuration attempt left 300 seconds selected, so `070/071/072` and its `62787921` raw input are not idle proof. Dragging the slider visibly set 30 seconds (`073/077`). An ensuing 32-second no-input interval (`074`) still showed the terminal; x reached the owned PTY. This raised a native idle concern; a fresh-process repeat is recorded below after completion. The raw PTY script only captures test-owned input, and every attempted capture/response is retained without claiming a quarantined wake until an idle overlay is actually observed.

## Completed focused followups

Fresh-process idle succeeded: `083` visibly has the checked 30-second setting; no input for 34 seconds produced the actual logo/“Press a key or click to return” idle screen in `084`. The actual wake key x dismissed it (`085`) and was absent from the raw PTY capture. The hidden terminal no longer owned keyboard focus after the menu interaction: the immediate y! attempt also did not reach it. Explicitly refocusing the terminal and sending y! yielded exactly bytes `79 21`, with the same owned Python capture returning `B06_OWNED_PTY_RESUMED` (`086`). Thus the wake key did not leak and the owned PTY survived; this is not a claim about automatic restoration of keyboard focus. `pty-input.bin` is the final successful focused followup; the two earlier `62787921` files are retained with explicit prior-attempt names. The earlier post-13-game nonactivation observation is not erased; it is sequence-specific and has not been isolated into a confirmed product defect.

Sudoku valid followup: row 1 column 3 accepted 8, with unsaved/Undo enabled (`087`). The prior rejected 1 in `013` is conflict-rejection evidence rather than a successful move.

Actual native root-window close used a real X11 `WM_PROTOCOLS / WM_DELETE_WINDOW` client message to owned window 2097156 (`native-close.py`), not a product UI shortcut or XDestroyWindow. The unsaved Sudoku guard appeared (`088`); Keep playing retained the 8 (`089`). A second native close plus Discard advanced to the live-session confirmation (`090`). Explicit Close sessions ended the owned app, asserted by `kill -0` failing in `action-043.log`; `091` is the final cleared Xvfb root. The namespace then exited 0 and `cleanup-native2.txt` was written after killing/reaping Xvfb and the owned app. A subsequent host process inventory confirms no process with this qualification namespace, Xvfb or frozen executable remains.

The only confirmed actionable product finding in this run is the repeated failed-seed New trap. Missing portal chooser support limits native Save/Load coverage. No blanket all-presets, all-platforms, package or release-completion claim follows from this run.
