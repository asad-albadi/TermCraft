# Windows Browser source review

Independent reviewer: `/root/windows_browser_review`. This is the pre-fix source review, not a native Windows execution result.

## Important finding

At `src/browser/windows_pipe.rs:296` and `:308`, both child CDP pipe endpoints are temporarily inheritable in the main application's handle table. The Chrome-specific handle list at approximately line589 confines Chrome's own inheritance but cannot constrain an unrelated concurrent `std::process::Command` launch. A sibling can inherit the private pipe outside the Browser job and retain/read/write it. The existing fixture starts its unrelated process after dropping the client endpoint, so it misses this overlap.

Acceptance: overlap ordinary sibling launch with the private endpoint preparation using a deterministic signal. The sibling must have neither endpoint and must not keep the Browser channel alive. Chrome must still receive exactly the intended endpoints. Fix the ownership/inheritance boundary rather than relying on a mutex that does not cover Rust's private process lock and third-party launches.

[Relevant Rust process implementation](https://github.com/rust-lang/rust/blob/master/library/std/src/sys/process/windows.rs) documents the ambient inheritance issue; the raw Win32 launcher does not participate in that private lock.

## Existing evidence and limits

The production Windows component and native fixture sources type-checked for x86_64 MSVC. Two PE eligibility tests executed on Linux. The Linux regression target had three real Chromium fixture tests and one settings case, not four browser executions. The full Windows application build stopped at missing `lib.exe`; native Windows execution and packaging remain pending. Chromium source confirms decimal uint32 pipe handles and synchronous child endpoints. No source was edited and no Windows/browser process was run by the reviewer.

## Independent scoped re-review

**Addressed in source.** The main process now owns only non-inheritable CDP endpoints. A creation-time job-confined helper is started with inheritance disabled. Both sides authenticate the control pipe against expected kernel process IDs. Only that helper receives inheritable duplicates and launches Chromium with the intended pipe/NUL list. The revised native fixture overlaps an ordinary sibling launch with the live endpoints, checks the main handle flags and requires EOF while that sibling remains alive. No additional confirmed Important or Critical inheritance issue was identified by the independent reviewer. Native fixture execution remains unavailable.

Native acceptance must also cover a per-user Chrome installation and the helper's known-folder/profile-parent validation with its redirected environment. Source checking alone does not establish those OS behaviors.
