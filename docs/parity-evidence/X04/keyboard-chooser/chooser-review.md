# Increment 958 owned Linux XDMCP chooser review

**SPEC ACCEPT · QUALITY ACCEPT** for increment 958’s normal Indirect chooser handoff against the frozen b5893bf70362241fbff489979d315307a34549e107f24b6f0de99388fbde6f2e baseline. The production change at `src/x11/xdmcp.rs:368` confines `-once` to Direct mode. The existing startup deadline, socket peer PID check, TCP listener ownership check, cookie handling and owned shutdown still apply. I independently verified the hashes, sizes and modes of all seven payload files, three base files, twelve fixture files and two additional archives, plus the patch hash.

The isolated cases use the packaged xdm chooser and two real managers. `src/x11/xdmcp_chooser_test.rs:141` checks the selected manager’s authenticated 800×600 window, forwarding event and original Xephyr PID. Both clipboard policies passed, with bidirectional Unicode exchange on the enabled path. The focused suite recorded 40 passes and nine ignored cases. I reviewed the recorded runs without repeating them.

One reporting distinction matters. `evidence/production-red.log:26` directly records engine exit with clipboard disabled. `evidence/production-clipboard-red.log:22` directly records a clipboard connection reset. Both baseline cases failed before the selected window appeared. An exit status of zero for both runs should not be attributed to those two logs alone.

Delayed manager startup versus clipboard reconnect, final remote logout and PAM login remain unverified. No production or candidate files were changed during review.
