# Exact-original static games recovery

Recovered from the user-supplied MobaXterm Personal 26.6.0.5560 executable, SHA-256 `e9a66144b3a1c4d32f8062b1eb34e26e9ed5298106abf1982b4654ab107f13a5`. This is **RECOVERED static evidence**, never MEASURED original behavior. No Wine, original executable, extracted game, GUI, network service, or child fixture was launched. No product code was changed and no point-completion/native count advances.

## Actual declared menu

`catalog.json` retains the exact binary DFM subtree and all object/property offsets. All 64 actual DFM streams parsed successfully with no failures (`parse-summary.json`); they are raw, uncompressed VCL resources. Main `FormXterm` resource signature is at `0xfb38ec`; menu parent path is `FormXterm/MainMenu3/Games1`, raw object `0xfc364c`. Its direct ordered children:

1. Terminal games — explicit disabled heading (sibling, not parent).
2. Soliterm, Termwalk, Ctris, Sudoku, Nimesweeper, Ninvaders, Greed — each has lowercase command `Hint` and `OnClick=ExecuterAppli`.
3. Typespeed — same handler linkage but **explicit `Visible=false`** at `0xfc3996`.
4. Netwalk (`net`), Mines (`mines`), Solo (`solo`), Guess (`guess`), Map (`map`), Untangle (`untangle`) — `OnClick=ExecuterSTGame`.
5. Tux consolesaver — `OnClick=Pingusconsolesaver1Click`, object `0xfc3bb6`.
6. Separator, then Remove games from MobaXterm — `RemovegamesfromMobaXterm1Click`.

The unhidden 13 game declarations are stronger catalog evidence than candidate archive filenames. An absent Visible/Enabled field is only absent in serialized data: runtime initialization can still change it. The actual expanded runtime menu remains unobserved. Toolbar `BtnGames` at `0xfbb10d` has Hint `Start a terminal game`, handler `DropperLeMenu`; `SheetGames` popup sheet exists, but exact dynamic toolbar-to-menu transfer was not traced.

## Concrete dispatch linkage

The published-method records bind `ExecuterSTGame` to VA `0x9e68c0` (name raw `0x54b1db`), `ExecuterAppli` to VA `0x99c0f4` (name raw `0x5492cb`), and `Pingusconsolesaver1Click` to VA `0x9c4434` (name raw `0x54996d`). CODE raw offset is VA minus `0x400c00`.

ST handler disassembly takes sender properties at `[sender+0x58]` or `[sender+0x80]` after class checks (`0x9e690f–0x9e6947`), resolves `/bin/stgames.exe` using literal VA `0x9e69e4` (raw `0x5e5de4`), concatenates quoted path plus `stgames.exe" ` and sender string (`0x9e694c–0x9e697b`), and passes the result to an imported call through `0x407d4c`. The precise WinAPI import name was not assigned; dispatch command assembly is recovered, successful process launch is not measured.

## Controls and rules recovered from actual companion bytes

`controls.json` contains per-game decoded SHA-256 and source string offsets for seven linked terminal games. These are embedded help declarations, not captured interaction:

- Greed: erase N squares where N is the first number in the chosen direction; cannot cross erased squares or leave grid; no valid move ends game. `hjklyubn`/keypad movement, `Ctrl-L` redraw, `p` highlight valid moves, `q` quit, `?` help.
- Sudoku: row/column/3x3 contain 1–9. Digits place, `0`/`.` erase, `c` clear board, `f` fix squares, `n` new, `q` quit, `s` save, `r` restart, `u` undo, `v` solve, `?` hint. Vertical cursor labels are split by control bytes and deliberately not reconstructed.
- Soliterm: mouse source then destination, arrows and Space; Freecell/Klondike options and passes count. Exact legal-move implementation/default variant not established.
- Termwalk: connect every server to central router, mouse/keyboard and odd grid size. Exact key map unresolved.
- Nimesweeper: arrow move, `f` flag, Space reveal, `p` pause, `h`/F1 help, `t`/F2 times, `c`/F3 difficulty, `r`/F5 restart, `q`/Escape quit. `%c` arguments were recovered from static printf immediates, detailed in JSON. Four exact help-declared grid/mine presets retained there.
- Ctris: embedded v0.42 (Jul 25 2007), old-style and counterclockwise options; `p` resumes pause. Exact full key map/rules unresolved.
- nInvaders: Space starts, skill options retained; full controls/rules unresolved.

Companion ZIP member contents are LZMA-alone payloads. Python standard-library LZMA decoding produced valid PE files and exact decoded sizes; nothing extracted was executed. The accompanying bashrc only normalizes SHELL to /bin/bash.exe, exports FIRSTTIME=true and sources /etc/profile when needed; it contains no game launch rules.

## Embedded puzzle payload

Actual original EXEFILE/MOBASE resource at raw `0x89fb00`, 2,555,942 bytes, decodes to a 15,222,391-byte ZIP. Its stored `bin/stgames.exe` is 292,864 bytes and SHA-256 `8a3239672bb359934b42696f383e13bcf35859e646d89bd20a7d4bd319b000fe`. It identifies `Version 20150612.f306b9d` at member raw `0x40a24` and Simon Tatham's Portable Puzzle Collection at `0x40f4c`; six selectors correspond to the actual DFM hints. **ZIP CRC anomaly:** declared CRC32 `0x46c80442` differs from recovered bytes CRC32 `0x02c5f575`; 21 of all 56 MOBASE members have mismatches. Cause is unestablished. No checksum-clean/runtime-valid claim is made, and this member must not be executed based on this report. Historical upstream source correspondence is a research lead only: primary exact-revision manual request returned HTTP403. No six-puzzle rule fixtures were invented.

`resource-decoding.json`, `MOBASE.catalog.json` and `sources.json` record transformations, byte hashes, SDK sources and all member integrity results. Python/liblzma cannot decode MOBASE's `lc=6` property; an official 7zip 25.01 SDK decoder was compiled from primary upstream sources and decoded under 768MiB address-space, 30CPU-second and 256MiB file limits, yielding result=0, exact declared length, MAYBE_FINISHED_WITHOUT_MARK. SDK wrapper and sources retained. An existing 7zz helper was first verified byte-identical against a fresh official HTTPS archive; its automatic LZMA recognizer refused both MOBASE and XW, and was not used to decode either. XW decoded with explicit Python FORMAT_ALONE and contains no additional matching game entries.

## Screensaver evidence

`PanPingus` raw `0xfb4c81` is black, initially hidden, with click/double-click `Panel1Click`. `TimPingus` raw `0xfc5f3a` is disabled, serialized Interval 120; `TimLaunchPingus` raw `0xfc5f88` disabled, Interval 3600. These are serialized timer fields, **not confirmed inactivity delays or frame timing**. The manual handler at VA `0x9c4434` has a split-mode warning and calls code at `0x9c2ce0` after setting another component's interval to 8000; semantics beyond the recovered flow remain unassigned.

Decoded original TXTRES/HtmlHelp2.txt lines 608–624 explicitly describe Personal games/screensaver and Pro customization to remove them; Pro can also retain/re-enable them. Source embedded text is preserved with hashes. It does not prove runtime-exposed entries or effective Personal disable behavior. Animation, exit behavior and inactivity trigger are unresolved.

## Files and cleanup

All private work is under `target/agent-work/original-static-recovery`. Only `docs/parity-evidence/original-static-recovery` contains the compact ready evidence snapshot. No long-running tool session was created; all compiler/decoder/network-read subprocess calls finished synchronously. No owned persistent process remains.
