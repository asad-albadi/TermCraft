# Final mouse fixture follow-up

CI45947625189 passed every native Mac target except the mouse-release backlog test; Windows and Linux were green. The three-platform package run45947639840 passed all startup gates.

The fixture admitted its release after filling32 message slots with32KiB packets, then required the native reader to drain about1MiB within four seconds. Queue admission and wire delivery are separate. A standalone exact-production InputQueue/nativePTY experiment with an8ms/1KiB reader reproduced the assumption:1,081,344 filler bytes and five actual full samples admitted the release, but only500,736 bytes had drained at4.002s. The1KiB packet variant still measured five full samples, admitted the release, and consumed54,281 bytes including its nine-byte packet in437ms. These are controlled Linux measurements, not claimed Mac throughput.

The test now uses1KiB filler packets while keeping its five full observations, pending-release requirement, retry and four-second ACK checks. Bounded owned-file diagnostics record reader progress. No production input code changes. Native Mac replay is required. The combined Mac CI job additionally requires an available Chromium fixture so provider absence cannot silently satisfy its browser tests.
