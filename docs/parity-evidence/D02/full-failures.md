# Current full-suite failures

Command: `CARGO_BUILD_JOBS=2 cargo test --offline --no-fail-fast` (exit 101).

Raw summed result lines: 256 passed, 126 failed, 6 ignored; nested child tests can be counted twice. This is not a passing suite or a comparison with the historical top-level count.

The runner denies loopback and Unix listeners. Most failures report EPERM directly; the RDP/HTTPS/agent fixtures and nested SSH fault/config tests also fail during startup. These remain failures requiring rerun, not passing or ignored tests. Full diagnostics and failed-target list are in full-final.log.

### ssh::x11::tests::unauthorized_x11_never_connects_to_local_server

```text
thread 'ssh::x11::tests::unauthorized_x11_never_connects_to_local_server' (73) panicked at src/ssh/x11.rs:295:67:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### ui::gui_tests::dirty_remote_editor_blocks_tab_close_and_reconnect

```text
thread 'ui::gui_tests::dirty_remote_editor_blocks_tab_close_and_reconnect' (101) panicked at /home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### ui::cli_dispatch::tests::forwarded_new_tab_opens_once_in_the_actual_workspace

```text
thread 'ui::cli_dispatch::tests::forwarded_new_tab_opens_once_in_the_actual_workspace' (89) panicked at src/ui/cli_dispatch.rs:13:75:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
```

### ui::gui_tests::native_drop_on_local_zone_does_not_upload

```text
thread 'ui::gui_tests::native_drop_on_local_zone_does_not_upload' (256) panicked at /home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### ui::gui_tests::new_remote_revision_blocks_stale_active_window_close_approval

```text
thread 'ui::gui_tests::new_remote_revision_blocks_stale_active_window_close_approval' (263) panicked at /home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### ui::gui_tests::scp_pane_never_identifies_itself_as_sftp

```text
thread 'ui::gui_tests::scp_pane_never_identifies_itself_as_sftp' (268) panicked at /home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### ui::gui_tests::ssh_directory_notices_follow_only_after_opt_in

```text
thread 'ui::gui_tests::ssh_directory_notices_follow_only_after_opt_in' (307) panicked at /home/asad/Documents/Craft Anything/mobaxterm-personal-26-6/app/tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### ui::credential_tests::explicit_proxy_save_links_only_proxy_reference_and_encrypts_both_fields

```text
thread 'ui::credential_tests::explicit_proxy_save_links_only_proxy_reference_and_encrypts_both_fields' (91) panicked at src/ui/../../tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### ui::credential_tests::optional_saved_password_stays_encrypted_and_requires_a_matching_live_challenge

```text
thread 'ui::credential_tests::optional_saved_password_stays_encrypted_and_requires_a_matching_live_challenge' (92) panicked at src/ui/../../tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### ui::ssh_key_tests::owned_agent_selected_in_the_gui_authenticates_terminal_and_shared_source

```text
thread 'ui::ssh_key_tests::owned_agent_selected_in_the_gui_authenticates_terminal_and_shared_source' (518) panicked at src/ui/ssh_key_tests.rs:23:5:
assertion failed: w.owned_agent_endpoint().is_some()
```

### ui::desktop::tests::real_rdp_surface_paints_inside_workspace_and_routes_focused_input

```text
thread 'ui::desktop::tests::real_rdp_surface_paints_inside_workspace_and_routes_focused_input' (94) panicked at src/ui/../../tests/support/rdp_server.rs:111:14:
Fixture readiness
```

### malformed_oversized_and_unauthenticated_ipc_are_rejected

```text
thread 'malformed_oversized_and_unauthenticated_ipc_are_rejected' (1023) panicked at tests/cli_dispatch.rs:108:75:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
```

### instance_record_is_private_and_symlinks_are_rejected

```text
thread 'instance_record_is_private_and_symlinks_are_rejected' (1022) panicked at tests/cli_dispatch.rs:126:71:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### generated_shortcut_dispatches_through_the_real_binary_without_another_gui

```text
thread 'generated_shortcut_dispatches_through_the_real_binary_without_another_gui' (1020) panicked at tests/cli_dispatch.rs:150:75:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
```

### different_data_dirs_do_not_share_commands

```text
thread 'different_data_dirs_do_not_share_commands' (1019) panicked at tests/cli_dispatch.rs:85:67:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
```

### same_workspace_dispatches_exactly_once

```text
thread 'same_workspace_dispatches_exactly_once' (1024) panicked at tests/cli_dispatch.rs:41:75:
called `Result::unwrap()` on an `Err` value: "Operation not permitted (os error 1)"
```

### redirection_options_match_negotiated_channels

```text
thread 'redirection_options_match_negotiated_channels' (1067) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### cancelling_certificate_prompt_reaps_the_helper

