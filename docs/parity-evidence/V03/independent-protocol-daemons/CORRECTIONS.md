# Evidence wording corrections

This clarification supplements the original sealed 40-file subset and complete archive. Their files, indexes and seals remain unchanged. No production-source finding or qualification outcome changes.

The report statements that the FTP fixture root was empty are too broad. The raw pre-cleanup record retains the server baseline `/srv/ftp/welcome.msg`. The established cleanup result is that no files or directories created by the qualification roundtrips remain: `roundtrip.bin`, `renamed.bin` and `owned-dir` were removed. The baseline welcome file is not a leaked test upload.

`server-observations.log` is a historical snapshot and its `SERVER_CLIPBOARD_OWNED` text is not the final client-to-server clipboard proof. The authoritative bidirectional result is `vnc-candidate-final.log`: the client observed the server clipboard marker, the run returned `status=0`, and an independent X display assertion checked the exact keyboard command file and exact `CLIENT_CLIPBOARD_OWNED` bytes while connected, returning `observation_status=0`.

This documentation clarification requires no runtime replay, test or build. Temporary TigerVNC fixture lifecycle remains owned by `real_wayland_portal` for the separately requested genuine GUI workflow and final cleanup.
