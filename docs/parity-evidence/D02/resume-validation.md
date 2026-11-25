# D02 listener investigation and resume — 2026-11-24

No D02 checkpoint or full parity outcome is claimed. Current totals remain **25/60 implementation checkpoints, 0/60 fully verified parity outcomes**.

## Execution restriction

Command, from `app/`, using the same `exec_command` route as Cargo:

```sh
python3 docs/parity-evidence/D02/listener_probe.py
```

Exit **1**. [Actual output](listener-probe-2026-11-24.txt): IPv4 `socket()` returns `EPERM`; Unix-domain `socket()` succeeds but `bind()` returns `EPERM`. A normal file in the same fresh temporary directory can be written. UID/GID are 1000; `NoNewPrivs: 1`, `Seccomp: 2`, one seccomp filter. `/proc/1/cmdline` identifies **codex-linux-sandbox**, `--apply-seccomp-then-exec`, and runner network policy **restricted**. This establishes an execution-sandbox restriction rather than a project directory's Unix mode or an occupied fixture port. The filter itself cannot be inspected here; exact filter rules and an additional LSM contribution cannot be independently attributed. Reading the current LSM label returns EINVAL.

The explicitly authorized alternative was tried through Craft Anything:

```text
build_command_run(command="CARGO_BUILD_JOBS=2 cargo test --offline --test rdp_options -- --nocapture", timeout_secs=600)
→ MCP tool call requires approval, but approval policy is never
```

This route is unavailable under the current approval policy. No elevation, sandbox disabling, alternate process escape or approval bypass was attempted. No available authorized route permitting listeners has been established.

Run this command **on the authorized Linux host outside this restricted runner**:

```sh
cd '/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app'
bash docs/parity-evidence/D02/run-host-validation.sh
```

The script first checks actual TCP and Unix listeners, then runs all three original commands, retaining output separately as `host-*.log`. It continues through failed test targets and returns failure if any command fails. It does not alter tests, bind publicly, change host settings, install services or skip fixtures.

## Commands and actual results in this environment

| Command | Result / retained output |
| --- | --- |
| `CARGO_BUILD_JOBS=2 cargo test --offline --test rdp_options -- --nocapture` | Exit 101; 7 pass, 1 fixture failure (`redirection_options_match_negotiated_channels`). [Log](resume-rdp-options.log) |
| `CARGO_BUILD_JOBS=2 cargo test --offline --test rdp_session -- --nocapture` | Exit 101; all 9 fail during real fixture setup. [Log](resume-rdp-session.log) |
| `CARGO_BUILD_JOBS=2 cargo test --offline --no-fail-fast` before changes | Exit 101; 15 failing targets. [Log](resume-full-before.log) |
| `CARGO_BUILD_JOBS=2 cargo test --offline --test desktop_fullscreen` before implementation | Exit 101; missing `session_fullscreen` API. [RED](fullscreen-red.log) |
| `CARGO_BUILD_JOBS=2 cargo test --offline --test rdp_credentials` before implementation | Exit 101; missing `credential_binding` API. [RED](vault-red.log) |
| `CARGO_BUILD_JOBS=2 cargo test --offline --test rdp_credentials --test desktop_fullscreen` | Exit 0; 3 local regressions pass. [GREEN](vault-green.log) |
| `CARGO_BUILD_JOBS=2 cargo test --offline --bin harbor-workspace real_rdp_vault_save_use_delete_and_session_fullscreen -- --nocapture` | Exit 101; real RDP fixture cannot start (EPERM). UI/protocol behavior pending, not regression RED evidence. [Log](vault-ui-live.log) |
| `CARGO_BUILD_JOBS=2 cargo test --offline --manifest-path engines/rdp/Cargo.toml` | Exit 0; 2 clipboard backend tests pass. [Log](resume-helper-tests.log) |
| `CARGO_BUILD_JOBS=2 cargo test --offline --no-fail-fast` after changes | Exit 101; 15 failing targets. Raw summed result lines: **259 passed / 127 failed / 6 ignored**, including nested-child and doc-test results, not top-level checkpoint totals. [Full log](resume-full-final.log), [individual diagnostic inventory](resume-full-failures.md) |
| `cargo fmt --check` | Exit 0. [Log](resume-fmt.log) |
| `CARGO_BUILD_JOBS=2 cargo build --offline` | Exit 0; existing dead-code warnings. [Log](resume-build.log) |
| `CARGO_BUILD_JOBS=2 cargo check --offline --target x86_64-pc-windows-msvc` | Exit 101; C/crypto build cannot find Microsoft `lib.exe`. Not native Windows validation. [Log](resume-windows-check.log) |
| `rebuild_check(project="/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6", wait_minutes=2)` | Build/startup screenshot pass (942×1005); mechanics/original comparisons incomplete. [Report](resume-rebuild-check.txt) |

