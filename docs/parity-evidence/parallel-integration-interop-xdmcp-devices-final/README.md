# Combined interop, XDMCP and device gate

2026-11-25. The offline locked all-targets run passed 717 tests, zero failures, 51 ignored across 68 targets. All 511 captured source/resource hashes remained unchanged. The explicitly supplied pinned Xephyr archive and its SHA are recorded in inputs.json. The prior failing gate remains retained in the sibling directory; its optional-input guard correction is separately reviewed.

The merged RDP helper passed 47 unit and eight PC/SC integration cases. Actual root XDMCP qualification also passed 16 focused tests, one isolated real xdm/cookie/monitor/two-session lifecycle case and pinned-payload packaging/corruption checks; logs are under X04/root-*.log. V01/V02 actual transport matrices and their independent reviews are under their evidence directories.

V01/V02 implementation checkpoints advance to 42/60 (70.0%), reflecting implemented runners and verified available-host cases. Native/original parity remains 0/60. D02/X04 retain their documented unsupported operations/native/source qualification gaps. The current immutable 74443392 release predates these source merges; it is not relabeled as latest.
