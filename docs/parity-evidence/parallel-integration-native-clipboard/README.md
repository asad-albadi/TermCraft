# Native source candidates, VNC clipboard and macro recovery

2026-11-25. Overall implementation checkpoints remain 40/60 (66.7%); strict native/original parity 0/60.

The fresh unfiltered offline locked all-targets gate passed 684 outer-target tests, zero failures, 49 ignored across 66 targets. All 471 captured source file hashes remained unchanged. Independent reviewed source candidates and the macro acknowledgment fixture correction are integrated. Formatting and two offline source-preparation Python tests also passed.

Actual private bwrap/Cage VNC qualification passed both cases: real X11 clipboard classic/Unicode/STRING/view-only/no-echo/selection-ownership cleanup, and retained framebuffer/authentication/key-layout/control/independent vncdotool/Stop behavior. No host clipboard, input or framebuffer was accessed. Fixed executable copies and hashes are under target/qualification-native-clipboard; source provenance is source-sha256.json.

Native Windows/macOS display backends remain disabled because patched executable build checksums and actual native runtime qualification are absent. Native Windows clipboard is API-checked, runtime unrun. Wayland/macOS VNC providers, HTTP viewing, original/hardware evidence and clean-machine closure remain open. E03 and X01 are not completed by this source slice. The existing 74443392 release predates these candidate merges; it remains a separately bounded tested archive.