```text
thread 'cancelling_certificate_prompt_reaps_the_helper' (1086) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### rdp_certificate_change_requires_resolution

```text
thread 'rdp_certificate_change_requires_resolution' (1090) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

### queued_pointer_input_survives_a_server_shrink

```text
thread 'queued_pointer_input_survives_a_server_shrink' (1089) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

### server_display_resize_renegotiates_the_framebuffer

```text
thread 'server_display_resize_renegotiates_the_framebuffer' (1093) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

### closing_rdp_releases_transport

```text
thread 'closing_rdp_releases_transport' (1087) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

### remote_cursor_and_focus_release_reach_real_connection

```text
thread 'remote_cursor_and_focus_release_reach_real_connection' (1092) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

### rdp_frame_updates_render_inside_workspace

```text
thread 'rdp_frame_updates_render_inside_workspace' (1091) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

### wheel_input_reaches_remote_desktop

```text
thread 'wheel_input_reaches_remote_desktop' (1094) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

### disconnect_retires_sign_in_without_waiting_for_a_password

```text
thread 'disconnect_retires_sign_in_without_waiting_for_a_password' (1088) panicked at tests/support/rdp_server.rs:111:14:
Fixture readiness
```

### queued_editor_operations_can_be_cancelled_without_closing_ssh

```text
thread 'queued_editor_operations_can_be_cancelled_without_closing_ssh' (1137) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### failed_save_keeps_recoverable_edits

```text
thread 'failed_save_keeps_recoverable_edits' (1136) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### close_cancel_retains_dirty_document

```text
thread 'close_cancel_retains_dirty_document' (1135) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### a_permission_restriction_during_staging_cannot_be_reversed_by_save

```text
thread 'a_permission_restriction_during_staging_cannot_be_reversed_by_save' (1134) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### remote_save_conflict_preserves_both_versions

```text
thread 'remote_save_conflict_preserves_both_versions' (1139) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### reconnect_import_requires_a_fresh_matching_endpoint_and_never_uploads_implicitly

```text
thread 'reconnect_import_requires_a_fresh_matching_endpoint_and_never_uploads_implicitly' (1138) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### unsupported_atomic_replace_cleans_up_owned_remote_staging

```text
thread 'unsupported_atomic_replace_cleans_up_owned_remote_staging' (1140) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### cancellation_and_conflicts_preserve_existing_destination

```text
thread 'cancellation_and_conflicts_preserve_existing_destination' (1161) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### explicit_folder_overwrite_merges_files_and_preserves_unrelated_entries

```text
thread 'explicit_folder_overwrite_merges_files_and_preserves_unrelated_entries' (1165) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### folder_merge_never_follows_existing_destination_directory_symlinks

```text
thread 'folder_merge_never_follows_existing_destination_directory_symlinks' (1167) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### cancelled_download_does_not_overwrite_destination

```text
thread 'cancelled_download_does_not_overwrite_destination' (1162) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### folder_merge_retains_modes_without_making_staging_read_only_too_early

```text
thread 'folder_merge_retains_modes_without_making_staging_read_only_too_early' (1168) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### composed_symlinks_cannot_publish_an_escape_from_the_selected_tree

```text
thread 'composed_symlinks_cannot_publish_an_escape_from_the_selected_tree' (1163) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### injected_disk_full_sync_and_close_errors_preserve_destination_and_partial

```text
thread 'injected_disk_full_sync_and_close_errors_preserve_destination_and_partial' (1169) panicked at tests/remote_transfers.rs:552:13:
Injected write failure was not safely handled
```

### disconnect_keeps_original_destination_and_recoverable_partial

```text
thread 'disconnect_keeps_original_destination_and_recoverable_partial' (1164) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### missing_sftp_rejects_file_jobs_without_stranding_the_queue

```text
thread 'missing_sftp_rejects_file_jobs_without_stranding_the_queue' (1172) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### recursive_delete_is_confirmed_bounded_and_never_follows_symlinks

```text
thread 'recursive_delete_is_confirmed_bounded_and_never_follows_symlinks' (1176) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### recursive_transfer_preserves_bytes_and_names

