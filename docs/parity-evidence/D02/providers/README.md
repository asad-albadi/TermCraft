# D02 drive/admin increment — 2026-11-24

This increment is **not D02 completion**. Counts remain 25/60 implementation checkpoints and 0/60 full parity. The earlier successful user-run `../host-*.log` files were read and are preserved. No further restricted-runner listener investigation was performed.

## Changed behavior

The embedded worker attaches an `rdpdr` channel only when explicit folder mappings exist. New mappings are read only. Capability directory handles confine filesystem access; directory enumeration streams one result per request. Regular-file creation/read/write/resize, metadata, directory creation and Unix filesystem capacity queries are implemented. Device I/O uses a dedicated worker and bounded queues; saturation produces a device-busy completion instead of disconnecting the desktop. Unknown controls fail explicitly. Handles enforce data access and remote sharing and remain bound to the opened object after local renames. FIFOs, device files, symlink entries, parent traversal, alternate data streams and oversized requests are refused.

The settings editor supports adding/removing folders, selecting their paths, changing their 1–7 character protocol names and read-only policy, and requesting the administrative session. Engine switching retains visible controls for clearing incompatible saved options. Administrative negotiation requests session zero through GCC cluster data without advertising automatic destination redirection; TLS verification/NLA remain enabled.

## Validation

- `final-engine.log`: **15 passed / 0 failed** in the separate helper package, using actual temporary files and serialized protocol requests. No listeners required. Includes existing clipboard tests.
- `options-green.log`: **9 passed / 0 failed / 1 filtered**. The filter is explicitly the listener-based clipboard endpoint case. It is preserved, not weakened, and remains in the unfiltered host commands.
- `ui-green.log`: **3 passed / 0 failed**, actual egui click frames changing folder/admin choices and clearing incompatible settings.
- `build.log`, `helper-build.log`: both locked offline builds pass. `test-compile.log`: the full root test suite compiles with `--no-run`; this is not a current full-suite runtime pass.
- Root/helper `cargo fmt --check` and host-script `bash -n` pass.
- `rebuild-check.txt`: build and 942×1005 startup rendering pass. `startup.png` is the captured startup image. Original mechanics and runtime/appearance comparisons remain unverified.
- RED evidence is retained in `engine-red.log`, `handshake-red.log`, `options-red.log`, `ui-red.log`, `names-red.log`, `directory-red.log`, `directory-late-red.log`, `volume-red.log`, `review-red.log`. Initial missing-module/API errors are distinguished from behavioral assertion failures. The initial UI harness texture-delta failure was corrected before recording its missing-control RED.

The review identified access/share-policy errors, allocation extending EOF, stale-path deletion/rename, queue-overflow disconnects, Unicode/create-response wire mismatches and hidden incompatible settings. These were fixed or conservatively refused, with passing behavioral coverage. No second reviewer loop was used.

## Rulings and remaining requirements

- **Ruling:** refuse rename, delete/disposition, delete-on-close and allocation operations explicitly. POSIX pathname mutation cannot safely target an opened object's identity after its name is rebound. Refuse delete-on-close before any truncation/create side effect; reject allocation rather than change logical EOF. Cost: remote deletion/rename and save-through-temporary-rename applications fail. Full writable-drive parity remains open.
- **Ruling:** reject unsupported file types and symlink entries instead of opening arbitrary streams or following platform-specific links. Cost: linked entries are unavailable over the mapped drive.
- **Ruling:** keep D02 open and do not advance to D03. Printer, serial and smartcard embedded providers are still absent; rejection and installed mstsc profile options do not implement them. Administrative byte encoding is not proof of modern Windows session behavior. Windows volume-size queries, Windows/macOS/hardware endpoints and original behavior remain unverified. No scope is removed from the approved checklist.
- Native RDP drive negotiation/copying, the current full root suite and connection UI require **new host validation of this increment**; the prior 390 raw host passes predate it. From `app/`, run `bash docs/parity-evidence/D02/providers/run-host-validation.sh`. It records source hashes and unfiltered engine/options/session/full logs separately, preserving the previous host evidence. This script has been syntax-checked, not run on the host by this agent. Craft Anything's build-command route returned `MCP tool call requires approval, but approval policy is never`; local builds and `rebuild_check` were available. No permission bypass or new sandbox diagnosis was attempted.

## Protocol sources

- [MS-RDPEFS device I/O](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpefs/a087ffa8-d0d5-4874-ac7b-0494f63e2d5d)
- [Drive device announcement](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpefs/d8b2bc1c-0207-4c15-abe3-62eaa2afcaf1)
- [Create response](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpefs/99e5fca5-b37a-41e4-bc69-8d7da7860f76)
- [Set information](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpefs/b5d3104b-0e42-4cf8-9059-e9fe86615e5c)
- [Query directory](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpefs/458019d2-5d5a-4fd4-92ef-8c05f8d7acb1)
- [Client cluster data](https://learn.microsoft.com/en-us/openspecs/windows_protocols/ms-rdpbcgr/d68c629f-36a1-4a40-afd0-8b3e56d29aac)

The provider is independently implemented using the already locked IronRDP static-channel interfaces and capability filesystem dependencies; it does not copy upstream RDPDR provider code.
