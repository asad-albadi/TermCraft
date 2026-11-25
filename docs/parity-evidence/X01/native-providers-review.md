# Independent X01 native-provider review — 2026-11-25

Verdict: **accepted as disabled partial source engineering, not native support or X01 completion**. Critical findings: none. Important findings: none identified within this candidate's stated boundary. Parity remains 40/60 (66.7%).

Reviewed against the approved complete-parity plan, X01–X04 requirements and parity contracts. Reviewed the six integration hunks, native artifact/policy/transport/Windows process/Windows and macOS providers, source preparer and upstream patches, and owner evidence. No application implementation was changed during review.

## Independent verification

- All six `evidence/integration-basehashes.json` entries match the current MAIN files exactly.
- `patch --dry-run -p1` from MAIN succeeds for `target/agent-work/x01-native/evidence/integration.patch`, with no rejected or offset hunks.
- All 79 entries in `evidence/owned-files.json` exist in the private candidate and match their SHA-256 values.
- Owner logs record 102 library passes / 5 ignored, seven portable native tests, and successful real Windows/macOS API component checks. These are reviewed owner-run logs, not tests rerun by this reviewer. The reported source preparation and native component checks do not establish native C/Objective-C build/link or runtime behavior.

## Security and integration assessment

The compiled `native_policy::{windows_pin,macos_pin}` error gates execute before authority creation or process spawn. Untrusted folder manifests and stock executables cannot enable either native branch. Linux lifecycle behavior stays on its existing provider. Owned-only absolute Unix display parsing is restricted to the owned lease branch; the System parser retains its prior restrictions.

Windows suspended creation enters a kill-on-close job before resume; failed job assignment terminates the suspended child. The selected IPv4 endpoint is checked against the owned process PID and requires anonymous rejection plus cookie-authenticated X setup. The upstream patch changes listener binding to loopback and exclusive address ownership, and refuses display scanning. macOS launches directly with a minimal environment, private authority/listener/image, authenticated inherited socket handling, and an owned process group retained through waitid(WNOWAIT) before cleanup. Its patch disables stock service fallback, global listener/lock behavior and automatic clipboard proxy, and preserves the explicit Composite limitation. Snapshot hashes are compiled decisions, and source archives are checked before extraction/patching.

The candidate does not overwrite global DISPLAY/Xauthority, expose public TCP to satisfy XDMCP, or advertise tested native capabilities. Native X03 remains unavailable. X04 must integrate through explicit provider modes/capabilities; this candidate provides only the default private authenticated local transport. Both candidates also edit the Windows dependency feature list: integration must union native IpHelper/WinSock with clipboard DataExchange/Memory rather than applying the second Cargo hunk over the first.

## Explicit remaining qualification, not silently excluded

Actual Windows/macOS native build/link, runtime cleanup and ownership checks, private loader/dependency and signing closure, monitor/fullscreen behavior, native graphics/X03, and redistribution qualification remain required before compiled output pins are approved. Native clipboard/XDMCP behavior is not provided by this patch. These are open parity requirements, not passing or not-applicable results. This review authorizes staging the gated engineering only; it does not authorize enabling native support on the strength of compilation.

No broad build, full suite, host display/clipboard/input access, native payload execution, Git operation or active command was left by this review. Root must perform the planned integrated gate after any deliberate shared-file reconciliation.