```text
thread 'recursive_transfer_preserves_bytes_and_names' (1177) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### remote_delete_requires_confirmed_target

```text
thread 'remote_delete_requires_confirmed_target' (1178) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### failed_local_and_remote_io_preserve_existing_data

```text
thread 'failed_local_and_remote_io_preserve_existing_data' (1166) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### overlapping_overwrite_jobs_commit_in_submission_order

```text
thread 'overlapping_overwrite_jobs_commit_in_submission_order' (1174) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### literal_binary_file_round_trip_uses_sftp

```text
thread 'literal_binary_file_round_trip_uses_sftp' (1170) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### oversized_recursive_delete_stops_before_removing_root_entries

```text
thread 'oversized_recursive_delete_stops_before_removing_root_entries' (1175) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### repeated_tree_cancellation_releases_bounded_sftp_handles

```text
thread 'repeated_tree_cancellation_releases_bounded_sftp_handles' (1181) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### symlink_cycle_is_not_followed

```text
thread 'symlink_cycle_is_not_followed' (1184) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### transfer_preserves_executable_modes_after_staging

```text
thread 'transfer_preserves_executable_modes_after_staging' (1189) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### completed_scp_staging_does_not_authorize_remote_publication_on_eof

```text
thread 'completed_scp_staging_does_not_authorize_remote_publication_on_eof' (1227) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### cancelling_scp_exec_startup_does_not_exhaust_session_channels

```text
thread 'cancelling_scp_exec_startup_does_not_exhaust_session_channels' (1226) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### explicit_scp_reports_negotiated_endpoint_capabilities

```text
thread 'explicit_scp_reports_negotiated_endpoint_capabilities' (1228) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### late_scp_conflicts_are_typed_and_keep_complete_remote_recovery

```text
thread 'late_scp_conflicts_are_typed_and_keep_complete_remote_recovery' (1229) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### moving_a_remote_upload_parent_cannot_publish_or_clean_up_unrelated_objects

```text
thread 'moving_a_remote_upload_parent_cannot_publish_or_clean_up_unrelated_objects' (1233) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### moving_a_local_download_parent_cannot_publish_a_replaced_staging_name

```text
thread 'moving_a_local_download_parent_cannot_publish_a_replaced_staging_name' (1232) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### scp_folder_merges_never_follow_existing_destination_directory_links

```text
thread 'scp_folder_merges_never_follow_existing_destination_directory_links' (1241) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### scp_names_cannot_be_interpreted_as_commands

```text
thread 'scp_names_cannot_be_interpreted_as_commands' (1245) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### scp_folder_round_trip_preserves_binary_names_empty_directories_and_cycles

```text
thread 'scp_folder_round_trip_preserves_binary_names_empty_directories_and_cycles' (1243) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### scp_folder_merge_preserves_unrelated_entries_and_existing_modes

```text
thread 'scp_folder_merge_preserves_unrelated_entries_and_existing_modes' (1240) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### scp_folder_policies_raw_names_and_modes_survive_round_trip

```text
thread 'scp_folder_policies_raw_names_and_modes_survive_round_trip' (1242) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### scp_entry_management_keeps_confirmation_and_no_replace_semantics

```text
thread 'scp_entry_management_keeps_confirmation_and_no_replace_semantics' (1237) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### scp_enumeration_failure_retains_the_remote_reason

```text
thread 'scp_enumeration_failure_retains_the_remote_reason' (1239) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### scp_recursive_delete_does_not_follow_links_and_requires_exact_confirmation

```text
thread 'scp_recursive_delete_does_not_follow_links_and_requires_exact_confirmation' (1250) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### scp_remote_editor_uses_versioned_conflicts_and_local_recovery

```text
thread 'scp_remote_editor_uses_versioned_conflicts_and_local_recovery' (1253) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### scp_upload_preserves_ordinary_new_and_existing_modes

```text
thread 'scp_upload_preserves_ordinary_new_and_existing_modes' (1256) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### sftp_disabled_endpoint_supports_explicit_scp

```text
thread 'sftp_disabled_endpoint_supports_explicit_scp' (1260) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### silent_terminal_does_not_delay_initial_scp_listing

```text
thread 'silent_terminal_does_not_delay_initial_scp_listing' (1265) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### special_entry_download_is_rejected_before_local_staging

```text
thread 'special_entry_download_is_rejected_before_local_staging' (1275) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### scp_bulk_cancel_finishes_the_current_entry_and_preserves_the_rest

