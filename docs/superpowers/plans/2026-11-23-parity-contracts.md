# Shared contracts for the complete parity plan

These are the approved interface contracts. F01/F02 APIs and fixtures now exist; later interfaces remain owned by their introducing tasks. Each task owns implementing and testing its declared API. Rust signatures omit module prefixes only where the owning task supplies the exact file. Module registration in `src/lib.rs`/`src/ui.rs` and dependency changes in `Cargo.toml`/`Cargo.lock` belong to the introducing task.

## Existing contracts to preserve

- `model::Session`, `Preferences`, `Store::load(&Path) -> Result<Store, String>`, `Store::save(&Path) -> Result<(), String>`; new persisted fields require serde defaults and old-store tests.
- `terminal::Screen::{new,feed,resize,plain}`, `TerminalTab::{send,poll,running}`, `Workspace::{start_with_launch,reconnect_active,close_tab}`.
- `process::Process::{spawn,write,poll,resize}` and Drop behavior; local process transports remain available.
- Penguin branding and both emerald palettes; System mode must continue honoring the native/portal preference.

## Shared types introduced by F01

```rust
pub type ConnectionId = u64;
pub type TerminalId = u64;
pub type WindowId = u64;
pub type SessionId = String;
pub type JobId = u64;
pub type CredentialId = String;
pub type ServiceId = u64;
pub type GameId = String;
pub struct Endpoint { pub host: String, pub port: u16 }
pub type RequestId = u64;
pub type TaskError = String;
pub struct RemotePath(Vec<u8>);
pub enum EntryKind { Directory, File, Symlink, Other }
pub struct RemoteEntry {
    pub path: RemotePath,
    pub display_name: String,
    pub kind: EntryKind,
    pub size: Option<u64>,
}
pub struct DirectorySnapshot {
    pub connection: ConnectionId,
    pub request: RequestId,
    pub path: RemotePath,
    pub entries: Vec<RemoteEntry>,
}
pub struct ConnectionConfig {
    pub id: ConnectionId,
    pub session: crate::model::Session,
    pub app_known_hosts: std::path::PathBuf,
    pub user_known_hosts: Vec<std::path::PathBuf>,
}
pub enum ConnectionCommand {
    Input(Vec<u8>), Resize { cols: u32, rows: u32 },
    List { request: RequestId, path: RemotePath },
}
```

`RemotePath::from_bytes(Vec<u8>) -> Result<Self, TaskError>` rejects NUL, retains other bytes; `child(&self, name: &[u8]) -> Result<Self, TaskError>` accepts one remote basename and never applies host-OS path rules. `as_bytes() -> &[u8]` returns the original bytes; display escaping never changes operation targets. Serialize byte paths losslessly where persistence is needed.

`ssh::ConnectionHandle::connect(config: ConnectionConfig) -> Result<Self, TaskError>` starts background work; `try_send(&self, ConnectionCommand) -> Result<(), TaskError>` never blocks egui; `poll(&mut self) -> Vec<ConnectionEvent>` drains at most 256 events; `cancel(&self)` and Drop cancel independently of full queues. `ConnectionEvent` variants carry connection IDs and include state, redacted challenge, output bytes, directory snapshot, file error, and terminal exit. Challenge response uses a separate bounded channel with a per-challenge ID and cancellation signal; secrets have no Serialize or printable Debug implementation.

Use a 128-command queue, 64-event queue, <=16 KiB terminal output frames, <=1 MiB unsent terminal input, and 10,000-entry directory ceiling with a visible limit error. A listing snapshot must never silently claim completeness after truncation. Zero-output/idle connections still service controls. Every asynchronous result is checked against both connection generation and operation request ID.

## Test fixture contracts

Create fixtures only in `tests/support/`; no test-only cleanup API enters production classes.

