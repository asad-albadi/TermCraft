All commands use the app directory as working directory unless noted.

- Prepared private copy with `python3 target/agent-work/tunnel-claim-fix2/snapshot.py`. Private Cargo identity is 0.1.948 / termcraft-tunnel-claim948.
- Inside `target/agent-work/tunnel-claim-fix2`, generated and ran each minimal reproducer with:
  `python3 repro.py MODE`
  `rustc --edition=2024 --test MODE.rs -o MODE-test`
  `./MODE-test fixture_port_claim_survives_socket_handoff_and_releases_on_drop --exact --nocapture`
  Modes are red, green, mutation-release, mutation-drop. Logged test output in MODE.log.
- Full scoped test: `work="$PWD/target/agent-work/tunnel-claim-fix2"`
  `CARGO_TARGET_DIR="$PWD/target" TERMCRAFT_INTEROP_LOG_DIR="$work/final-sshd" cargo test --offline --locked -j 2 --manifest-path "$work/source/Cargo.toml" --test tunnel_interop -- --nocapture --test-threads=14 > "$work/all.log" 2>&1`.
- Packaged with `python3 target/agent-work/tunnel-claim-fix2/package-evidence.py`, followed by `patch --dry-run -p1 -d . < "$work/payload.patch" > "$work/patch-dry-run.log"`.