```text
thread 'scp_bulk_cancel_finishes_the_current_entry_and_preserves_the_rest' (1236) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### lost_remote_commit_acknowledgement_is_indeterminate_after_real_publication

```text
thread 'lost_remote_commit_acknowledgement_is_indeterminate_after_real_publication' (1230) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### malformed_remote_editor_commit_reply_is_indeterminate

```text
thread 'malformed_remote_editor_commit_reply_is_indeterminate' (1231) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### unsupported_enumerator_reports_capability_error

```text
thread 'unsupported_enumerator_reports_capability_error' (1277) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### virtual_file_sizes_cannot_silently_publish_empty_content

```text
thread 'virtual_file_sizes_cannot_silently_publish_empty_content' (1280) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### bounded_source_sftp_read_does_not_start_a_terminal_or_execute_login_command

```text
thread 'bounded_source_sftp_read_does_not_start_a_terminal_or_execute_login_command' (1338) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### source_sftp_size_limit_never_returns_partial_definitions

```text
thread 'source_sftp_size_limit_never_returns_partial_definitions' (1339) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### shared_sftp_source_uses_saved_embedded_identity_and_verified_host_without_a_shell

```text
thread 'shared_sftp_source_uses_saved_embedded_identity_and_verified_host_without_a_shell' (1347) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### owned_https_source_checks_certificates_and_keeps_basic_credentials_out_of_cache

```text
thread 'owned_https_source_checks_certificates_and_keeps_basic_credentials_out_of_cache' (1346) panicked at tests/shared_sources_network.rs:124:40:
called `Result::unwrap()` on an `Err` value: ParseIntError { kind: Empty }
```

### https_redirect_and_oversized_source_are_rejected_without_partial_snapshot

```text
thread 'https_redirect_and_oversized_source_are_rejected_without_partial_snapshot' (1345) panicked at tests/shared_sources_network.rs:124:40:
called `Result::unwrap()` on an `Err` value: ParseIntError { kind: Empty }
```

### stop_owned_never_kills_system_agent

```text
thread 'stop_owned_never_kills_system_agent' (1380) panicked at tests/ssh_agent.rs:104:9:
assertion failed: process.try_wait().unwrap().is_none()
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### generated_ecdsa_and_rsa_keys_authenticate_with_openssh

```text
thread 'generated_ecdsa_and_rsa_keys_authenticate_with_openssh' (1375) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### removing_identity_updates_authentication

```text
thread 'removing_identity_updates_authentication' (1379) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### protected_import_preserves_encryption_and_agent_lifetime_expires

```text
thread 'protected_import_preserves_encryption_and_agent_lifetime_expires' (1378) panicked at tests/ssh_agent.rs:183:27:
called `Result::unwrap()` on an `Err` value: "Could not listen on the owned agent endpoint"
```

### x11_cookie_forwarding_shares_the_authenticated_connection

```text
thread 'x11_cookie_forwarding_shares_the_authenticated_connection' (1392) panicked at tests/ssh_authentication.rs:96:67:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### password_auth_and_stalled_sftp_preserve_shell

```text
thread 'password_auth_and_stalled_sftp_preserve_shell' (1390) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### public_key_then_mfa_uses_updated_methods

```text
thread 'public_key_then_mfa_uses_updated_methods' (1391) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### keyboard_interactive_preserves_prompt_echo_flags

```text
thread 'keyboard_interactive_preserves_prompt_echo_flags' (1389) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### cancelling_a_host_prompt_stops_the_connection

```text
thread 'cancelling_a_host_prompt_stops_the_connection' (1401) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### bounded_output_poll_preserves_complete_ssh_tail_and_exit

```text
thread 'bounded_output_poll_preserves_complete_ssh_tail_and_exit' (1400) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### changed_key_prevents_authentication

```text
thread 'changed_key_prevents_authentication' (1402) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### exit_reason_survives_a_saturated_event_queue

```text
thread 'exit_reason_survives_a_saturated_event_queue' (1405) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### closed_sftp_channel_can_reopen_without_reauthenticating

```text
thread 'closed_sftp_channel_can_reopen_without_reauthenticating' (1403) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### embedded_shell_and_browser_share_one_authentication

```text
thread 'embedded_shell_and_browser_share_one_authentication' (1404) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### full_output_queue_cannot_prevent_cancellation

```text
thread 'full_output_queue_cannot_prevent_cancellation' (1406) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### missing_sftp_keeps_shell_usable