- `tests/support/ssh_server.rs`: `SshServer::start(options: ServerOptions) -> Result<Self, String>` owns a temporary root, host/client keys, nonpersistent listener and child processes. `config(id: u64) -> ConnectionConfig`, `write(relative: &str, bytes: &[u8])`, `set_sftp_enabled(bool)`, `accepted_authentications() -> usize`, and Drop cleanup. Defaults: localhost, ephemeral high port, key auth, no writes to user SSH configuration. Start under the current user. A missing fixture prerequisite is an explicit ignored/unavailable test result, not a pass.
- The fixture can reject auth, rotate its host key, withhold SFTP responses, delay forwarding, and close sockets deterministically. For password/keyboard-interactive and packet counters, a controlled in-process protocol server is allowed; at least one OpenSSH interoperability test must exercise the real engine.
- `tests/support/connection_driver.rs`: await a typed event with a deadline while servicing challenges; inspect actual bytes/events and server counters. It cannot synthesize success, bypass host-key checks, or convert an unavailable endpoint into a passing assertion.
- `tests/support/gui.rs`: render actual workspace widgets through egui input, return observable geometry/text/control IDs, and discard only intentionally unused texture deltas. Network events can be injected at the transport boundary; networking has separate real tests.
- `tests/support/endpoints.rs`: owned FTP/FTPS, RFB, RDP, Telnet, tunnel and X11 fixtures as introduced by their owning task. All helpers clean up only their own files/listeners/processes.
- `tests/support/serial.rs`: POSIX PTY pairs for deterministic transport tests; physical-device tests are a distinct opt-in suite with recorded device settings.

## Evidence contract introduced by E01

`EvidenceRecord` in `docs/parity-evidence/schema.json` contains: point ID, target product/version/edition, source URL or capture path, capture date, observed behavior, TermCraft behavior, test command, platform, result (`passed`, `failed`, `unavailable`, `not-applicable`), and remaining limitations. Research tasks record evidence, not fake Rust tests. `not-applicable` needs source evidence and explicit scope review; it is never inferred from a missing web-page mention.

## Owning-task definitions for non-shared types

Each task owns its explicitly named domain types and supporting enums/results in the files it creates. Do not invent a second transport, terminal ID, connection ID or path representation in a later plan. Fields required by the task description are part of its type contract; implementation-private fields are left to the builder.

S01 adds stable `SessionId` values with a backwards-compatible migration: assign IDs to old sessions once, persist them on the next successful save, preserve them across rename/edit, and generate new IDs for duplicates/import-as-new. S03/S04 consume those IDs. Display names are never identity keys.

F01 introduces `ConnectionDriver::new(handle: ConnectionHandle)`, `trust_once() -> Result<(), String>`, `wait_directory(timeout: Duration) -> Result<DirectorySnapshot, String>`, `send(bytes: &[u8]) -> Result<(), String>`, and `wait_output_contains(bytes: &[u8], timeout: Duration) -> Result<Vec<u8>, String>` in `tests/support/connection_driver.rs`. These drive the real transport and wait for actual server events. Other assertion fragments refer to observed fixture output, never values supplied as a fake successful result.

The full plan adds required prerequisites inside their first consumer's task: the evidence record fields can be recorded before E01 writes the formal JSON schema, and runtime/package manifests can be introduced by X01 before P08 assembles installers. Later validation tasks consolidate evidence instead of preventing earlier regression tests.

## Cross-plan completion contract

Every code task first adds its named behavioral test, runs it to observe the expected failure, implements the declared API and UI integration, then runs the targeted tests and the whole `cargo test --offline` suite. Update `PARITY_PROGRESS.md` after each task with commands and results. Update `PARITY_CHECKLIST.md` and `FEATURE_PARITY.md` only to the level the evidence supports. An implementation can be complete while the checklist outcome remains pending native-platform/original-runtime validation.

No Git repository is present. Until one exists, record verified file changes and test output in the progress log; do not claim commits, branches, worktrees, or a clean Git diff. Creating a repository is not a hidden prerequisite.

S01 storage contract: background import Apply owns a staged Store snapshot and blocks UI settings changes until its save result publishes. Subsequent asynchronous refresh/IPC producers must defer Store mutations while Workspace::import_applying() is true, then resume after commit. Checking only persist() after mutating memory is insufficient. Read cancellation discards its source-bound result; an explicitly started Apply completes before app close.
