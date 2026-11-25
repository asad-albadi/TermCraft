**SPEC ACCEPT · QUALITY ACCEPT** for the nine-path Linux x86_64 keyboard candidate 957.

Independently matched all 52 sealed artifact entries, both archives (9 and 733 members), the nine current MAIN base digests and the post-application map in a private scratch copy. The four recorded rebuild inputs matched. Both final engine builds and the decompressed payload matched 8cf61e9f10f5c797c249eae12a36f8cc0f43ca51ceda89c62bc663593f30f460.

The editor changes only the existing keyboard string (src/ui/xdmcp.rs:334). Validation and literal argv construction are at src/x11/xdmcp.rs:291 and :408. Catalogue membership and stock versus ready selection are at src/x11/xdmcp_engine.rs:249 and :263. Parent clipboard access remains separately conditional at :407. The native patch requires explicit layout compilation, reapplies default rules each generation and retains the session device map instead of copying the parent (packaging/x11/xdmcp/readiness/xephyr.patch:69, :105, :140).

The isolated US and German fixture logs report opposing keysyms on 29/52 and unchanged complete parent maps. The German bidirectional and hostname cases passed. The missing-symbols case records the startup diagnostic and tests child/socket cleanup. Sealed evidence records 41 focused library passes, 11 individual isolation passes and 9 UI passes. The ordinary private debug executable used the documented compiler workarounds. This review verified the recorded evidence and hashes without repeating the builds or live fixtures.

Qualification covers Linux GNU x86_64, the installed evdev catalogue, and two observed layouts. Physical keyboard input, other platforms, and the separate chooser candidate remain outside the review.
