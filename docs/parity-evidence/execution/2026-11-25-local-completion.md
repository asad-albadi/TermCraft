# TermCraft local qualification and remaining work

2026-11-25. Implementation checkpoints: **46/60(76.7%)**. Strict native/original acceptance: **0/60**. The first count measures implemented checkpoints; the second requires the full native/original evidence in the approved plan. Passing Linux subsets do not silently advance either count.

## Completed local work

Current source `32e512ae85fe16dd44d9b775060a0bd1ac70de345ddcf862a221e0f861db74ad`, source commit `7b04c58`, local branch `parity-implementation`. This qualification was captured before the initial GitHub publication. The repository is now published at [asad-albadi/TermCraft](https://github.com/asad-albadi/TermCraft). The first hosted CI run subsequently failed one RDP allocation-cleanup test (81 helper tests passed, 1 failed); see [CI/release issue #3](https://github.com/asad-albadi/TermCraft/issues/3). That new result remains open and does not overwrite the earlier local evidence. The XDMCP reset, Wayland idle-screen disconnect and TigerVNC clipboard compression defects have independently reviewed fixes. Original failures, intermediate artifacts and immutable evidence remain preserved.

The fresh root gate completed0:986passed/0failed/73ignored across71outer targets,750source/test inputs unchanged. Five new clipboard regressions are discovered by the ordinary suite; six optional Wayland engine tests pass separately. Root parent/compiler affinity stayedCPU0; only the recorded owned library/UI test processes moved toCPU2 to avoid contention. No tests were skipped as a speed workaround. Formatting passes.

Representative independent deployed servers now cover ProFTPD six plain/explicit/implicit and active/passive modes; Telnet/Rlogin/Rsh login/command/closure; FreeRDP-shadow NLA certificate/authentication/desktop/pointer/F3/Return; and TigerVNC authentication/pixels/input/bothclipboard directions/server resize. The RDP shadow server has an upstream Unicode TODO and no clipboard/drive channel registration, so those behaviors are not claimed. API/harness evidence and current genuine GUI evidence remain distinguished.

The current genuine full Linux release passes3372archive/extracted hashes and37normal-app startup/sealed-RDP-helper/actualGUI TigerVNC checks. Actual Copy remote clipboard reaches an independent GTK clipboard peer, and actual remote-canvas Ctrl+V reaches the daemon's clipboard. Owned GUI/daemon containers and generated secret/key files were cleaned. The prior125actual Wayland server GUI checks are retained through explicitly unchanged13source pins and exact helper identity;125checks were not replayed on the later client-only change.

Current native Debian12 core/revision3 passed real clean install, revision2warm start, monotonic2→3upgrade, same-version replacement, removal and reinstall. All43commands,1074payload hashes,16before/after bytepairs and5lifecycle inventories pass. Previous132sealed files and baseline artifacts are unchanged. Earlier native GTK/game coverage is retained only through identical UI/game/persistence source; the current build ran actual startup/lifecycle, not a repeated full game matrix.

## Current artifacts

| Artifact | SHA256 | Scope |
|---|---|---|
| Full Linux optional archive | `d8cec853b37018e55c76a837340c86d8cdb406ef4bff9c6008d7ddebd92098f0` | Declared Arch-compatible Linux x86_64 optional native runtime ABI;3372files |
| Debian12 core portable archive | `4628e1c5e58f50fc6bec4f31140a65567138c589b020d7be74abde769f32e8af` | Native Debian12 core;1074files; optional Arch engines/browser/Mosh/Wayland payload absent |
| Debian revision3 package | `9a7c2b80deaa3f062c783fa4a49cd3f926681bf3de31238f608a0b238860f337` | Genuine `0.1.0-3` and actual revision2→3 lifecycle |

Verified copies are retained under `dist/snapshots/<archive-sha>/`; each has a qualification sidecar. Older archives retain their original source and known limitations.

## Remaining implementation checkpoints

| IDs | Required environment/work |
|---|---|
| D02 | New hosted allocation-cleanup test failure needs investigation; native Windows/macOS RDP/device endpoints and printer/smartcard/serial fixtures remain required for broader coverage |
| X01–X04 | Build and qualify reviewed native Windows/macOS engines, authentication/clipboard/display workflows; physical graphics/monitor and original comparisons |
| P01,P08,E01 | Native Windows/macOS client/browser payloads, installers, clean runtime and upgrade workflows; signing identities only if signed releases are chosen |
| E03 | Native Windows/macOS services/capture/control, physical desktop/GPU/consent fixtures and original service measurements |
| E04,E05,V07 | Original Personal26.6 on supported Windows: limits/defaults/edition menus/package behavior and paired workflow captures |
| V04 | Physical serial adapters and real network-adapter transitions; controlled software roaming already has evidence |
| V05 | Native Windows/macOS builds and interactive workflows |

The46implemented checkpoints also retain outstanding native/original comparisons. This table is not a claim that the remaining14need only tests; native engine/runtime/provider completion still requires those environments.

**Platform limitation:** identity-safe opened-object rename/delete and general POSIX COMMSTATUS/properties/modem-event semantics remain unavailable where the OS exposes no equivalent safe API. Adding Linux hardware or moving to another Linux machine does not solve that architecture gap. These operations remain explicitly unsupported, not implemented or verified. Completing equivalent behavior requires an appropriate native platform/API or a separately justified architectural solution.

## Evidence

- [Whole-app gate](../parallel-integration-tigervnc-clipboard-fix/result.json)
- [Current genuine release and GUI proof](../parallel-integration-tigervnc-clipboard-fix-release/gui-qualification/REPORT.md)
- [Explicit Wayland GUI evidence transfer](../parallel-integration-tigervnc-clipboard-fix-release/wayland-gui-evidence-transfer.json)
- [Debian core/revision3 lifecycle](../P08/tigervnc-fix-debian-core-upgrade-32e512/evidence-summary.md)
- [Independent daemons](../V03/independent-protocol-daemons/REPORT.md) and [wording corrections](../V03/independent-protocol-daemons/CORRECTIONS.md)
- [Independent RDP scope](../V03/independent-rdp-server/README.md)
- [Authoritative queue](../../../PARITY_EXECUTION_QUEUE.json)

At the recorded pre-publication qualification checkpoint, the locally reproduced defects and listed audit cells were addressed within these declared scopes. The subsequent hosted allocation-cleanup failure described above is new open work, and D02 investigation is actionable again. Remaining environments and platform limitations stay open; no full-parity, universal-Linux or original-runtime signoff is implied. Final independent artifact/metadata review is recorded alongside the current release packet.
