# Pre-merge code and security review

The owner confirmed the current RDP review works, then completed manual testing and authorized a code/security review and fix loop before merging. This supersedes the earlier merge hold only after the review and automated checks pass.

## Scope

Review compares the complete issue-4 branch against main `3059bac10b0ef39479a3286a07fa0124175064d1`. Independent reviewers examined security-sensitive Rust/UI changes; rendering, input ownership, lifecycle and desktop transport correctness; and release/CI scripts and issue acceptance. The branch is large (273 changed files before this fix loop); review and passing tests do not establish the absence of every defect or replace native hardware qualification.

Security review covered RDP TLS/NLA and certificate identity, credentials and undo history, verified helper launch, private wire geometry/revision bounds, VNC resizing, modal input release, file/service/import UI and diagnostics redaction. No confirmed critical or important security defect was found in those changes. Dependency and tracked-source secret-pattern checks are recorded separately below when complete. No real passwords, owner remote endpoints or clipboard data were used.

## Findings and fixes

1. **Solo game metadata clipped below the fitted board (important correctness).** The board consumed the visible height and its topology/difficulty labels were painted below the clip. Move the existing metadata above the board, retaining centered, responsive geometry. Preserve exact grade, cage and region assertions; additionally require labels to be visible above the board.
2. **RDP test client treated protocol-2 cropped frames as JSON.** The production receiver already handled those packets, but the queued-pointer/server-shrink fixture only recognized full frames. Accept both frame forms and validate canvas dimensions, payload length, patch geometry, baseline and contiguous revisions. Keep the real pointer clamp/release assertions.
3. **Stale font/sidebar expectations.** Required Nerd Font fallback is now checked explicitly for each alias. The sidebar regression checks the actual current session-tree empty state and selected sidebar mode, then verifies switching back preserves connection identity and remote selection.

No authentication downgrade, unbounded helper frame queue or uncontrolled command interpolation was identified in the reviewed changes. The new graphics compositor has its own dependency-level memory bounds; published framebuffer validation is separate, as documented in [the RDP graphics evidence](rdp-graphics.md).

## Completion record

Exact-head hosted CI and dependency scan results will be added after they finish. The owner already accepted the reviewed Linux behavior. Windows/macOS physical input, permission, display and device qualification and public distribution gates remain separate from issue #4; see issues #1, #2 and #3. Their existing unchecked requirements must not be relabeled as completed by this review.

## Dependency and local checks at this revision

RustSec database `3eebb903ba6c43f60c57dc0e15fa61eef06231d1` (1,571 advisories, updated 2026-11-28) identified two workspace advisories. Updated the pinned `url` dependency to 2.5.9 for [RUSTSEC-2026-0592](https://rustsec.org/advisories/RUSTSEC-2026-0592.html), and `webpki-root-certs` to 1.0.10 for [RUSTSEC-2026-0383](https://rustsec.org/advisories/RUSTSEC-2026-0383.html). The workspace rescan reports no vulnerability findings; maintenance warnings are not represented as fixed. The independent Wayland lock reports no vulnerabilities. The RDP lock retains the genuine prerelease RSA advisory and transitive heapless unsoundness warnings. Independent source/reachability review found their affected operations unreachable in current application use, as detailed below; the RDP scan is not clean and no advisory was globally ignored.

Local checks passed 16 focused UI tests, 10 RDP endpoint/bundle checks and 61 packaging checks (21 explicit skips). The release/native modules passed 29 tests with one skip; release workflow's initial import failed because local PyYAML was absent. Installed Pillow/PyYAML into an isolated environment and reran that module: four checks passed. Tracked-source token/private-key pattern inventory contained only key-format handling strings and synthetic redaction-test content; this limited scan is not an exhaustive secret-detection guarantee. Hosted CI must validate the updated dependency resolution on all three supported targets.

## Remaining dependency applicability

`rsa 0.10.0-rc.18` has the genuine [Marvin timing advisory](https://rustsec.org/advisories/RUSTSEC-2023-0071.html). The prerelease is not exempt merely because it uses crypto-bigint. The current RDP path only constructs `Credentials::UsernamePassword` and passes no Kerberos configuration to CredSSP, selecting NTLM in the pinned connector. Outer TLS uses rustls. Native redirected smartcards use owned platform PC/SC workers, not the transitive emulated card's software private key. No RSA private decryption/signing oracle is supplied to a remote peer in this path. The root app already resolves stable RSA 0.10.0.

Three pinned upstream authentication crates require exactly the prerelease RSA version; updating it independently is rejected by Cargo. A coordinated upstream update is preferred over introducing local forks of all three for an unreachable operation. This is a documented applicability disposition, not a dependency fix. Enabling Kerberos, software smartcard authentication or JWE/private decryption must trigger reassessment and an upstream update before release.

The nine heapless unsoundness warnings arise from `iso7816 0.1.4 → winscard 0.4.0 → sspi 0.23.0`. Complete iso7816 source references use only `heapless::Vec<u8, N>`. The affected SortedList (0574/0578), SPSC (0575), IndexMap (0576/0582), Deque (0579/0581) and pool/Treiber (0580) operations are absent. The panicking-Drop case (0577) cannot occur for `u8`. These are RUSTSEC-2026 IDs; maintenance warnings remain recorded separately. Reassess these dispositions when changing the authentication or APDU dependency path. Both versions predate this PR on main.

The [machine-readable audit record](premerge-dependency-audit.json) preserves findings and lock hashes. Raw scans remain available locally under `target/issue-4/premerge/audit-*.json`. The scan covers Rust dependency advisories, not a fresh Chromium/private-X binary audit or production distribution approval; those runtime gates remain in issue #3.
