# F05 independent review — pending one fix pass

Reviewer /root/f05_review independently reran the 3 SCP endpoint tests and fmt; no re-review is planned.

Critical:
1. st_size-only copying silently truncates readable virtual files such as /proc/version. Check EOF after advertised bytes before commit.
2. Cancellation can race local rename and remote EOF-driven publication. Separate cancellable staging, explicit remote authorization, authoritative commit outcome, and indeterminate lost acknowledgement.
3. Pathname-reopened staging/cleanup can act in a replaced directory. Anchor directory handles and staged identity; abort changes; truthful recovery paths; never recursively remove unrelated replacements.

Important:
1. Initial SCP listing waits for unrelated select wake; silent startup command hangs pane.
2. Raw SSH channel open/exec lacks cancellation-safe close before into_stream; exec send needs a bound; repeated cancelled starts must not exhaust session limit.
3. Full remote snapshot preparation plus fixed header deadline fails healthy slow/large sources; distinguish progress from stalls.
4. Negotiate actual helper/browse/transfer/atomic-policy capabilities; fail missing prerequisites before transferring; retain bounded diagnostics for enumeration failures.
5. Late no-replace conflicts must be typed Conflict/Skipped; upload failures need useful recovery; distinguish publication from post-commit cleanup failure.
6. Snapshot chmod can make readable sources unreadable; new upload mode can be reduced by umask. Preserve owner readability during staging and apply ordinary final mode on publication.
7. Header incorrectly says Embedded SFTP for SCP; all toolbar/drop/backend gates must share capabilities.
8. Folders, mutations and remote editing remain actual implementation gaps, not exclusions. Do not mark full F05 complete.

Deferred minors (do not fix as part of review pass): escaped display-name budget expansion; duplicate local staging orchestration.

Declined to judge / rulings required:
- Original normal/enhanced wire behavior, defaults, speed and router fallback need E04/V07 runtime evidence.
- Native Windows/mac GUI/drag/ACL/filesystem/package lifecycle need native machines.
- Other SCP implementations/restricted appliances need endpoint evidence; missing capabilities are findings above.
- Broader trust/cert/MFA/vault/jump behavior belongs F01/F06/S05/D01; shared handle and single auth inspected.
- Archive/redirect/package upgrade/removal/unrelated install paths unchanged.
- Existing F01-F04 deferred minors retain their owning records; F05 integrity/folders/mutations/edit were assessed here.
