# F06 independent review and fix-pass record

Reviewer: `/root/f06_review`, read-only, fresh context, gpt-6.2-astra/high. No Git history is present; review used the declared F06 change boundary, current files, plan and supplied test logs. No re-review dispatched.

Five Important findings, no Critical findings:

1. A password prompt could outlive a disconnected SSH transport (`src/ssh/auth.rs`, original line 33). Fixed by racing connected credential input against the transport future and retiring lifecycle state. `server_disconnect_retires_a_visible_password_prompt_without_an_answer`: real TCP relay disconnect, RED→GREEN.
2. Gateway/target credential labels omitted role and port (`src/ssh/auth.rs`, original line 238; `worker.rs`, original line 350). Fixed by trusted client-side Target/Gateway SSH host:port prefixes for username/key/password/MFA and trust challenges, before server-supplied MFA text. `credential_titles_identify_the_role_host_and_port` RED→GREEN; additional real gateway title case GREEN.
3. Password partial success discarded updated methods (`src/ssh/auth.rs`, original line 247), repeating passwords instead of advancing to MFA. Fixed with bounded dynamic remaining-method dispatch. `password_then_mfa_uses_the_new_methods_without_repeating_password`: real protocol fixture, RED→GREEN.
4. Recovery identity omitted verified host keys (`editor.rs`, original lines 57/352). Fixed by binding target and optional gateway SHA-256 fingerprints from successful verification to the authenticated endpoint. `recovery_never_crosses_a_changed_trust_once_server_key`: real owned sshd restart at the same address with a new key, RED→GREEN. Additional gateway-key rotation coverage GREEN. Changed-key recovery is refused; no implicit trust or automatic upload is introduced.
5. Recovery could copy empty text from an unsuccessful open or overwrite a dirty destination (`editor.rs`, original lines 354/370). Fixed by requiring loaded source/destination state and a clean destination. `recovery_from_an_unsuccessful_open_cannot_replace_a_fresh_document` and `recovery_import_preserves_unsaved_destination_edits`: RED→GREEN.

Minor findings:

- Configured username remains in the editor window label for prompted-user sessions. Deferred; authenticated identity comparison is independent of that display label.
- Initial SFTP denial followed by success was not exercised; existing real-sshd recovery starts from a working subsystem. This evidence gap remains deferred. The separate credential-submission counting concern was regraded as Important because no-post-cancel credential submission is an F06 security assertion: protocol callback counters now verify no password/key/MFA submissions or new method starts after cancellation. No secret values are stored by these counters.

Declined-to-judge list and executor rulings are retained in PARITY_PROGRESS.md. Full post-fix suite: 162 passed, 3 opt-in ignored; 14 lifecycle integration cases passed.
