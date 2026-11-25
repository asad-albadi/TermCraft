# Genuine packaged GUI Wayland VNC acceptance

**PASS:125 fresh artifact/GUI/RFB/native checks.** The feasible local E03 packaged-GUI gap is closed within a configured private Linux GNOME/Mutter headless desktop. This report does not claim global all-clear, physical-GPU desktop parity, Windows/macOS qualification or completion of unrelated VNC-client work.

## Genuine release identity

Ordinary scripts/package-release.py rebuilt normal app/RDP from frozen source **d1a54c2ec020d0bc2a4e731184065bc33c28362f9b56f9bd95401f8a14947b9f**, with the normal optional Wayland helper built first from that same snapshot. Verified SDK1.96.1, locked/offline native builds, isolated Cargo home, environment CARGO_BUILD_JOBS=2 and serial --jobs1 wrapper/CPU0. Only capture.rs changed versus source22bb;595 other source inputs are unchanged. Build/repackage finished0 in322.44s, with no source drift.

- Archive **8a9c97e5dc5791ee278fb485a2082f5fa7c75220801f798e46e1c21a0dd72bc8**, target/package-post-wayland-idle-fix-reviewed/TermCraft-0.1.0-linux-x86_64-arch-xwayland-xdmcp-mosh-unix-chromium-wayland-sharing.tar.gz.
- Normal app **c8d09ba5fd2c36d567c6885f117d6068e2a87185648db883853cde84d678f7e6**, bit-identical after genuine rebuild; RDP helper **85f33c8c7987cf84d73748c2d33283cf9878426febcbfed72a3f26acb42beba6** unchanged.
- Corrected genuine Wayland helper **fa7147099cda83486a4bdc3a59510cecc0e560e10200f4c1bac4bf6ff5b19f59**, native libpipewire-0.3.so.0 ABI, SDK1.96.1 provenance recorded.
- Strict extraction and all **3372** manifest file hashes verified by runner, independently rechecked by root. Exact commands, source inventory, helper provenance, SDK/ELF/linkage, archive integrity and build result: docs/parity-evidence/parallel-integration-wayland-idle-fix-release. Old398d/helper26c349 remain immutable RED evidence.

No fixture/debug app or provider harness was used for the following GUI service: launched that exact packaged app normally; selected Servers and actual Wayland portal sharing controls; typed volatile password; clicked actual Start/Stop. Independent memory-only RFB3.8 client performed ordinary VNC password challenge authentication to the actual GUI-started127.0.0.1:5900 service. Production source/GUI binaries were not instrumented or modified.

## Actual measured results

| Case | Observation |
|---|---|
| GUI Start and native consent | Actual GNOME Remote Desktop dialog; fixture clicked Allow Remote Interaction/Share. GUI Running, real listener and app-owned sealed helper present; /proc executable image hash equals genuine packagedfa714 helper. |
| Static desktop | GUI minimized to owned target. After **8.00038seconds** without target changes, app listener, sealed helper and native observer all remain alive. |
| RFB pixels | Retained before/after RFB1600×1000 images have identical pixel SHA6081f460a7e70a527da5586e0b6a941fd72a2dc29558b632bfdb65614d24c97b; each exactly matches independent native-monitor screenshot, every RGB channel mean difference0. Known green target pixel(38,209,145), legible owned-target title. Distinct .rfb.png/reference.png files retained. |
| Real RFB input | Ordinary a press/release and absolute pointer/button reach owned GTK receiver. Global(650,450) maps to client(230,181), matching window/header geometry. Native event/wire traces retained. |
| Held-input client disconnect | RFB client disconnects with Shift and left button held. Both native releases observed about6ms later; listener/session/helper stay live for next client. |
| **Actual GUI Stop while connected/Shift held** | Clicked real Stop. Listener and sealed helper gone in **0.447s**, portal closed-count0→1, client EOF; GUI visibly **Stopped**, siblings alive. Later target native b is lowercase/state0, proving held Shift cleared after the real service cancellation→source-drop/session cleanup path. |
| Password/Undo | Cleared actual password field remains empty after native Ctrl+Z. Every private-home/config/data scan reports zero plaintext password hits across five files. Password generated/held only in fixture memory, typed into masked volatile field, never in command files/logs/argv/env; app wire logging disabled. |
| Genuine session revocation | A fresh actual GUI Start/native grant/RFB authentication succeeds. Fixture invokes real GNOME backend Session.Close on /org/freedesktop/portal/desktop/session/1_114/harbor_5; listener/helper gone in **0.217s**, client EOF. GUI correctly Failed with Selected display capture stopped. This is genuine backend-session termination, not a claimed physical system revoke-button click. |
| Actual GUI view-only | Unchecked control in real GUI, fresh native Share Screen grant and authenticated RFB capture. Pixels exactly match independent native screenshot. RFB key/pointer requests produce **zero target events**, exact telemetry byte equality; real GUI Stop closes listener/helper/session in0.448s and client EOF. |
| Actual native denial | Clicked native Cancel. GUI Failed with Desktop sharing was denied or cancelled; no listener/helper, session closed, siblings alive. |
| **Actual GUI Stop during pending permission** | While native Share Screen prompt and GUI Starting were visible, clicked real GUI Stop. Pending portal session closed in **0.427s**; GUI visibly Stopped; no listener/helper was ever started, closed-count4→5. |
| Cleanup | All explicitly owned child processes reaped; private PID namespace exited; all app/target/compositor siblings survived every service lifecycle until intentional fixture teardown. No leftover product helper/listener. |

Final authoritative evidence is final-reports/, final-run.log, FINAL-VERIFICATION.json, verify-final.py and FINAL-EVIDENCE-SHA256.json. All76 acknowledged native GUI/RFB operations succeeded; command76 intentionally requested teardown. No extra GUI cases remain pending. Unit/native-provider results are separate from this actual packaged-GUI proof.

## Fix and scope

The initial genuine GUI run uncovered that five-second no-damage timeout incorrectly killed healthy static Mutter streams. Independent real native7.000s reproduction failed with old helper26c349; two meaningful regression tests failed before correction. Independently reviewed/accepted capture.rs candidateb39b189 was mechanically integrated by root. It keeps initial-frame acquisition bounded, retains valid static pixels while transport is live, and rejects native error or exited-worker cache. All six engine library tests pass. Genuine rebuilt-helper native test additionally idles8.000s successfully and fails closed on actual native backend loss. RED packet and raw runs remain preserved.

Signed private Mutter51.1/GNOME portal51.0-2, real private PipeWire/WirePlumber,1600×1000 owned virtual monitor and owned GTK target. The fixture established a separate virtual keyboard **before target/app map**, modelling an existing desktop keyboard; the earlier truly keyboard-less-seat binding race is independently attributed/preserved in prior evidence. Fixture native input was used only for owned GUI/consent/setup and post-Stop state observation. Product input effects above came from the independent RFB client.

Private network/PID namespaces, private bus/HOME/config/data/runtime, hidden host/home/run, minimal/dev without real GPU/input/audio devices. No host desktop, portal, bus, home, clipboard, input, network or system service was accessed during tests. No physical desktop/GPU or other-OS claim.

This proof applies exactly to archive8a9/current component identities. A later client-only fix/package must retain explicit identity/scope transfer: unchanged server/GUI/provider source inventory and helperfa714, with independent review of changed client paths. A changed package hash alone does not require replaying unchanged native-provider/GUI coverage, but this report does not automatically approve later artifacts or unrelated clipboard behavior.
