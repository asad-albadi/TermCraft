# V01/V02 implementation checkpoint handoff

Private tree: `/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/target/agent-work/v01-v02`.

MAIN was never edited. Private Cargo package version is `0.1.0-v01v02` and private normal-bin name is `termcraft-v01v02-private`; neither Cargo identity change is included in the patch. Every Cargo invocation used `CARGO_BUILD_JOBS=2` and the shared ROOT target directory. ROOT owns fresh review, integration, full suite and any count/checklist advancement.

## Results

- V01: **24 actual runtime matrix cells passed; native Windows and macOS are unavailable**. Includes installed OpenSSH 10.6p2; pinned controlled russh 0.65.4 password/MFA fixture; privately built independent upstream Dropbear 2026.94. Existing real transport assertions were inventoried and reused rather than duplicated. New typed InteropCase coverage checks validate endpoint/auth/trust/jump/operation declarations and source-test references.
- V02: **11 actual runtime matrix cells passed; native Windows and macOS are unavailable**. Exact nine-byte TUNNEL_OK through local/remote/dynamic IPv4/IPv6, SOCKS5 CONNECT, explicit 127.0.0.2 bind in all modes, actual listener-table wildcard rejection, bind-conflict terminal diagnostics/process exit, destination refusal, close with live channel, listener rebind and fresh-process reconnect twice using the saved Tunnel config.
- Runner: 4 Python contract tests passed; zero tests, ignored tests and unsuccessful exits cannot pass a cell. Missing prerequisites/staged independent endpoint become explicit unavailable EvidenceRecords. Every final record's required schema fields/result/source existence/log digest was checked; jsonschema package was unavailable, so no claim of full third-party schema validation.
- Coverage checks: final exact Rust test executable reports 2 passed, 1 explicit prerequisite ignored. The matrix runner separately executes that ignored independent test with --ignored and requires `1 passed; 0 failed; 0 ignored`.
- No product defect or production source change was needed. New fixtures first observed mismatched SOCKS-refusal semantics and an unsupported Dropbear -E flag when syslog is compiled out, then corrected those fixture expectations/options and passed. Original RED logs remain available.

## Actual commands and retained evidence

All commands below ran in the private tree, with `CARGO_TARGET_DIR='/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/target'` and `CARGO_BUILD_JOBS=2` for Cargo and matrix runners.

1. `python3 scripts/test_interop_runner.py` → 4 passed; runner missing-function RED logs and GREEN log in V01.
2. `cargo test --offline --test tunnel_interop --no-run` → compiled unique private normal-bin and test; initial build log in V02.
3. `python3 scripts/check-interop.py V02 --output docs/parity-evidence/V02/run-linux-final` → 11 passed / 2 unavailable; exit 0. Per-cell exact executable commands, executable SHA256, test observations, private OpenSSH stderr and hashed EvidenceRecords retained.
4. `TERMCRAFT_DROPBEAR_DIR="$PWD/private-tools/dropbear/dropbear-2026.94" scripts/prepare-dropbear-interop.sh` → successful private staging and build; detached signature verifies fingerprint F7347EF2EE2E07A267628CA944931494F29C6773; archive SHA256 e098034a843699200c8c977a991fff73159735bf795d5f72ef672c41a6b1ae81. Repro script itself was executed after equivalent manual staging; all three endpoint/helper binary hashes remained identical, so no unchanged runtime rerun was needed.
5. `TERMCRAFT_DROPBEAR_DIR="$PWD/private-tools/dropbear/dropbear-2026.94" python3 scripts/check-interop.py V01 --output docs/parity-evidence/V01/run-linux-final` → 24 passed / 2 unavailable; exit 0. Exact per-cell commands and executable hashes retained.
6. Final retained `ssh_interop-402819b4a2c8008c --nocapture --test-threads=1` → 2 manifest checks passed, 1 explicit prerequisite ignore; `contracts-final.log`.
7. `sh -n scripts/check-ssh-interop.sh scripts/check-tunnels.sh scripts/prepare-dropbear-interop.sh`; `python3 -m py_compile scripts/check-interop.py scripts/test_interop_runner.py` → exit 0.

Private downloaded source archive/tree are in `private-tools/dropbear/`; retained detached signature, official signing key, verified status output, notices, configure output, staging output and archive/binary hashes are in `docs/parity-evidence/V01/dropbear/`. Official primary source: https://matt.ucc.asn.au/dropbear/dropbear.html and https://matt.ucc.asn.au/dropbear/releases/dropbear-2026.94.tar.bz2 . Pinned official-site signing key verifies release integrity; no independent web-of-trust certification is claimed.

## Scope and cleanup