The failure inventory retains every emitted individual diagnostic block. Direct socket EPERM is distinguished from indirect agent readiness, child-process status and fixture wrappers; unresolved wrappers require individual host reruns. Neither the historical 373-pass suite nor a screenshot verifies this source's protocol behavior.

## Implemented increment and remaining scope

- Automatic fullscreen: the persisted session flag is enabled in the editor. On first paint of a connected desktop, its actual root/detached viewport enters fullscreen through the existing window controller; restore geometry is preserved. The request is consumed once and does not override subsequent manual exit. Two local window-controller regressions pass. Native window-manager and real connection/UI evidence remain pending.
- Desktop vault: a typed RDP binding includes exact endpoint, certificate fingerprint, actual username/domain, and authentication-profile digest. The real encrypted vault rejects certificate/user/domain/profile changes in a passing local storage test. UI offers explicit optional Save and Use; Use fills the live matching sign-in form without submitting. Lock/delete and username/domain edits clear vault-filled passwords; saves link only a still-matching local saved definition. Ordinary workspace JSON holds only the opaque reference. The added owned-RDP/egui test must still pass on the host; these controls are not yet a fully verified end-to-end outcome.
- Clipboard: existing real direction/channel and UI transport tests remain intact. Helper-level initialization/UTF-16 validation passes. Actual endpoint/OS clipboard behavior is still blocked, not replaced by mocks.
- Administrative/device providers remain **unimplemented** in the embedded engine. The installed-adapter profiles and capability rejection are not evidence of implemented embedded drive/serial/printer/smartcard redirection. Completing them requires provider implementation and real endpoint/device evidence. No unavailable option was relabeled supported.
- Native Windows/macOS, physical devices and original Personal 26.6 comparisons remain external dependencies. D03–V08 remain the authorized queue; this increment does not claim implementation of them or full completion of the request.


## Increment review and fix pass

An independent reviewer identified incorrect automatic fullscreen routing for egui embedded detached windows. The regression was reproduced using egui's actual embedded viewport fallback and fixed by requiring the target and rendered viewport IDs to match before consuming the request. [Review](resume-review.md), [behavioral RED](fullscreen-review-red.log), [GREEN: 4 local tests](fullscreen-review-green.log). No Critical or additional Important credential findings. This is a code increment review, not native/protocol verification.


Post-review final results: full-suite exit 101, 15 failing targets, raw 260 passed / 127 failed / 6 ignored (child/doc-test summaries included). [Final full output](resume-post-review-full.log), [current individual diagnostics](resume-post-review-failures.md). Final app build exits 0 ([resume-final-build.log](resume-final-build.log)); formatting exits 0. The strengthened live UI harness recompiles and remains blocked at real fixture startup ([vault-ui-live-final.log](vault-ui-live-final.log)). Final `rebuild_check` again passes build/startup with original/mechanics comparisons unverified ([report](resume-post-review-rebuild-check.txt)). No counts advance.
