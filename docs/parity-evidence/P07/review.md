# P07 independent review

Progress at review was 50.0% (30/60 checkpoints). The gate needs corrections.

## Important findings

- **Bind capture output to the opened file and destination directory.** At `src/tools/capture.rs:303`, the provider independently opens a pathname for writing after the application creates a private file. At `src/tools/capture.rs:379`, publication resolves the source and destination by pathname again. A 0700 directory protects its contents, but a writer with rename permission on its parent can replace the directory entry. Canonicalizing the destination parent once also leaves its identity unbound. Capture data could be written outside the selected directory, or a different file published. Write to the originally opened file, use anchored no-replace publication and identity-aware cleanup, and test directory replacement using local file fixtures.
- **Reject redirected standalone inputs on Windows at open time.** At `src/tools/mod.rs:26`, ordinary Windows file opening follows a replacement reparse point between the preceding metadata check and open. Checking metadata on the resulting handle identifies the target. File comparison and image preview can therefore read a different file. Open the leaf as a reparse point and reject redirected handles before reading, as the recursive comparison already does.
## Minor API difference

- **Supply the specified public capture entry point.** At `src/tools/capture.rs:122` the implementation exposes only `CaptureConfig::start`. A caller using the documented P07 interface `PacketCapture::start(config)` cannot compile. Add the entry point, retain existing callers and exercise it from an integration test.

## Verified scope

The capture worker uses provider-owned handles with scoped release, nonblocking reads, a short idle wait, checked packet sizes and bounded classic PCAP output. The recorded Linux namespace runs exercised selected traffic, filter errors, truncation, idle and partial cancellation, byte/time/packet limits, permission denial and destination collision. Two ordinary parser/configuration tests also passed. The namespace runs were reviewed from P07/parallel-capture.log. This review opened no interfaces.

Recursive folder comparison retains the root handle and opens each descendant component without following links. The hardening log records both directory replacement regressions, 53 library tests and 13 tools tests passing. Folder comparison checks cancellation during traversal and between bounded file reads at src/tools/diff.rs:274. An individual read of up to 8 MiB uses read_to_end without checking cancellation. The scanner parses a literal IP and validates wildcard, multicast and limited broadcast addresses, including mapped IPv4 forms. The image loader checks encoded input, dimensions and pixel count before pixel decoding. The UI dispatches scan and capture work to workers and polls completion.

## Native and release evidence

Windows evidence covers cross-compilation of the capture component. Runtime Npcap loading, ACL inheritance, savefile behavior and cleanup on Windows remain unverified. macOS libpcap activation, permissions, ABI and capture output also need native evidence. The two reviewed logs establish scoped Linux results. Whole-suite and formatting results belong to the integrating checkpoint.

## Capture and standalone input corrections

The capture worker serializes classic PCAP through the originally opened staging file and reads metadata back from that same file. On Linux, publication links the held file descriptor into the held destination directory with exclusive creation. macOS uses an exclusive rename relative to the held staging and destination directories after checking the source identity. Windows creates each private staging entry with an owner-only ACL and an anchored handle in the same call, then renames the held file relative to the held destination directory. A destination collision or detected directory replacement preserves the validated capture and reports its recovery path when available. Cleanup checks identities on Unix and operates on held handles on Windows.

Windows standalone file comparison and image preview open the leaf reparse point itself and reject it using the resulting handle's attributes. The capture controls and integration test use the public `PacketCapture::start` entry point. `CaptureConfig::start` remains available.

The [capture hardening verification log](parallel-capture-review-fix.log) records three Linux staging tests, two ordinary capture tests, thirteen tools tests, two isolated Linux libpcap tests, and a Windows capture component type-check. The staging fixtures verify that replacing the selected directory cannot redirect or discard captured bytes and that cleanup preserves a replacement staging directory. The namespace tests used only fixture loopback traffic. Native capture and staging behavior on Windows and macOS remain unverified.
