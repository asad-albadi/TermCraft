# E03 VNC independent composition review

2026-11-25. **Approve the private composition for ROOT integration and subsequent ROOT gates. No Critical or Important composition findings.** This is partial E03 evidence only. All three feature lanes remain private until ROOT integrates and verifies them. Progress remains **42/60 (70.0%)**, strict native/original qualification **0**.

## Exact scope and identity

Reviewed only the composition of the previously accepted Wayland, macOS FIX1 and browser VNC lanes, shared hooks, payload/replay, and the parent-authorized Source-error cancellation correction. The accepted reviews are `vnc-wayland-review.md`, `vnc-macos-review-fixes.md`, and `vnc-browser-review.md` in this directory. Unchanged vendor/backend implementations were checked for preservation, not reopened as new reviews.

Candidate handoff: `target/agent-work/e03-vnc-integrated/handoff/`.

- `integration.patch` SHA-256: `f544bed7c276ce7ad7ff6986cc4b1b507fcd0fa6352f1d93bd094fd7b67203ae`.
- `integration-new-files.tar.gz` SHA-256: `a6a450729d08e4d5b7cf7a4796eabde994995b449af67cea1279d4b34d00eef5`.
- ROOT owned-baseline anchor: `00f2c5743d2b0fa6066f7ad03c29e347e7cc2147bdd7eb6aed83d0e9d12626e4`; independently checked each current owned ROOT base hash/absence against the recorded baseline. No Git identity is implied.

## Independent integrity and replay

All 138 normalized candidate file hashes match the supplied `files/` bytes. Every frozen patch/candidate/review/provenance entry in `approved-input-proofs.json` and every entry in `handoff-sha256.json` matches its actual bytes. Independently compared the preservation lists to the accepted candidate manifests: 19 Wayland entries, all 22 Mac new files (including exact no-terminal-newline `.cargo_vcs_info.json`), and 87 browser entries including CREDITS are unchanged.

Validated all 127 tar members before extraction: unique canonical relative paths, no traversal, regular files only, no links/special files, every name a declared new owned file. Modes are 0644 except `tests/support/run_wayland_portal_isolated.sh` at 0755. A temporary scratch replay copied actual ROOT existing inputs, applied the 11-file patch with `patch --batch --fuzz=0 -p1`, extracted the validated payload, and matched all 138 final SHA-256 values. This includes the binary upstream noVNC PDF and repaired exact Mac JSON; the text-only patch is not a substitute for the payload.

Independently reversed each exact composition comparison with zero fuzz and verified the resulting shared files against accepted candidate hashes or ROOT for untouched files: Wayland 10 comparisons, Mac 11, browser 10. Mac shared base hooks absent from its new-file manifest were reconstructed from its frozen original patch. The documented malformed new-file metadata patch boundary does not affect the final exact payload. No frozen handoff was edited.

## Composition findings

**Critical: none. Important: none.**

- The single `open_source` dispatch retains actual platform cfgs and the cancellable Arc callback. The supervisor supplies the same Context cancellation used for startup/Stop; Mac receives its expected callback, Linux retains Wayland and explicit X11. Original X11/Windows provider implementations and RFB authentication/protocol behavior are preserved. Browser sources and the new controlled source remain test-only.
- The browser optional bind/default/consent checks coexist with cancellable source setup. Capture and validation precede listener publication; HTTP bind failure drops the owned RFB listener/source. The browser connects to the captured same-instance upstream, status exposes its actual address, and serving joins both capture and gateway ownership on exit. No duplicate uncancellable production bind route was introduced.
- UI platform source choices coexist with browser enable/address/URL controls. Control, clipboard and browser remain off by default; Wayland suppresses clipboard and never infers DISPLAY. Mac display ID, primary-at-Start behavior and permission/global-clipboard wording survive. The same backend policy applies to browser viewers, including view-only restrictions. Session persistence/defaults did not change.
- Packager argument forwarding and existing app/RDP/Xwayland/Xephyr/Mosh/Iperf/Unix/Chromium inputs remain intact. The source freeze now includes both optional engine source/lock/example and viewer/vendor inputs. Helper source/notices/hash/version/ABI fields coexist with offline browser source/notices. Optional helper selection remains explicit; omission does not force helper building/loading. The backward-compatible optional manifest field is retained. The main lock adds the approved Mac packages/pin and has no PipeWire package; ordinary application and GUI test ELF dependency evidence excludes PipeWire, while the separate helper owns that ABI.
- Ordinary unsupported-source tests no longer invoke their matching native OS provider. Wayland clipboard is rejected before opening a portal; browser configuration-only validation does not request permissions. No silent loss of an accepted lane feature was found in the shared diffs.

## New cancellation correction

`source_failed` checks actual `Context.cancelled()`, rather than treating `Interrupted` as success. Capture, input, clipboard write/poll and owned-input-release error branches use that same predicate and break through normal final Source Release cleanup. An already observed intentional Stop therefore avoids recording a spurious failure. An uncancelled error records failure and requests cancellation; an error that wins before cancellation is observed may still be Failed, as explicitly documented. The final best-effort cleanup release behavior is retained. This does not mask authentication/protocol failures or change RDP.

Reviewed the controlled fixture: startup returns owned pixels, then an explicit gate holds capture or clipboard poll until the real supervisor Stop is recorded. It checks Stopped/no error, Release, and closure of both listener ports. The same Interrupted failures without Stop must be Failed. This tests lifecycle ownership and actual status rather than only the predicate.

Independently ran the existing combined test executable, with no compilation:

`target/debug/deps/harbor_workspace-783ffab7aa0a9efd vnc --test-threads=2`

Executable SHA-256: `192306e9beae50368060f496bea26dc7d3f71f69c4f834f3e9e9399988c89251`, matching the handoff ELF audit. Result: **34 passed, 0 failed, 4 explicit isolated cases ignored**. Both new lifecycle tests passed; browser bind rollback/defaults, view-only/clipboard/protocol, X11 overflow validation and optional helper IPC/verifier gates passed. No native display/input/clipboard, permission dialogs, or ignored native fixture was used. All review commands completed and temporary scratch trees were removed.

## Evidence limits and ROOT follow-through

Inspected the hash-verified owner integration evidence: UI 1 pass, packaging 4 passes, private portal/real PipeWire frame fixture 1 pass with daemon exit 0, actual Chromium synthetic-source modes 2, Mac pure helpers 11 passes and actual aarch64 API component check. These are owner results, not independently rerun native qualifications here. They precede the final Source-only guard where stated; the final combined lifecycle gate covers that correction.

ROOT still owns final integrated full app/GUI/helper/release gates and artifact dependency inspection. Native macOS/Windows execution, final Mach-O and older-OS startup, GPU/distribution coverage, real compositor permission/control and browser end-to-end sessions, Wayland clipboard, public network/browser matrix, HTTPS and original-application equivalence remain open exactly as in the accepted lanes. This approval neither closes E03 nor increments completion/native qualification counts.
