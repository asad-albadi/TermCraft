# Post-review D02 failure inventory — 2026-11-24

Command: `CARGO_BUILD_JOBS=2 cargo test --offline --no-fail-fast` → exit 101. Raw summed result lines: 260 passed / 127 failed / 6 ignored, including child/doc-test result summaries, not checkpoint totals. Fifteen targets fail.

Full output: [resume-post-review-full.log](resume-post-review-full.log). No existing tests or assertions were skipped, ignored or weakened. This inventory supersedes the pre-review inventory for current source; previous records are retained.

Direct EPERM is distinguished from readiness/child wrappers. Indirect failures require individual host reruns; none proves that downstream product assertions pass.

## method in this implementation
...
161 |     fn poll(&mut self) {
    |        ^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: methods `shared_sources_busy` and `refresh_shared_source_with` are never used
   --> src/ui/shared_sessions.rs:684:19
    |
640 | impl Workspace {
    | -------------- methods in this implementation
...
684 |     pub(super) fn shared_sources_busy(&self) -> bool {
    |                   ^^^^^^^^^^^^^^^^^^^
...
999 |     pub(super) fn refresh_shared_source_with(
    |                   ^^^^^^^^^^^^^^^^^^^^^^^^^^

warning: methods `generated_key_paths`, `start_owned_agent`, `add_agent_identity`, and `generate_key_bundle` are never used
   --> src/ui/ssh_keys.rs:126:19
    |
112 | impl Workspace {
    | -------------- methods in this implementation
...
126 |     pub(super) fn generated_key_paths(&self) -> Option<&KeyPairPaths> {
    |                   ^^^^^^^^^^^^^^^^^^^
...
129 |     pub(super) fn start_owned_agent(&mut self, ctx: &Context) {
    |                   ^^^^^^^^^^^^^^^^^
...
132 |     pub(super) fn add_agent_identity(
    |                   ^^^^^^^^^^^^^^^^^^
...
151 |     pub(super) fn generate_key_bundle(
    |                   ^^^^^^^^^^^^^^^^^^^

warning: `harbor-workspace` (bin "harbor-workspace") generated 3 warnings
   Compiling harbor-workspace v0.1.0 (/home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app)
warning: associated items `with_cursor`, `with_resize`, `with_shrink`, and `stop` are never used
   --> tests/support/rdp_server.rs:68:12
    |
 64 | impl Server {
    | ----------- associated items in this implementation
...
 68 |     pub fn with_cursor() -> Self {
    |            ^^^^^^^^^^^
...
 74 |     pub fn with_resize() -> Self {
    |            ^^^^^^^^^^^
...
 77 |     pub fn with_shrink() -> Self {
    |            ^^^^^^^^^^^
...
137 |     pub fn stop(&mut self) {
    |            ^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `harbor-workspace` (test "rdp_options") generated 1 warning
warning: method `restart_with_new_host_key` is never used
   --> tests/support/ssh_server.rs:131:12
    |
 14 | impl SshServer {
    | -------------- method in this implementation
...
131 |     pub fn restart_with_new_host_key(&mut self) {
    |            ^^^^^^^^^^^^^^^^^^^^^^^^^
    |
    = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: `harbor-workspace` (test "ssh_browser") generated 1 warning
warning: variants `PasswordThenMfa` and `ScpStartup` are never constructed
  --> tests/support/auth_server.rs:18:5
   |
16 | pub enum Mode {
   |          ---- variants in this enum
17 |     Password,
18 |     PasswordThenMfa,
   |     ^^^^^^^^^^^^^^^
...
22 |     ScpStartup,
   |     ^^^^^^^^^^
   |
   = note: `Mode` has a derived impl for the trait `Clone`, but this is intentionally ignored during dead code analysis
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: fields `attempts` and `startup` are never read
   --> tests/support/auth_server.rs:271:5
    |
270 | pub struct AuthServer {
    |            ---------- fields in this struct
271 |     attempts: Arc<Attempts>,
    |     ^^^^^^^^
272 |     startup: Arc<StartupState>,
    |     ^^^^^^^

warning: methods `startup_requests`, `active_startup_helpers`, `allow_startup_helpers`, and `attempts` are never used
   --> tests/support/auth_server.rs:345:12
    |
281 | impl AuthServer {
    | --------------- methods in this implementation
...
345 |     pub fn startup_requests(&self) -> usize {
    |            ^^^^^^^^^^^^^^^^
...
348 |     pub fn active_startup_helpers(&self) -> usize {
    |            ^^^^^^^^^^^^^^^^^^^^^^
...
351 |     pub fn allow_startup_helpers(&self) {
    |            ^^^^^^^^^^^^^^^^^^^^^
...
370 |     pub fn attempts(&self) -> [usize; 4] {
    |            ^^^^^^^^

warning: `harbor-workspace` (test "ssh_authentication") generated 3 warnings
warning: methods `to_remote` and `from_remote` are never used
  --> tests/support/../../src/desktop/options.rs:12:12
   |
11 | impl ClipboardPolicy {
   | -------------------- methods in this implementation
12 |     pub fn to_remote(self) -> bool {
   |            ^^^^^^^^^
...
15 |     pub fn from_remote(self) -> bool {
   |            ^^^^^^^^^^^
   |
   = note: `#[warn(dead_code)]` (part of `#[warn(unused)]`) on by default

warning: struct `RdpCapabilities` is never constructed
  --> tests/support/../../src/desktop/options.rs:54:12
   |
54 | pub struct RdpCapabilities {
   |            ^^^^^^^^^^^^^^^

warning: associated function `embedded` is never used
  --> tests/support/../../src/desktop/options.rs:64:12
   |
63 | impl RdpCapabilities {
   | -------------------- associated function in this implementation
64 |     pub fn embedded() -> Self {
   |            ^^^^^^^^

warning: struct `NegotiatedRdpOptions` is never constructed
  --> tests/support/../../src/desktop/options.rs:77:12
   |
77 | pub struct NegotiatedRdpOptions {
   |            ^^^^^^^^^^^^^^^^^^^^

warning: methods `is_default`, `validate`, and `negotiate` are never used
   --> tests/support/../../src/desktop/options.rs:83:12
    |
 82 | impl RdpOptions {
    | --------------- methods in this implementation
 83 |     pub fn is_default(&self) -> bool {
    |            ^^^^^^^^^^
...
 86 |     pub fn validate(&self) -> Result<(), String> {
    |            ^^^^^^^^
...
115 |     pub fn negotiate(
    |            ^^^^^^^^^

warning: `harbor-workspace` (test "rdp_session") generated 5 warnings
    Finished `test` profile [unoptimized + debuginfo] target(s) in 38.79s
     Running unittests src/lib.rs (target/debug/deps/harbor_workspace-94bde7e000a59e92)

running 39 tests
test platform::atomic_rename::tests::atomic_commit_preserves_both_files_on_conflict ... ok
test http_server::tests::rejects_encoded_traversal ... ok
test desktop::queue_tests::local_keyboard_and_view_only_are_enforced_before_queueing_input ... ok
test remote_files::editor::review_tests::earlier_job_completion_cannot_finish_a_new_save ... ok
test remote_files::editor::review_tests::full_document_queue_is_retryable_and_keeps_local_recovery ... ok
test remote_files::transfer::operations::tree::windows_name_tests::windows_download_names_reject_device_aliases_and_path_syntax ... ok
test remote_files::transfer::review_tests::equal_sized_jobs_keep_distinct_immutable_operation_summaries ... ok
test remote_files::transfer::operations::commit::tests::atomic_commit_preserves_both_files_on_conflict ... ok
test desktop::queue_tests::focus_release_is_encoded_even_with_full_input_queue ... ok
test remote_files::transfer::review_tests::bulk_cancellation_waits_for_an_authoritative_entry_and_stops_the_next ... ok
test remote_files::transfer::local_stage::tests::replaced_folder_containers_and_archives_are_never_published_or_cleaned ... ok
test remote_files::transfer::operations::source::cap_backend::tests::capability_backend_moves_files_between_anchored_directories ... ok
test remote_files::transfer::tests::queued_cancellation_is_observable_before_worker_start ... ok
test remote_files::transfer::operations::source::tests::replaced_children_are_not_followed_after_enumeration ... ok
test remote_files::transfer::review_tests::submitted_commits_have_authoritative_or_indeterminate_outcomes ... ok
test ssh::auth::tests::agent_factor_preserves_new_authentication_methods ... ok
test ssh::worker::budget_tests::directory_budget_bounds_retained_bytes_before_allocating_paths ... ok
test remote_files::transfer::folder_archive::portable::tests::capability_folder_encoder_rejects_an_unselected_root_and_escaping_link ... ok
test ssh::host_keys::tests::remembering_a_conflicting_key_cannot_expand_the_pin_set ... ok
test remote_files::transfer::folder_archive::unix::tests::cancelled_folder_preparation_stops_before_copying ... ok
test ssh::x11::tests::substitutes_only_verified_cookie_in_both_byte_orders ... ok
test ssh::x11::tests::authority_requires_matching_display_host_and_protocol ... ok
test remote_files::transfer::operations::source::cap_backend::tests::capability_backend_rejects_swapped_sources_and_destination_links ... ok
test ssh::x11::tests::unauthorized_x11_never_connects_to_local_server ... FAILED
test process::backend::tests::real_shell_uses_pty_and_returns_output ... ok
test remote_files::transfer::folder_archive::portable::windows_decoder_tests::windows_decoder_rejects_device_names_streams_and_unknown_link_types ... ok
test terminal::tests::cursor_edit_and_erase ... ok
test tests::rejects_option_injection_and_bad_hosts ... ok
test ssh::x11::tests::display_parser_rejects_remote_targets_and_port_overflow ... ok
test terminal::tests::alternate_screen_restores_primary ... ok
test terminal::tests::preserves_split_utf8 ... ok
test tests::ssh_preserves_key_paths_without_shell_interpolation ... ok
test terminal::tests::scrolling_keeps_history ... ok
test ssh::x11::tests::rejects_wrong_cookie_and_oversized_handshakes ... ok
test tests::protocol_defaults_match_sessions ... ok
test remote_files::transfer::folder_archive::portable::windows_decoder_tests::portable_decoder_preserves_content_and_resolves_selected_tree_link_types ... ok
test remote_files::transfer::folder_archive::portable::tests::capability_folder_encoder_preserves_bytes_links_names_and_ordinary_modes ... ok
test remote_files::transfer::folder_archive::unix::tests::hostile_folder_payloads_cannot_escape_or_follow_directory_links ... ok
test process::backend::lifecycle_tests::dropping_tab_ends_foreground_and_background_jobs ... ok

failures:

---- ssh::x11::tests::unauthorized_x11_never_connects_to_local_server

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'ssh::x11::tests::unauthorized_x11_never_connects_to_local_server' (3890) panicked at src/ssh/x11.rs:295:67:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## ui::desktop::tests::real_rdp_vault_save_use_delete_and_session_fullscreen

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'ui::desktop::tests::real_rdp_vault_save_use_delete_and_session_fullscreen' (3913) panicked at src/ui/../../tests/support/rdp_server.rs:111:14:
Fixture readiness
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## ui::gui_tests::dirty_remote_editor_blocks_tab_close_and_reconnect

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'ui::gui_tests::dirty_remote_editor_blocks_tab_close_and_reconnect' (3936) panicked at /home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## ui::desktop::tests::real_rdp_surface_paints_inside_workspace_and_routes_focused_input

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'ui::desktop::tests::real_rdp_surface_paints_inside_workspace_and_routes_focused_input' (3912) panicked at src/ui/../../tests/support/rdp_server.rs:111:14:
Fixture readiness
```

## ui::cli_dispatch::tests::forwarded_new_tab_opens_once_in_the_actual_workspace

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'ui::cli_dispatch::tests::forwarded_new_tab_opens_once_in_the_actual_workspace' (3907) panicked at src/ui/cli_dispatch.rs:13:75:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
```

## ui::gui_tests::native_drop_on_local_zone_does_not_upload

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'ui::gui_tests::native_drop_on_local_zone_does_not_upload' (3978) panicked at /home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## ui::gui_tests::new_remote_revision_blocks_stale_active_window_close_approval

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'ui::gui_tests::new_remote_revision_blocks_stale_active_window_close_approval' (3980) panicked at /home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## ui::gui_tests::scp_pane_never_identifies_itself_as_sftp

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'ui::gui_tests::scp_pane_never_identifies_itself_as_sftp' (3995) panicked at /home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## ui::gui_tests::ssh_directory_notices_follow_only_after_opt_in

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'ui::gui_tests::ssh_directory_notices_follow_only_after_opt_in' (4013) panicked at /home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## ui::credential_tests::explicit_proxy_save_links_only_proxy_reference_and_encrypts_both_fields

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'ui::credential_tests::explicit_proxy_save_links_only_proxy_reference_and_encrypts_both_fields' (3909) panicked at src/ui/../../tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## ui::credential_tests::optional_saved_password_stays_encrypted_and_requires_a_matching_live_challenge

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'ui::credential_tests::optional_saved_password_stays_encrypted_and_requires_a_matching_live_challenge' (3910) panicked at src/ui/../../tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## ui::ssh_key_tests::owned_agent_selected_in_the_gui_authenticates_terminal_and_shared_source

Indirect setup/readiness/child assertion. This specific failure remains unresolved; inspect the diagnostic and rerun this named case on the host. Not assumed passing or automatically environmental.

```text
thread 'ui::ssh_key_tests::owned_agent_selected_in_the_gui_authenticates_terminal_and_shared_source' (4186) panicked at src/ui/ssh_key_tests.rs:23:5:
assertion failed: w.owned_agent_endpoint().is_some()
```

## malformed_oversized_and_unauthenticated_ipc_are_rejected

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'malformed_oversized_and_unauthenticated_ipc_are_rejected' (4348) panicked at tests/cli_dispatch.rs:108:75:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## different_data_dirs_do_not_share_commands

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'different_data_dirs_do_not_share_commands' (4344) panicked at tests/cli_dispatch.rs:85:67:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
```

## instance_record_is_private_and_symlinks_are_rejected

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'instance_record_is_private_and_symlinks_are_rejected' (4347) panicked at tests/cli_dispatch.rs:126:71:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
```

## same_workspace_dispatches_exactly_once

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'same_workspace_dispatches_exactly_once' (4349) panicked at tests/cli_dispatch.rs:41:75:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
```

## generated_shortcut_dispatches_through_the_real_binary_without_another_gui

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'generated_shortcut_dispatches_through_the_real_binary_without_another_gui' (4345) panicked at tests/cli_dispatch.rs:150:75:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
```

## redirection_options_match_negotiated_channels

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'redirection_options_match_negotiated_channels' (4398) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## rdp_certificate_change_requires_resolution

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'rdp_certificate_change_requires_resolution' (4421) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## disconnect_retires_sign_in_without_waiting_for_a_password

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'disconnect_retires_sign_in_without_waiting_for_a_password' (4419) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

## rdp_frame_updates_render_inside_workspace

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'rdp_frame_updates_render_inside_workspace' (4422) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

## wheel_input_reaches_remote_desktop

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'wheel_input_reaches_remote_desktop' (4426) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

## cancelling_certificate_prompt_reaps_the_helper

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'cancelling_certificate_prompt_reaps_the_helper' (4417) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

## server_display_resize_renegotiates_the_framebuffer

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'server_display_resize_renegotiates_the_framebuffer' (4424) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

## remote_cursor_and_focus_release_reach_real_connection

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'remote_cursor_and_focus_release_reach_real_connection' (4423) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

## closing_rdp_releases_transport

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'closing_rdp_releases_transport' (4418) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

## queued_pointer_input_survives_a_server_shrink

RDP fixture readiness failure; targeted runs retain child stderr showing EPERM. Real RDP/UI assertions were not reached. Host rerun pending.

```text
thread 'queued_pointer_input_survives_a_server_shrink' (4420) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

## close_cancel_retains_dirty_document

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'close_cancel_retains_dirty_document' (4466) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## a_permission_restriction_during_staging_cannot_be_reversed_by_save

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'a_permission_restriction_during_staging_cannot_be_reversed_by_save' (4465) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## queued_editor_operations_can_be_cancelled_without_closing_ssh

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'queued_editor_operations_can_be_cancelled_without_closing_ssh' (4468) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## failed_save_keeps_recoverable_edits

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'failed_save_keeps_recoverable_edits' (4467) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## reconnect_import_requires_a_fresh_matching_endpoint_and_never_uploads_implicitly

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'reconnect_import_requires_a_fresh_matching_endpoint_and_never_uploads_implicitly' (4469) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## remote_save_conflict_preserves_both_versions

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'remote_save_conflict_preserves_both_versions' (4470) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## unsupported_atomic_replace_cleans_up_owned_remote_staging

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'unsupported_atomic_replace_cleans_up_owned_remote_staging' (4471) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## cancellation_and_conflicts_preserve_existing_destination

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'cancellation_and_conflicts_preserve_existing_destination' (4492) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## cancelled_download_does_not_overwrite_destination

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'cancelled_download_does_not_overwrite_destination' (4493) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## composed_symlinks_cannot_publish_an_escape_from_the_selected_tree

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'composed_symlinks_cannot_publish_an_escape_from_the_selected_tree' (4494) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## disconnect_keeps_original_destination_and_recoverable_partial

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'disconnect_keeps_original_destination_and_recoverable_partial' (4495) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## explicit_folder_overwrite_merges_files_and_preserves_unrelated_entries

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'explicit_folder_overwrite_merges_files_and_preserves_unrelated_entries' (4496) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## failed_local_and_remote_io_preserve_existing_data

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'failed_local_and_remote_io_preserve_existing_data' (4497) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## folder_merge_never_follows_existing_destination_directory_symlinks

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'folder_merge_never_follows_existing_destination_directory_symlinks' (4498) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## folder_merge_retains_modes_without_making_staging_read_only_too_early

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'folder_merge_retains_modes_without_making_staging_read_only_too_early' (4499) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## missing_sftp_rejects_file_jobs_without_stranding_the_queue

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'missing_sftp_rejects_file_jobs_without_stranding_the_queue' (4503) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## injected_disk_full_sync_and_close_errors_preserve_destination_and_partial

Indirect setup/readiness/child assertion. This specific failure remains unresolved; inspect the diagnostic and rerun this named case on the host. Not assumed passing or automatically environmental.

```text
thread 'injected_disk_full_sync_and_close_errors_preserve_destination_and_partial' (4501) panicked at tests/remote_transfers.rs:552:13:
Injected write failure was not safely handled
```

## literal_binary_file_round_trip_uses_sftp

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'literal_binary_file_round_trip_uses_sftp' (4502) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## overlapping_overwrite_jobs_commit_in_submission_order

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'overlapping_overwrite_jobs_commit_in_submission_order' (4504) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## recursive_transfer_preserves_bytes_and_names

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'recursive_transfer_preserves_bytes_and_names' (4508) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## remote_delete_requires_confirmed_target

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'remote_delete_requires_confirmed_target' (4509) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## recursive_delete_is_confirmed_bounded_and_never_follows_symlinks

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'recursive_delete_is_confirmed_bounded_and_never_follows_symlinks' (4506) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## oversized_recursive_delete_stops_before_removing_root_entries

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'oversized_recursive_delete_stops_before_removing_root_entries' (4505) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## repeated_tree_cancellation_releases_bounded_sftp_handles

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'repeated_tree_cancellation_releases_bounded_sftp_handles' (4513) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## symlink_cycle_is_not_followed

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'symlink_cycle_is_not_followed' (4515) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## transfer_preserves_executable_modes_after_staging

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'transfer_preserves_executable_modes_after_staging' (4518) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## completed_scp_staging_does_not_authorize_remote_publication_on_eof

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'completed_scp_staging_does_not_authorize_remote_publication_on_eof' (4558) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## moving_a_local_download_parent_cannot_publish_a_replaced_staging_name

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'moving_a_local_download_parent_cannot_publish_a_replaced_staging_name' (4563) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## explicit_scp_reports_negotiated_endpoint_capabilities

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'explicit_scp_reports_negotiated_endpoint_capabilities' (4559) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## moving_a_remote_upload_parent_cannot_publish_or_clean_up_unrelated_objects

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'moving_a_remote_upload_parent_cannot_publish_or_clean_up_unrelated_objects' (4564) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## cancelling_scp_exec_startup_does_not_exhaust_session_channels

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'cancelling_scp_exec_startup_does_not_exhaust_session_channels' (4557) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## late_scp_conflicts_are_typed_and_keep_complete_remote_recovery

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'late_scp_conflicts_are_typed_and_keep_complete_remote_recovery' (4560) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## scp_entry_management_keeps_confirmation_and_no_replace_semantics

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'scp_entry_management_keeps_confirmation_and_no_replace_semantics' (4566) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## scp_folder_merges_never_follow_existing_destination_directory_links

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'scp_folder_merges_never_follow_existing_destination_directory_links' (4572) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## scp_folder_round_trip_preserves_binary_names_empty_directories_and_cycles

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'scp_folder_round_trip_preserves_binary_names_empty_directories_and_cycles' (4574) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## scp_enumeration_failure_retains_the_remote_reason

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'scp_enumeration_failure_retains_the_remote_reason' (4569) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## scp_names_cannot_be_interpreted_as_commands

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'scp_names_cannot_be_interpreted_as_commands' (4575) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## scp_folder_merge_preserves_unrelated_entries_and_existing_modes

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'scp_folder_merge_preserves_unrelated_entries_and_existing_modes' (4570) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## scp_folder_policies_raw_names_and_modes_survive_round_trip

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'scp_folder_policies_raw_names_and_modes_survive_round_trip' (4573) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## scp_recursive_delete_does_not_follow_links_and_requires_exact_confirmation

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'scp_recursive_delete_does_not_follow_links_and_requires_exact_confirmation' (4581) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## scp_remote_editor_uses_versioned_conflicts_and_local_recovery

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'scp_remote_editor_uses_versioned_conflicts_and_local_recovery' (4584) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## scp_upload_preserves_ordinary_new_and_existing_modes

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'scp_upload_preserves_ordinary_new_and_existing_modes' (4587) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## sftp_disabled_endpoint_supports_explicit_scp

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'sftp_disabled_endpoint_supports_explicit_scp' (4590) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## silent_terminal_does_not_delay_initial_scp_listing

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'silent_terminal_does_not_delay_initial_scp_listing' (4596) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## special_entry_download_is_rejected_before_local_staging

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'special_entry_download_is_rejected_before_local_staging' (4599) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## unsupported_enumerator_reports_capability_error

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'unsupported_enumerator_reports_capability_error' (4606) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## malformed_remote_editor_commit_reply_is_indeterminate

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'malformed_remote_editor_commit_reply_is_indeterminate' (4562) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## virtual_file_sizes_cannot_silently_publish_empty_content

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'virtual_file_sizes_cannot_silently_publish_empty_content' (4611) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## scp_bulk_cancel_finishes_the_current_entry_and_preserves_the_rest

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'scp_bulk_cancel_finishes_the_current_entry_and_preserves_the_rest' (4565) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## lost_remote_commit_acknowledgement_is_indeterminate_after_real_publication

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'lost_remote_commit_acknowledgement_is_indeterminate_after_real_publication' (4561) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## source_sftp_size_limit_never_returns_partial_definitions

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'source_sftp_size_limit_never_returns_partial_definitions' (4670) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## bounded_source_sftp_read_does_not_start_a_terminal_or_execute_login_command

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'bounded_source_sftp_read_does_not_start_a_terminal_or_execute_login_command' (4669) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## shared_sftp_source_uses_saved_embedded_identity_and_verified_host_without_a_shell

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'shared_sftp_source_uses_saved_embedded_identity_and_verified_host_without_a_shell' (4678) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## owned_https_source_checks_certificates_and_keeps_basic_credentials_out_of_cache

Indirect setup/readiness/child assertion. This specific failure remains unresolved; inspect the diagnostic and rerun this named case on the host. Not assumed passing or automatically environmental.

```text
thread 'owned_https_source_checks_certificates_and_keeps_basic_credentials_out_of_cache' (4677) panicked at tests/shared_sources_network.rs:124:40:
called `Result::unwrap()` on an `Err` value: ParseIntError { kind: Empty }
```

## https_redirect_and_oversized_source_are_rejected_without_partial_snapshot

Indirect setup/readiness/child assertion. This specific failure remains unresolved; inspect the diagnostic and rerun this named case on the host. Not assumed passing or automatically environmental.

```text
thread 'https_redirect_and_oversized_source_are_rejected_without_partial_snapshot' (4676) panicked at tests/shared_sources_network.rs:124:40:
called `Result::unwrap()` on an `Err` value: ParseIntError { kind: Empty }
```

## stop_owned_never_kills_system_agent

Indirect setup/readiness/child assertion. This specific failure remains unresolved; inspect the diagnostic and rerun this named case on the host. Not assumed passing or automatically environmental.

```text
thread 'stop_owned_never_kills_system_agent' (4712) panicked at tests/ssh_agent.rs:104:9:
assertion failed: process.try_wait().unwrap().is_none()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## generated_ecdsa_and_rsa_keys_authenticate_with_openssh

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'generated_ecdsa_and_rsa_keys_authenticate_with_openssh' (4706) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## removing_identity_updates_authentication

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'removing_identity_updates_authentication' (4710) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## protected_import_preserves_encryption_and_agent_lifetime_expires

Indirect setup/readiness/child assertion. This specific failure remains unresolved; inspect the diagnostic and rerun this named case on the host. Not assumed passing or automatically environmental.

```text
thread 'protected_import_preserves_encryption_and_agent_lifetime_expires' (4709) panicked at tests/ssh_agent.rs:183:27:
called `Result::unwrap()` on an `Err` value: "Could not listen on the owned agent endpoint"
```

## x11_cookie_forwarding_shares_the_authenticated_connection

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'x11_cookie_forwarding_shares_the_authenticated_connection' (4723) panicked at tests/ssh_authentication.rs:96:67:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## keyboard_interactive_preserves_prompt_echo_flags

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'keyboard_interactive_preserves_prompt_echo_flags' (4720) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## password_auth_and_stalled_sftp_preserve_shell

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'password_auth_and_stalled_sftp_preserve_shell' (4721) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## public_key_then_mfa_uses_updated_methods

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'public_key_then_mfa_uses_updated_methods' (4722) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## exit_reason_survives_a_saturated_event_queue

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'exit_reason_survives_a_saturated_event_queue' (4736) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## cancelling_a_host_prompt_stops_the_connection

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'cancelling_a_host_prompt_stops_the_connection' (4732) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## closed_sftp_channel_can_reopen_without_reauthenticating

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'closed_sftp_channel_can_reopen_without_reauthenticating' (4734) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## bounded_output_poll_preserves_complete_ssh_tail_and_exit

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'bounded_output_poll_preserves_complete_ssh_tail_and_exit' (4731) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## changed_key_prevents_authentication

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'changed_key_prevents_authentication' (4733) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## full_output_queue_cannot_prevent_cancellation

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'full_output_queue_cannot_prevent_cancellation' (4737) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## missing_sftp_keeps_shell_usable

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'missing_sftp_keeps_shell_usable' (4738) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## stalled_shell_write_does_not_block_sftp_navigation

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'stalled_shell_write_does_not_block_sftp_navigation' (4741) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## tab_separated_wildcard_host_pin_is_enforced

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'tab_separated_wildcard_host_pin_is_enforced' (4742) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## embedded_shell_and_browser_share_one_authentication

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'embedded_shell_and_browser_share_one_authentication' (4735) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## unknown_host_can_be_rejected_before_authentication

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'unknown_host_can_be_rejected_before_authentication' (4743) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## single_jump_host_authenticates_each_hop_and_opens_files

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'single_jump_host_authenticates_each_hop_and_opens_files' (4740) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## revoked_host_key_overrides_a_matching_app_pin

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'revoked_host_key_overrides_a_matching_app_pin' (4739) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## credential_titles_identify_the_role_host_and_port

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'credential_titles_identify_the_role_host_and_port' (4774) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## cancelling_encrypted_key_unlock_rejects_late_passphrases

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'cancelling_encrypted_key_unlock_rejects_late_passphrases' (4773) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## cancel_auth_stops_the_entire_attempt

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'cancel_auth_stops_the_entire_attempt' (4772) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## browser_retry_does_not_reauthenticate

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'browser_retry_does_not_reauthenticate' (4771) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## password_then_mfa_uses_the_new_methods_without_repeating_password

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'password_then_mfa_uses_the_new_methods_without_repeating_password' (4778) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## reconnect_discards_old_challenges_and_listings

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'reconnect_discards_old_challenges_and_listings' (4780) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## gateway_credential_title_identifies_the_gateway_before_target_authentication

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'gateway_credential_title_identifies_the_gateway_before_target_authentication' (4776) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## reconnect_rejects_real_old_directory_snapshots

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'reconnect_rejects_real_old_directory_snapshots' (4781) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## prompted_username_identity_survives_fresh_editor_recovery_without_implicit_upload

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'prompted_username_identity_survives_fresh_editor_recovery_without_implicit_upload' (4779) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## password_credential_binding_uses_verified_endpoint_and_typed_method

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'password_credential_binding_uses_verified_endpoint_and_typed_method' (4777) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## recovery_import_preserves_unsaved_destination_edits

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'recovery_import_preserves_unsaved_destination_edits' (4783) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## recovery_from_an_unsuccessful_open_cannot_replace_a_fresh_document

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'recovery_from_an_unsuccessful_open_cannot_replace_a_fresh_document' (4782) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## recovery_never_crosses_a_changed_trust_once_gateway_key

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'recovery_never_crosses_a_changed_trust_once_gateway_key' (4784) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## recovery_never_crosses_a_changed_trust_once_server_key

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'recovery_never_crosses_a_changed_trust_once_server_key' (4785) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## server_disconnect_retires_a_visible_password_prompt_without_an_answer

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'server_disconnect_retires_a_visible_password_prompt_without_an_answer' (4786) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## keepalive_and_proxy_reach_the_transport

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'keepalive_and_proxy_reach_the_transport' (4820) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

## config_cannot_enable_x11_on_source_only_sftp_connection

Indirect setup/readiness/child assertion. This specific failure remains unresolved; inspect the diagnostic and rerun this named case on the host. Not assumed passing or automatically environmental.

```text
thread 'config_cannot_enable_x11_on_source_only_sftp_connection' (4817) panicked at tests/ssh_options.rs:518:9:
assertion failed: status.success()
```

## proxy_authentication_uses_private_typed_prompts_for_http_and_socks

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'proxy_authentication_uses_private_typed_prompts_for_http_and_socks' (4821) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## timeout_covers_silent_target_and_gateway_handshakes

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'timeout_covers_silent_target_and_gateway_handshakes' (4823) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## two_configured_gateways_authenticate_in_order_before_target

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'two_configured_gateways_authenticate_in_order_before_target' (4824) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## unsupported_configuration_exits_before_opening_network_connection

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'unsupported_configuration_exits_before_opening_network_connection' (4826) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

## waiting_for_host_key_answer_pauses_handshake_deadline

Direct EPERM at the reported socket/listener setup location. Listener-capable host rerun required before evaluating downstream behavior.

```text
thread 'waiting_for_host_key_answer_pauses_handshake_deadline' (4828) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```
