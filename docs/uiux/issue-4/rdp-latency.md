# RDP frame flow and review performance

The owner confirmed that a computer-qualified local account with an empty Domain connects. They then reported severe lag and repeated reconnecting. This follow-up fixes measured client bottlenecks; acceptance on that Windows session remains open. Do not merge PR #8.

## Measured causes and resulting behavior

The prior review replaced an optimized packaged helper with a development build. In addition, even a 64 × 64 bitmap update on a 3840 × 2160 desktop copied and synchronously wrote the entire 33,177,612-byte framebuffer on the same loop that handles RDP input. A stopped or slow frame consumer therefore delayed keyboard input. The GUI also rebuilt and uploaded the whole texture for each small update.

The review now uses an optimized release helper. A separate bounded writer preserves control/frame order and allows only one frame queued or in flight. While it is busy, the decoder retains the union of dirty rectangles on its current canvas instead of enqueueing more images. Revisions advance only after a successful enqueue. Opaque alpha handling and pixel-buffer cleanup run in the writer, and shutdown remains bounded when its pipe is full.

Private wire protocol **2** sends a full baseline at connection/reactivation and bounded region packets afterward. The receiver validates geometry, byte lengths, base canvas and consecutive wire revisions. It maintains one complete canvas and coalesces damage before UI polling. Snapshot-only callers still get complete frames; the GUI opts into region updates and uploads only the affected texture region. A replacement full frame stays pending across later patches until its complete texture is uploaded, including hidden-tab and resize cases.

The GUI/helper must be refreshed together. Release staging, startup and delivery checks now expect protocol 2. Earlier protocol-1 evidence remains historical. The release test fixture also includes the Nerd Font assets already required by current packaging.

## Controlled loopback measurements

A single controlled fixture run collected 25 steady frame deliveries from 64 × 64 bitmap changes on a 4K canvas, after the initial full frame, with the same server and host:

| Helper | Elapsed | Helper CPU | Pipe bytes for updates |
| --- | ---: | ---: | ---: |
| Previous development review helper | 15,895 ms | 15,500 ms | 829,440,425 |
| Previous helper, optimized | 1,212 ms | 760 ms | 829,440,425 |
| New helper, optimized | 845 ms | 30 ms | 410,225 |

A separate controlled test held the initial output pipe completely full (65,536 / 65,536 bytes). The previous optimized helper did not deliver the key during 500 ms of backpressure; it arrived at 563 ms, after draining. The new helper delivered it before draining, at 2.5 ms. Closing the helper with a saturated pipe completed within its bounded final-message timeout (about two seconds).

These are helper/loopback measurements, not WAN, interactive GUI latency, video playback or native Windows acceptance benchmarks. The fixture uses classic RDP bitmap updates.

## Verification and remaining checks

- [x] 104 helper tests, including blocked output, control/frame ordering, damage serialization and opaque pixels.
- [x] 19 parent transport/model checks, including frame coalescing, invalid geometry, missing bases and revision gaps; existing VNC security/input tests remain passing.
- [x] 13 UI/input/ownership checks, including actual typed/pasted sign-in, cropped texture uploads, full-before-patch invalidation and a stationary pane that does not repeatedly request display changes.
- [x] 8 independent RDP/helper checks, including TLS sign-in, certificates, shutdown, resizing and input ordering.
- [x] 23 release delivery contract checks with protocol-2 metadata.
- [x] Actual endpoint observed all 80 keyboard transitions in order across a resize, followed by 69 consecutive pixel-verified region updates.
- [ ] Owner confirms responsiveness and whether the reported repeated reconnects persist on the real Windows endpoint.
- [ ] Full owner human acceptance and explicit merge approval.

The review keeps the verified Chromium payload beside the current GUI and optimized RDP helper. Component/source identities and profile are recorded in [the review manifest](rdp-latency-review-build.json). Close the older application before using the same `target/issue-4/run-terminal-row-review` launcher. Keep inherited pane sizing enabled for the first retest; fixed resolution remains available for isolating any remaining display-renegotiation issue.
