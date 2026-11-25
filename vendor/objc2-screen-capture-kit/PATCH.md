# Lazy framework loading patch

Exact crates.io `objc2-screen-capture-kit` 0.3.2, upstream commit
`7b1abfd750a2cacaea71d6a56ecfb83cb7de560b`, path
`framework-crates/objc2-screen-capture-kit` in https://github.com/madsmtm/objc2.
Selected license: MIT. LICENSE.md is the exact-commit upstream declaration.
LICENSE-MIT.txt is upstream main's full MIT text fetched 2026-11-25; the
0.3.2 commit distributed only the license declaration/link.
Crate checksum is pinned in the application source-provenance.json.

Only code change: remove `#[link(name = "ScreenCaptureKit", kind = "framework")] extern "C" {}`
from `src/generated/mod.rs`. Application macOS provider loads the absolute Apple system
framework path on explicit Start after macOS14 version check and retains it forever.
All binding types/methods remain upstream. Do not enable objc2's unstable-static-class.
Used SCK APIs are Objective-C class/message APIs, with dynamic class lookup; no SCK
extern function/constant is used. Reapply and audit this patch on every binding update.

This avoids imposing a strong framework autolink on the application. A real Mach-O
link/startup on older supported macOS remains UNAVAILABLE on the Linux build host.