```text
thread 'missing_sftp_keeps_shell_usable' (1407) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### single_jump_host_authenticates_each_hop_and_opens_files

```text
thread 'single_jump_host_authenticates_each_hop_and_opens_files' (1409) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### revoked_host_key_overrides_a_matching_app_pin

```text
thread 'revoked_host_key_overrides_a_matching_app_pin' (1408) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### stalled_shell_write_does_not_block_sftp_navigation

```text
thread 'stalled_shell_write_does_not_block_sftp_navigation' (1411) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### tab_separated_wildcard_host_pin_is_enforced

```text
thread 'tab_separated_wildcard_host_pin_is_enforced' (1412) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### unknown_host_can_be_rejected_before_authentication

```text
thread 'unknown_host_can_be_rejected_before_authentication' (1413) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### browser_retry_does_not_reauthenticate

```text
thread 'browser_retry_does_not_reauthenticate' (1440) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### cancelling_encrypted_key_unlock_rejects_late_passphrases

```text
thread 'cancelling_encrypted_key_unlock_rejects_late_passphrases' (1442) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### cancel_auth_stops_the_entire_attempt

```text
thread 'cancel_auth_stops_the_entire_attempt' (1441) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### credential_titles_identify_the_role_host_and_port

```text
thread 'credential_titles_identify_the_role_host_and_port' (1443) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### password_credential_binding_uses_verified_endpoint_and_typed_method

```text
thread 'password_credential_binding_uses_verified_endpoint_and_typed_method' (1445) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### gateway_credential_title_identifies_the_gateway_before_target_authentication

```text
thread 'gateway_credential_title_identifies_the_gateway_before_target_authentication' (1444) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### password_then_mfa_uses_the_new_methods_without_repeating_password

```text
thread 'password_then_mfa_uses_the_new_methods_without_repeating_password' (1446) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### prompted_username_identity_survives_fresh_editor_recovery_without_implicit_upload

```text
thread 'prompted_username_identity_survives_fresh_editor_recovery_without_implicit_upload' (1447) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### reconnect_discards_old_challenges_and_listings

```text
thread 'reconnect_discards_old_challenges_and_listings' (1449) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### reconnect_rejects_real_old_directory_snapshots

```text
thread 'reconnect_rejects_real_old_directory_snapshots' (1450) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### recovery_from_an_unsuccessful_open_cannot_replace_a_fresh_document

```text
thread 'recovery_from_an_unsuccessful_open_cannot_replace_a_fresh_document' (1451) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### recovery_import_preserves_unsaved_destination_edits

```text
thread 'recovery_import_preserves_unsaved_destination_edits' (1452) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### recovery_never_crosses_a_changed_trust_once_gateway_key

```text
thread 'recovery_never_crosses_a_changed_trust_once_gateway_key' (1453) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### recovery_never_crosses_a_changed_trust_once_server_key

```text
thread 'recovery_never_crosses_a_changed_trust_once_server_key' (1454) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### server_disconnect_retires_a_visible_password_prompt_without_an_answer

```text
thread 'server_disconnect_retires_a_visible_password_prompt_without_an_answer' (1455) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### keepalive_and_proxy_reach_the_transport

```text
thread 'keepalive_and_proxy_reach_the_transport' (1489) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace
```

### config_cannot_enable_x11_on_source_only_sftp_connection

```text
thread 'config_cannot_enable_x11_on_source_only_sftp_connection' (1486) panicked at tests/ssh_options.rs:518:9:
assertion failed: status.success()
```

### proxy_authentication_uses_private_typed_prompts_for_http_and_socks

```text
thread 'proxy_authentication_uses_private_typed_prompts_for_http_and_socks' (1490) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### timeout_covers_silent_target_and_gateway_handshakes

```text
thread 'timeout_covers_silent_target_and_gateway_handshakes' (1492) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### two_configured_gateways_authenticate_in_order_before_target

```text
thread 'two_configured_gateways_authenticate_in_order_before_target' (1494) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### unsupported_configuration_exits_before_opening_network_connection

```text
thread 'unsupported_configuration_exits_before_opening_network_connection' (1495) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```

### waiting_for_host_key_answer_pauses_handshake_deadline

```text
thread 'waiting_for_host_key_answer_pauses_handshake_deadline' (1498) panicked at tests/support/ssh_server.rs:44:57:
called `Result::unwrap()` on an `Err` value: Os { code: 1, kind: PermissionDenied, message: "Operation not permitted" }
```
