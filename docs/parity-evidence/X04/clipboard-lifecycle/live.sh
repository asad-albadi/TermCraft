#!/usr/bin/env bash
set -eu
w=$(cd "$(dirname "$0")" && pwd)
app=$(realpath "$w/../../../..")
cd "$w/source"
export TERMCRAFT_TEST_XDMCP_PACKAGE="$w/engine-package"
export TERMCRAFT_TEST_XDMCP_FIXTURE="$w/../fixture"
binary=$(sed -n 's/^ *Running unittests src\/lib.rs (\(.*\))$/\1/p' "$w/evidence/library.log")
test -x "$binary"
for name in xdmcp_clipboard::deadline_tests::recovery_clears_owned_parent_selection_after_previous_budget_expires xdmcp_engine::tests::actual_xdm_negotiated_clipboard_both_directions xdmcp_engine::tests::actual_hostname_connect_resolves_fresh_and_manages_authenticated_window xdmcp_engine::tests::manager_wait_honors_cancellation_and_timeout_before_parent_access xdmcp_clipboard::deadline_tests::owned_endpoint_reauthenticates_and_initializes_with_one_deadline_per_epoch xdmcp_clipboard::fixture::actual_owned_bidirectional_clipboard_and_independent_leases xdmcp_engine::tests::actual_xdm_nondefault_port_opens_selected_display_and_cleanup_preserves_host; do
 tests/support/run_xdmcp_isolated.sh "$app/target/x11-arch-package" "$binary" "x11::$name" --exact --ignored --nocapture > "$w/evidence/${name##*::}.log" 2>&1
done