Dropbear uses its supported upstream -D option to an owned authorized_keys directory. No home SSH/account files were edited. Password/PAM and all login accounting are disabled. Its SFTP subsystem is explicitly the installed external OpenSSH `/usr/lib/ssh/sftp-server`; this is independent SSH-server interoperability, not an independent SFTP implementation claim. The fixture lives below the private checkout so strict Dropbear parent permissions remain enabled. All endpoint listeners are explicit loopback; no LAN or host network changes occurred. Private GnuPG sessions were shut down.

Tunnel tests use the exact production Tunnel::args → Process::spawn path used by Tunnels UI. Drop tests use the actual tab/app lifetime primitive; rendered GUI close actions are not claimed. Source inspection confirms Workspace reconnect retains the launch override; existing `reconnect_keeps_tool_program_and_arguments` is ROOT-owned existing coverage. No separate embedded tunnel/GatewayLease implementation was invented.

Native Windows/macOS, rendered GUI close actions and original Personal 26.6 workflow comparison remain separate open evidence. No V01/V02 parity or implementation count is advanced here. No private whole-app rerun was performed: ROOT owns that gate after review. All submitted exact binaries are independently copied under `handoff/binaries/` with paths and SHA256 in `handoff/executable-sha256.json`.

## Scoped review fix round 1 — current controlling evidence

The independent review found two Important issues in the initial candidate. Both are addressed privately; V02 source/evidence is unchanged. Initial staging commands/paths above describe the earlier candidate and are superseded for current rerunnable staging by this section.

1. `encrypted-key-agent` now selects `encrypted_key_agent_authenticates_pinned_openssh_then_expires` in `tests/ssh_interop.rs`. RED observed a real owned OpenSSH daemon’s authentication counter at **0 instead of required 1** when no authentication was performed. GREEN imports encrypted key bytes unchanged, adds that identity to an owned agent for two seconds, clears file-key fallback, selects Owned mode, authenticates a strict pinned ConnectionHandle, checks one observed daemon authentication, awaits actual agent expiry, rejects a fresh connection without any credential prompt, and checks the daemon remains at one authentication. The old `protected_import_preserves_encryption_and_agent_lifetime_expires` assertion is accurately labeled **local-only supplementary coverage** in `supplementary-coverage.json` and is no longer presented as an endpoint observation.
2. Dropbear preparation now always builds a **new extraction of the just-verified pinned signed archive** in an owned `.stages` directory. It executes no source/build file from the previous destination. After success it atomically replaces only a managed destination symlink below this checkout’s `target/interop-tools`. Existing unmanaged directories or symlinks outside owned stages are refused; no arbitrary environment-provided tree is deleted. Previous managed stages remain retained. A test first reproduced execution of a tampered prior configure script (RED); GREEN proves that configure never executes, previous injected source/object files remain untouched, and a fresh signed tree successfully builds/publishes. A separate regression proves unmanaged destinations are preserved.

Scoped verification only:

- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=<ROOT>/target cargo test --offline --test ssh_interop encrypted_key_agent_authenticates_pinned_openssh_then_expires -- --exact --nocapture` → observed behavioral RED, counter 0 vs 1.
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=<ROOT>/target python3 scripts/check-interop.py V01 --case encrypted-key-agent --output docs/parity-evidence/V01/review-fix-round1/encrypted-agent-green` → actual endpoint GREEN, 1 passed / 0 ignored.
- `python3 scripts/test_dropbear_staging.py` → original RED, preexisting unverified configure executed; fixed GREEN, fresh signed-source build and atomic publication asserted (1 passed, 24.5s).
- `python3 scripts/test_dropbear_staging.py FreshSignedStaging.test_unmanaged_destination_is_refused_without_changes` → 1 passed.
- `scripts/prepare-dropbear-interop.sh` with default path → successful fresh verified build/publication; current exact stage is recorded in refreshed `dropbear/provenance.json`.
- `CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=<ROOT>/target python3 scripts/check-interop.py V01 --case independent-dropbear --output docs/parity-evidence/V01/review-fix-round1/independent-green` → actual freshly staged independent endpoint GREEN, 1 passed / 0 ignored.
- Exact final executable metadata tests `ssh_interop_auth_and_host_keys` and `sftp_interop_failure_and_reconnect` → each 1 passed. Python runner contracts → 4 passed. Shell syntax → passed.

Only the two affected aggregate V01 records/logs were regenerated; **24 passed / 2 native unavailable** remains the aggregate. No unrelated endpoint or tunnel suite was rerun. Current normal ssh_interop harness has 3 runnable tests and 1 explicit independent-fixture prerequisite ignore; the endpoint matrix still explicitly executes that ignored case. Exact current ssh_interop executable copy/hash was refreshed; unrelated executable copies and all V02 artifacts remain unchanged. No production source/checklist/progress/count changes were made. ROOT owns scoped re-review and the eventual whole-app gate.
