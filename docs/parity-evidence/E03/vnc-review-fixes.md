# E03 VNC modifier correction review

2026-11-24. Focused independent source/evidence review of the correction to the prior Important case/shifted-punctuation/cross-layout finding. **Verdict: with fixes. One Important cross-layout gap remains. E03 remains open.** No production source edits, application builds, live X operations or broad suite reruns were performed.

## Reviewed identity

- Patch SHA-256: `2a507921cdc9e280428d28c27698c4659ce6fedb0a90e4dab294d088015b27b9`.
- Independently verified all 15 private candidate, seven integration-hook and seven current production-base manifest entries; all matched at review time.
- Reviewed the actual corrected `src/services/vnc/x11/keyboard.rs`, `x11.rs`, `keyboard.rs`, `windows.rs`, native fixture assertions and independent fixture interpreter. Read modifier/native/UI/component logs; their test executions are implementer evidence, not independent reruns.
- The recorded 16 library/four UI passes, explicit private Cage native wire/UI passes, independent vncdotool check and actual-provider component checks match the stated narrow evidence. Native Windows runtime remains unrun.

## Corrected behavior

The original case/shifted-punctuation defect is addressed for the exercised symbols. X11 now reads actual XKB types, group, modifier mapping and effective/locked/latched state; it plans the requested level rather than flattening rows. It restores explicit remote modifiers after each event, protects Control/Alt/Win-like shortcut masks and refuses transitions requiring release of an unowned modifier. Temporary presses and owned keys are tracked for cleanup. The held-key lookup preserves the original keycode for release, and the event path records ownership before injection. Source failures stop the service and drop the provider, giving its release path a cleanup attempt.

Windows retains the required modifier byte for layout-based shortcut mapping, checks generic modifier state before synthesizing missing modifiers, restores its temporary modifiers, and uses literal Unicode text outside shortcuts. The new code does not deliberately release client-held Ctrl/Alt/Shift to synthesize text. Component compilation verifies ABI/type usage only; SendInput scheduling, actual character delivery and runtime failure cleanup still need native Windows qualification.

The native fixture asserts interpreted A/# without Shift, lowercase with explicit Shift followed by restored uppercase, Ctrl/Alt state, CapsLock behavior, changed German layout, legacy EuroSign/AltGr and complete initial pressed-key bitmap restoration after disconnect. Its libxkbcommon interpreter reads the fixture server map and is independent of the provider translation algorithm. No additional Critical/Important modifier ownership or ABI issue was established in this scoped review.

## Important — normalize Unicode and legacy character keysym aliases

Locations: private `src/services/vnc/x11/keyboard.rs:183`; private `src/services/vnc/keyboard.rs:13–16`; native fixture `src/services/vnc_service_tests.rs:431`.

The Linux matcher compares raw keysym numbers. A standard German XKB map exposes EuroSign as legacy keysym `0x20ac`, while a client may send the same character as Unicode keysym `0x010020ac`. The latter passes the provider's character classification but can never match this map. It returns a translation error; `vnc.rs:203` treats that as a source failure and stops the entire sharing service. The fixture exercises only `0x20ac`, so its successful Euro test misses this case.

The Windows helper has the inverse gap: `0x010020ac` is accepted, but legacy `0x20ac` is rejected, as are other legacy non-Latin character keysyms. There is no special-key fallback for EuroSign, so it is silently dropped. The Windows mapping test uses only the Unicode form. This falls within the original cross-layout correction, rather than a request for a new protocol feature.

Independent, display-free verification used the installed libxkbcommon API: `xkb_keysym_to_utf32(0x20ac)` and `xkb_keysym_to_utf32(0x010020ac)` both return U+20AC; legacy Greek `0x07e1` and Unicode `0x010003b1` both return U+03B1. An offline `de` keymap compiled through `xkb_keymap_new_from_names` contains `0x20ac` at keycode 26 levels 2/3 and contains no exact `0x010020ac`. This check connected to no X server and generated no input. It confirms the source-level mismatch; no live failure reproduction is claimed.

Fix: convert supported character keysyms to their Unicode scalar for comparison/planning, preserving special-key identities and modifier ownership. Support equivalent legacy and Unicode forms in both providers. Add both Euro forms to the existing isolated fixture and Windows pure mapping tests; one legacy/Unicode non-Latin pair would guard the general conversion. Assert the service remains active after the translated input and preserve the existing disconnect bitmap assertion. A broad suite rerun is unnecessary for this remaining translation-only change.

## Integration and remaining scope

Root must preserve any later production changes to `managed_servers.rs` when rebasing the shared hooks, regenerate affected hashes/patch and perform the integrated gate. This review does not authorize copying the private package version to production. The patch dry-run and manifests describe the reviewed instant, not future baselines.

Bidirectional clipboard, native macOS, approved Wayland portal capture/control, browser HTTP viewing, exact original settings and native Windows qualification remain explicitly outstanding. This correction and review close no parity item; overall progress remains 40/60 (66.7%).
