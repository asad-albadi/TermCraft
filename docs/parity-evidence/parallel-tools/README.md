# P07 graphical tools

The Tools page has local file and folder comparison, PNG and JPEG preview, and a TCP port check. The comparison and preview read only paths selected through the file picker. The application keeps the selected paths and results in memory. Each operation has a single background worker and the TCP worker can stop between connection attempts. Folder comparison can stop between entries and file reads.

- File comparison checks exact bytes and shows inserted and deleted lines for small UTF-8 inputs. Folder comparison anchors both trees to open directory handles, rejects symbolic links and Windows reparse points within the trees, compares matching files by content, and reports paths present on only one side or changed in place. It enforces limits on entries, directory depth, and bytes read.
- Image preview checks the encoded input size and image dimensions before decoding the full image. It uses separate per-image pixel and texture size limits.
- TCP checks use one literal IPv4 or IPv6 address and up to 256 explicitly entered TCP ports. They report successful connections, refused connections, timeouts, and other unreachable results under the address actually checked. Network fixtures use test-owned loopback sockets.
- The Packet Capture section reports that this build has no capture provider. Neither `dumpcap` nor `tcpdump` was found on this host. Live capture, capture cancellation, live external-target validation, original application parity, and native platform validation remain open.

`red.log` records the initial integration fixture failing because the tools API was absent. The targeted verification command is `CARGO_BUILD_JOBS=2 cargo test --offline --test tools -- --nocapture`.

`green.log` records all 11 Linux integration tests passing, including exact folder results, UTF-8 and binary differences, symbolic link and depth rejection, an oversized file, PNG/JPEG pixels, a pre-decode dimension rejection, and loopback connection and cancellation behavior. The command also compiles the desktop binary with the Tools page wired.

## Directory replacement and address validation

The hardening fixtures rename the selected root after enumeration and replace its old path with a symbolic link. Comparison continues through the original open directory. A separate fixture swaps a nested directory for a symbolic link after enumeration, and the comparison rejects the replacement before reading the matching file. The cancellation fixture stops traversal after it starts walking the first selected root.

`hardening-red.log` records the fixtures before the anchored comparison and validation APIs existed. `hardening-first-run.log` records the two directory fixtures passing and the port validation suite detecting that the first build did not canonicalize IPv4-mapped IPv6 addresses. The final code canonicalizes addresses for validation and configuration fixtures call validation directly, without starting any network worker.

`hardening-green.log` records `CARGO_BUILD_JOBS=2 cargo test --offline --lib --test tools` completing with 53 library tests and 13 Tools integration tests passing on Linux. The tools source and tests also pass a scoped Rust formatting check. The Windows reparse point check still needs native validation.
