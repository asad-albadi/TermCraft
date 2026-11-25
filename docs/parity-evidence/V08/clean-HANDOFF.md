# Bounded clean Linux package observations

This evidence uses immutable unsigned archives. It does not advance V08, establish full native/original parity, or claim Windows/macOS/signing completion. No product files were edited. Existing root diagnostics remain intact.

## Artifacts and isolated images

- Current archive SHA-256: `68f356f63c92a9a6aced6abe3d845ab9d064e5da0d37b52ff0c4c99f0fc4b117`, frozen source `4726eebadb56a15c2e9ed9f4018a63ffe9a55e9834ad927fea5472f2f2c29b53`.
- Old upgrade archive: `b0d1f602e02a06050afa4196b3f4e6f6c5e3b1b370ddb741b01295a1d09c1a98`.
- Official base registry digest: `375a401bb3c6376b37f40b6dfa25067db37ec78d0b7af67ae6315f43a0184a9d`; root GUI image `c326d40de0700f7086d34670a6d21b0af6d8a6da71bc442fceed67bcd0648fe5`.
- Owned input image `bf83b781e2d467fe7cbafb6c4235c3263260fd56cec7cfb9b22dafde0a914a12` adds signed Arch Xvfb/xdotool/ImageMagick packages.
- Owned declared-runtime image `1ab61a2bb16febd0a86829a43c26849b06ca82aca31be8385880cc7a3570c79d` adds signed packages named by the bundled Chromium `SOURCE-PKGINFO` dependency list, plus declared Perl/OpenSSH. No system Chromium or Mosh is installed.

Dockerfiles, build logs, image inspections and complete package-version inventories are retained. The base registry image is pinned; pacman repository state is captured by actual installed-version inventories, not claimed to be a future-reproducible package repository snapshot.

Every runtime container uses a unique `termcraft-v08-clean-*` name, the label `org.termcraft.validation=v08-clean`, no network, no host display/home/libraries/credentials, dropped capabilities, read-only root, a dedicated read-only package bind, one writable evidence bind, and private `/tmp`. The final runner explicitly allows execution in its private tmpfs for extracted Unix bytes. No privileged mode, host network, sandbox bypass or user-container/image pruning is used. `finally` removes only the runner's own container; child processes are reaped and time limited. Built validation images are retained for review.

## Actual observations

| Scenario | Result and boundary |
|---|---|
| Current fresh native GUI → Settings **Save preferences** → restart | Shell `/usr/bin/bash` is saved; complete workspace bytes match after restart. |
| Current native local terminal | Actual focused terminal-widget keyboard input writes `CLEAN_NATIVE_PTY_OK` through a real shell. |
| Legacy JSON → old b0d1 actual **Save preferences** → current release → actual Save → restart | Old Save persists a generated stable ID. Current release preserves the entire saved JSON byte sequence, including session/folder, ID, Dark appearance/font 18 and legacy macro. No-edit ID migration is not inferred. |
| Damaged workspace → actual Save attempt → restart | Original corrupt bytes survive exactly; UI says settings are damaged and asks for backup before reset. |
| Missing installed Mosh | Native tab explicitly reports selected installed provider unavailable; no bundled fallback. |
| Bundled Mosh without Perl | Native tab explicitly reports required system Perl `/usr/bin/perl`; no system-provider fallback. |
| Per-user install → installed payload GUI screenshot → uninstall | Installed/not-installed status is observed. User workspace, unrelated integration file and MIME defaults survive. Program bytes are copied only into container-private `/tmp`, then removed. |
| Extracted Iperf 2/3 bytes | Each transfers 262144 bytes through owned loopback endpoints with empty PATH. Direct ELF byte closure only; not the app's managed-service API. |
| Extracted BusyBox 1.31/1.35 archives | Exact packaged ash bytes run on real PTYs with empty PATH; terminal identity and pipeline output are asserted. Direct byte closure only; not PackageManager integration. |
| Bundled browser, minimal GUI image | Launch fails; exact loader log identifies missing declared `libopenh264.so.8`. This image intentionally lacks full browser dependencies. |
| Bundled browser, declared runtime dependencies installed | Packaged `--version` succeeds: Chromium 158.0.8127.88. Sandboxed GUI/native browser still aborts: container lacks a usable root-owned setuid helper/unprivileged namespace sandbox. No `--no-sandbox` attempt was made. Full browser operation remains unresolved here. |
| Current full Arch archive → Debian wrapper dependency calculation | Safely rejects unmapped `libz.so.1` before producing a package. Highest declared GLIBC is 2.44. This current full Arch payload is not qualified as a Debian installer. |

Screenshots, actual launch commands, scripts, stdout, image IDs, archive hashes, package-file counts and retained evidence hashes are indexed by `clean-observations.json`. The original diagnostic setup mistakes remain recorded and explicitly distinguished from product defects. No product defect is claimed from a fixture/setup error.

## Replay and review

From the application root, with the two retained image IDs and immutable extractions present:

```sh
python docs/parity-evidence/V08/clean-suite.py
```

The suite resets only its named disposable fixture subdirectories under `clean-run`, reconstructs fixture JSON, deletes its old PTY marker, runs every final scenario sequentially, verifies immutable archive/extraction hashes and checks preservation assertions. `clean-suite-replay.log` records the actual replay. `clean-runner.py` can also run one `clean-scenarios/*.sh` script with an explicit `--package` and `--image`. Complete container-expanded scripts are retained alongside each observation; replay sources are the narrow scripts in `clean-scenarios`.

Remaining gaps: native physical-machine/distro installation and upgrade coverage; native Windows/macOS; credential-vault upgrade contents; browser sandbox success on a supported native machine; complete managed-API success coverage for every runtime; original behavior/source closure and signing/notarization. Portable and per-user evidence here does not qualify all of these gaps.
