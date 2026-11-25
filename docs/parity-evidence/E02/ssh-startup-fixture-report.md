# SSH startup regression fixture

The shared macro/X11 batch authenticated successfully and failed on the next assertion, which expected the first SSH prompt to have reached the worker queue after a fixed 500 ms sleep. The failure reproduces the distinction between a session connection event and its later channel data. The SSH worker sends Connected after successful shell startup and publishes output after receiving channel data. Applying the first event does not imply the second is already queued.

The regression fixture now gates the real SSH peer’s first prompt on a temporary file. The test releases the peer after observing its real Connected event, waits for the real FIRST_SSH_PROMPT output, then applies both through the terminal’s normal poll as one bounded batch. The peer disables PTY echo and uses cat, so the final assertion sees a reply from the remote process. The SSH transport, authentication, macro queue and event handling remain active. The poll seam selects the standard SSH handle batch reader in production.

Baseline batch evidence is in ../parallel-integration-macros-x11/full.log. The targeted rerun and shared integration gate are tracked separately.

The exact test passed through Cargo and passed five consecutive reruns from the same verified test binary. The macro-filtered binary run passed all 13 tests, including SSH, PTY and VNC. The scoped formatting check also passed. Logs are ssh-startup-fixture-green.log, ssh-startup-repeat.log, ssh-startup-macro-regression.log and ssh-startup-format.log. The shared full-suite rerun remains pending.
