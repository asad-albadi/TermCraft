# B02 HTTP sharing

The Linux integration tests exercise the real HTTP server through ephemeral TCP listeners on loopback. The fixture owns all shared and neighboring paths.

| Case | Observed behavior |
| --- | --- |
| Range and cache validation | GET of bytes 2–4 over `abcdef` returns 206, `cde`, `Content-Range: bytes 2-4/6`, and length 3. Suffix ranges return the expected bytes. An unsatisfiable range returns 416. HEAD describes the full file and sends no body. A matching weak ETag returns 304. |
| Authentication and default access | Missing or incorrect bearer credentials return 401 before file access. Correct credentials permit download. The compatibility start method rejects PUT with 405. |
| Upload bounds and conflicts | Authorized, enabled PUT publishes the complete file once. Existing destinations return 409, oversized declarations return 413, and truncated bodies do not leave a destination or staging file. Two simultaneous PUTs yield one 201 and one 409. |
| Bounded listings | Deep listings render their directory path once and use relative links. The server returns 503 above 4,096 scanned entries or a 1 MiB rendered body. Controlled partial-write fixtures verify listings stop at their deadline or on cancellation. |
| Confinement and cancellation | Encoded traversal and symlink escapes are rejected. Dropping the running server interrupts an unfinished PUT and removes its staging file. |

The pre-change component fixture produced unresolved API errors for `HttpShareConfig` and `Server::start_with_config`. The workspace's first pre-change run was blocked by a concurrent desktop module type error. The post-change workspace command was `CARGO_BUILD_JOBS=2 cargo test --offline --test http_sharing -- --nocapture`.

A component-only cross-target type check passed for `x86_64-pc-windows-msvc`. No Windows runtime or original Personal HTTP behavior was measured. This evidence covers Harbor's HTTP server and does not establish original-product parity.

The implementation uses plaintext HTTP, an optional bearer token, and opt-in PUT uploads capped at 64 MiB. Publication requires hard-link support in the shared filesystem. Filesystems without hard-link support, such as FAT and exFAT, can serve downloads but cannot publish uploads. A publication error returns an explanation and cleans up the staging file. A destination already present is preserved, including when another request creates it during transfer.
