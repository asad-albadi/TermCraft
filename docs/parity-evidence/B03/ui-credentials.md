# Managed service UI credential and SSH lifecycle verification

Private implementation: `target/agent-work/service-ui-credentials`, package version
`0.1.0-service-ui-private`. No MAIN production source was modified by this task.
Root must obtain a fresh scoped review before merging `scoped.patch`.
`base-hashes.json` records the exact original and proposed SHA-256 values for the
only two changed files: `src/ui/managed_servers.rs` and
`src/ui/managed_ssh_iperf.rs`. MAIN still matched both bases at handoff.

## Reproduced defect and narrow fix

Pinned egui 0.36.2's TextEdit builder feeds `text.as_str().to_owned()` into its undoer
both before and after input handling (local registry source builder.rs lines
1085 and 1375), even with `.password(true)`. These service fields masked their
text but omitted the undo clearing already used by
`remote_files::auth_field_scoped`.

`red-recovery.log` is meaningful behavioral RED: three tests rendered the actual
FTP, Telnet, and SSH password fields, pointer-focused each, typed a fake fixture
password, let egui create an idle snapshot, zeroized the form value, and rendered
again. Each then loaded the real TextEditState. Calling its cloned undoer's
`undo(empty)` recovered the exact cleared fixture password; all three failed the
explicit no-recovery assertion. No real credential was used or printed.

The production change is a shared password-field wrapper used by both cards. It
clears existing undo state before rendering and clears new snapshots after
rendering, following the established authentication-field behavior. It preserves
masked display and field widths. It uses the same auto-ID allocation sequence as
egui's normal TextEdit. No service protocol or library source changed.

The generic FTP/Telnet Start implementation currently retains the form password;
this task deliberately does not change that pre-existing behavior. The regression
explicitly zeroizes the form to demonstrate the separate undo retention defect.
SSH Start already zeroizes its form password and the lifecycle test verifies it.

## Actual Servers-page integration

`servers_ssh_pointer_start_auth_root_readonly_stop_preserves_local_pty` uses the
real Workspace Servers page and pointer press/release events on Start and Stop.
It selects a temporary share and ephemeral loopback port, then verifies:

- Unknown username and wrong password are rejected; the configured username and
  password authenticate successfully against the actual embedded SSH listener.
- SFTP reads a fixture from the selected share, rejects `../outside`, and rejects
  creation with default read-only access.
- Start clears the SSH form password.
- Stop reaches Stopped, rejects new TCP connections, and allows rebinding the
  exact former listener address.
- A separate actual local PTY produces command output before and after Stop.

The existing B01 pointer test also passed: FTP/TFTP/Telnet real listener lifecycle,
FTP restart, and supervisor ownership/drop behavior. The shared test click helper
now lets wheel smoothing finish before capturing pointer targets. An initial SSH
Stop fixture failure was traced to the scrolling button moving between capture
and press; no production Stop change was made.

## Verification

All Cargo runs used `CARGO_BUILD_JOBS=2`, the shared MAIN target directory, and
`--bin harbor-workspace` with the unique private package version; they produced a
hashed test harness rather than replacing the unversioned application.

- `red-recovery.log`: 0 passed, 3 expected failures, each reporting that undo
  recovered the cleared fixture password.
- `final-targeted.log`: `cargo test --manifest-path
  target/agent-work/service-ui-credentials/Cargo.toml --bin harbor-workspace
  ui::managed_ -- --nocapture`: 5 passed, 0 failed, 139 filtered out.
- `fmt.log`: `rustfmt --edition 2024 --check` on the two proposed files passed,
  exit 0.

Early private-copy compile attempts lacked include-time asset/document/support
fixtures; these were corrected before behavioral RED and are not RED evidence.
Only the requested narrow tests were run. This does not claim a full suite,
native Windows/macOS execution, or completion of the parent's release gate.
