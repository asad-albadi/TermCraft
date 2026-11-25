# E03 VNC character-alias correction review

2026-11-24. Focused independent rereview of the remaining Important alias issue in `vnc-review-fixes.md`, limited to that correction and new breakage in its translation/ownership paths. **Verdict: the scoped finding is resolved; ready for root integration and its integrated gate. No new Critical or Important finding established. E03 remains open.**

## Identity and review limits

- Independently verified `vnc-private.patch` SHA-256 `a07c5a62430699009eba49fad3a3e28f471a7858fe106f216ac446b10bde3944`.
- All 16 private candidate entries and all eight integration-hook entries matched their manifests. Seven of eight production-base entries matched; `src/ui.rs` had changed during the parent's announced concurrent Browser integration. This is a current rebase requirement, not a claim that the historical dry-run still applies to the changing tree. Parent was informed immediately.
- Read the corrected shared character helper, Linux XKB matcher/key-release path, Windows planner/canonical held identity, new native assertions, dependency/lockfile diff, implementation notes and alias-specific logs. No application builds, broad tests, live X access or production source edits were performed by this reviewer.
- Existing native and component results cited below are implementer executions, inspected rather than independently rerun.

## Finding resolution

The shared helper converts both legacy EuroSign `0x20ac` and Unicode `0x010020ac` to the same scalar. It likewise normalizes legacy/Unicode Greek alpha. Linux compares converted scalar values against the actual XKB map instead of comparing raw symbol numbers; Windows passes that same converted character into its existing literal/shortcut planner. This addresses both directions of the previous asymmetry. The exactly pinned pure Rust xkeysym 0.2.1 dependency supplies the conversion; the Cargo.lock diff adds only the existing locked dependency to the production root package, without changing production version 0.1.0.

Both providers canonicalize held-key identity before matching down/up events, so mixed legacy/Unicode releases find the original press. Function, modifier and keypad symbols remain distinct from text. The helper explicitly rejects control text; invalid Unicode such as a surrogate does not become a text character. Windows retains the prior shortcut-required modifier handling and its explicitly stated BMP limit. The Linux shared-module configuration is enabled for ordinary Linux builds, rather than only Windows/tests.

Linux now releases an existing held key using its recorded keycode, avoiding a fresh character translation requirement on keyup. Unmappable valid text returns successfully before held-key insertion, temporary-modifier changes or input injection; its later unmatched keyup is ignored. The service therefore continues sharing after such text. Existing actual XKB type/group/lock matching and owned-only temporary modifier transitions remain intact. No additional alias-related modifier ownership, cleanup or special-key regression was established by inspection.

## Focused evidence

The native RED log records the private target failing to receive Unicode Euro before the correction. The GREEN fixture tests both Euro encodings on a changed German layout and legacy/Unicode Greek alpha on a Greek layout, including mixed-encoding keyup. It then sends an unmappable rocket and requires subsequent interpreted character delivery, a live frame and Running service status. The original case/shifted punctuation, explicit Shift restoration, Ctrl/Alt, CapsLock, explicit AltGr, independent vncdotool and entire initial pressed-key bitmap restoration assertions remain present.

Inspected logs record 17 library tests and four UI tests passing, the explicit private Cage native wire case passing, the actual Servers Start/Stop pointer case passing, the actual Windows provider component check passing, and the Linux component plus three pure helper/planner tests passing. These checks directly cover the reported gap; no broad rerun was requested or performed here. Native Windows execution remains unrun, and component checks do not establish its runtime behavior.

## Root integration and open scope

Rebase shared hooks onto the final root sources, preserving the concurrent Browser work, Recovery deadline/password helper and approved SCP guard; regenerate affected patch/manifests and run the integrated gate. The old dry-run log identifies its earlier baseline. Do not copy the private package version into production.

Bidirectional clipboard, native macOS, approved Wayland portal capture/control, browser HTTP viewing, exact original settings and native Windows qualification remain outstanding. This scoped approval closes neither E03 nor any overall parity item. Progress remains 40/60 (66.7%).
