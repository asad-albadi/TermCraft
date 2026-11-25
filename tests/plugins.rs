#![cfg(all(target_os = "linux", target_arch = "x86_64"))]

use flate2::{Compression, write::GzEncoder};
use harbor_workspace::{plugins::PluginRegistry, process::Process};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn fixture() -> &'static Path {
    static BINARY: OnceLock<(tempfile::TempDir, PathBuf)> = OnceLock::new();
    &BINARY
        .get_or_init(|| {
            let directory = tempfile::tempdir().unwrap();
            let source = directory.path().join("fixture.c");
            let binary = directory.path().join("fixture");
            fs::write(
                &source,
                r#"
#include <stdio.h>
#include <signal.h>
#include <string.h>
#include <unistd.h>
int main(void) {
    char request[256];
    if (!fgets(request, sizeof(request), stdin)) return 6;
    if (!strstr(request, "\"protocol\":1")) return 7;
    FILE *marker = fopen("invoked", "a");
    if (!marker) return 8;
    fprintf(marker, "%d\n", getpid());
    fclose(marker);
    if (strstr(request, "\"entrypoint\":\"crash\"")) raise(SIGSEGV);
    if (strstr(request, "\"entrypoint\":\"hang\"")) {
        for (;;) pause();
    }
    if (strstr(request, "\"entrypoint\":\"large\"")) {
        for (;;) putchar('z');
    }
    if (strstr(request, "\"entrypoint\":\"invalid\"")) {
        puts("malformed");
        return 0;
    }
    if (strstr(request, "\"entrypoint\":\"bad\"")) {
        puts("{\"protocol\":2,\"ok\":true}");
        return 0;
    }
    puts("{\"protocol\":1,\"ok\":true}");
    return 0;
}
"#,
            )
            .unwrap();
            assert!(
                Command::new("cc")
                    .args(["-O2", "-o"])
                    .arg(&binary)
                    .arg(&source)
                    .status()
                    .unwrap()
                    .success()
            );
            (directory, binary)
        })
        .1
}

fn bundle(
    root: &Path,
    version: &str,
    bad_member: Option<(&[u8], tar::EntryType, u64)>,
    wrong_cpu: bool,
) -> PathBuf {
    let mut binary = fs::read(fixture()).unwrap();
    if wrong_cpu {
        binary[18] = 0xb7;
    }
    let mut archive = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::fast()));
    if let Some((path, kind, size)) = bad_member {
        let mut header = tar::Header::new_ustar();
        header.as_mut_bytes()[..path.len()].copy_from_slice(path);
        header.set_mode(0o755);
        header.set_size(size);
        header.set_entry_type(kind);
        if kind.is_symlink() {
            header.set_link_name("../../outside").unwrap();
        }
        header.set_cksum();
        archive.append(&header, b"x".as_slice()).unwrap();
    }
    for (path, bytes) in [
        ("bin/fixture", binary.as_slice()),
        ("notices/LICENSE", b"Test fixture".as_slice()),
    ] {
        let mut header = tar::Header::new_ustar();
        header.set_mode(0o755);
        header.set_size(bytes.len() as u64);
        header.set_cksum();
        archive.append_data(&mut header, path, bytes).unwrap();
    }
    let data = archive.into_inner().unwrap().finish().unwrap();
    let manifest = json!({
        "schema": 1, "id": "fixture", "version": version, "host_protocol": 1,
        "platform": "linux", "architecture": "x86_64", "archive": format!("{version}.tar.gz"),
        "archive_sha256": hash(&data),
        "binary_sha256": hash(&binary), "executable": "fixture",
        "entrypoints": ["check", "crash", "hang", "large", "bad", "invalid"],
        "permissions": ["native-execution"]
    });
    let manifest_path = root.join(format!("{version}.termcraft-plugin.json"));
    fs::write(root.join(format!("{version}.tar.gz")), data).unwrap();
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    manifest_path
}

fn installed(root: &Path) -> PluginRegistry {
    let registry = PluginRegistry::new(root.join("plugins"));
    let reviewed = registry
        .inspect(&bundle(root, "1.0.0", None, false), &AtomicBool::new(false))
        .unwrap();
    registry
        .install(&reviewed, &AtomicBool::new(false))
        .unwrap();
    registry
}

fn directory(root: &Path) -> PathBuf {
    fs::read_dir(root.join("plugins/versions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path()
}

#[test]
fn disabled_plugin_cannot_execute() {
    let root = tempfile::tempdir().unwrap();
    let registry = installed(root.path());
    let cancel = AtomicBool::new(false);
    let dir = directory(root.path());
    assert!(registry.run("fixture", "check", &cancel).is_err());
    let installed = registry.list().unwrap().remove(0);
    registry
        .enable("fixture", &installed.release_digest)
        .unwrap();
    registry.disable("fixture").unwrap();
    assert!(registry.run("fixture", "check", &cancel).is_err());
    assert!(!dir.join("invoked").exists());
}

#[test]
fn plugin_crash_keeps_terminal_running() {
    let root = tempfile::tempdir().unwrap();
    let registry = installed(root.path());
    let mut terminal = Process::spawn("/bin/sh", &[], Some(root.path()), 80, 24).unwrap();
    let release = registry.list().unwrap().remove(0);
    registry.enable("fixture", &release.release_digest).unwrap();
    let error = registry
        .run("fixture", "crash", &AtomicBool::new(false))
        .unwrap_err();
    assert!(error.to_string().contains("exited"), "{error}");
    terminal
        .write(b"printf '\\156\\157\\162\\155\\141\\154\\012'\n")
        .unwrap();
    let until = Instant::now() + Duration::from_secs(2);
    let mut output = Vec::new();
    while Instant::now() < until {
        if let Ok(data) = terminal.rx.recv_timeout(Duration::from_millis(50)) {
            output.extend(data);
            if output.windows(6).any(|bytes| bytes == b"normal") {
                break;
            }
        }
    }
    assert!(output.windows(6).any(|bytes| bytes == b"normal"));
}

#[test]
fn malicious_archive_cannot_escape_plugin_root() {
    let root = tempfile::tempdir().unwrap();
    let registry = PluginRegistry::new(root.path().join("plugins"));
    let outside = root.path().join("outside");
    fs::write(&outside, b"untouched").unwrap();
    let absolute = outside.to_str().unwrap().as_bytes();
    for (path, kind, size) in [
        (b"bin/../../outside".as_slice(), tar::EntryType::Regular, 1),
        (absolute, tar::EntryType::Regular, 1),
        (b"bin/fixture", tar::EntryType::Symlink, 0),
        (b"bin/fixture", tar::EntryType::Link, 0),
        (b"bin/fixture", tar::EntryType::Regular, 8 * 1024 * 1024 + 1),
    ] {
        let filename = bundle(root.path(), "1.0.0", Some((path, kind, size)), false);
        assert!(
            registry
                .inspect(&filename, &AtomicBool::new(false))
                .is_err()
        );
        assert!(registry.list().unwrap().is_empty());
        assert_eq!(fs::read(&outside).unwrap(), b"untouched");
    }
    let filename = bundle(root.path(), "1.0.0", None, true);
    assert!(
        registry
            .inspect(&filename, &AtomicBool::new(false))
            .is_err()
    );
}

fn wait_for_pid(path: &Path) -> i32 {
    let until = Instant::now() + Duration::from_secs(2);
    while Instant::now() < until {
        if let Ok(contents) = fs::read_to_string(path)
            && let Some(pid) = contents.lines().last().and_then(|line| line.parse().ok())
        {
            return pid;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("Plugin never accepted an invocation")
}

fn assert_reaped(pid: i32) {
    // Signal zero only probes the PID recorded by the test's own child.
    assert_eq!(unsafe { libc::kill(pid, 0) }, -1);
    assert_eq!(
        std::io::Error::last_os_error().raw_os_error(),
        Some(libc::ESRCH)
    );
}

#[test]
fn failed_updates_keep_active_release_and_valid_updates_revoke_the_old_grant() {
    let root = tempfile::tempdir().unwrap();
    let registry = installed(root.path());
    let cancel = AtomicBool::new(false);
    let original = registry.list().unwrap().remove(0);
    registry
        .enable("fixture", &original.release_digest)
        .unwrap();
    registry.run("fixture", "check", &cancel).unwrap();

    let new_manifest = bundle(root.path(), "2.0.0", None, false);
    let review = registry.inspect(&new_manifest, &cancel).unwrap();
    let archive = root.path().join("2.0.0.tar.gz");
    let bytes = fs::read(&archive).unwrap();
    fs::write(&archive, b"changed after inspection").unwrap();
    assert!(registry.install(&review, &cancel).is_err());
    assert!(registry.list().unwrap()[0].enabled);
    assert_eq!(registry.list().unwrap()[0].manifest.version, "1.0.0");
    fs::write(archive, bytes).unwrap();
    cancel.store(true, Ordering::Release);
    assert!(registry.install(&review, &cancel).is_err());
    cancel.store(false, Ordering::Release);
    assert!(registry.list().unwrap()[0].enabled);

    let old_dir = directory(root.path());
    let user_file = old_dir.join("notes.txt");
    let old_notice = old_dir.join("notices/LICENSE");
    fs::write(&user_file, b"keep").unwrap();
    fs::write(&old_notice, b"updated user notice").unwrap();
    let updated = registry.install(&review, &cancel).unwrap();
    assert!(!updated.enabled);
    assert_eq!(updated.manifest.version, "2.0.0");
    assert!(!old_dir.join("bin/fixture").exists());
    assert_eq!(fs::read(user_file).unwrap(), b"keep");
    assert_eq!(fs::read(old_notice).unwrap(), b"updated user notice");
    assert!(registry.run("fixture", "check", &cancel).is_err());
    assert!(
        registry
            .enable("fixture", &original.release_digest)
            .is_err()
    );
    registry.enable("fixture", &updated.release_digest).unwrap();
    assert!(
        registry
            .run_release("fixture", &original.release_digest, "check", &cancel)
            .is_err()
    );
    registry.run("fixture", "check", &cancel).unwrap();
}

#[test]
fn removing_running_plugin_waits_for_its_lease_and_preserves_user_files() {
    let root = tempfile::tempdir().unwrap();
    let registry = installed(root.path());
    let release = registry.list().unwrap().remove(0);
    let dir = directory(root.path());
    let executable = dir.join("bin/fixture");
    let cancel = AtomicBool::new(false);
    registry.enable("fixture", &release.release_digest).unwrap();

    thread::scope(|scope| {
        let worker = scope.spawn(|| registry.run("fixture", "hang", &cancel));
        let pid = wait_for_pid(&dir.join("invoked"));
        let outcome = registry.remove("fixture", &AtomicBool::new(false)).unwrap();
        assert!(outcome.deferred);
        assert!(registry.list().unwrap().is_empty());
        assert!(
            registry
                .run("fixture", "check", &AtomicBool::new(false))
                .is_err()
        );
        assert!(executable.is_file());
        let until = Instant::now();
        cancel.store(true, Ordering::Release);
        let error = worker.join().unwrap().unwrap_err();
        assert!(matches!(
            error,
            harbor_workspace::packages::PackageError::Cancelled
        ));
        assert!(until.elapsed() < Duration::from_secs(1));
        assert_reaped(pid);
    });
    // Cancellation defers retirement work. A later uncancelled request
    // performs the pending cleanup even though the plugin was removed.
    assert!(executable.is_file());
    assert!(
        registry
            .run("fixture", "check", &AtomicBool::new(false))
            .is_err()
    );
    assert!(!executable.exists());
    assert!(dir.join("invoked").is_file());
}

#[test]
fn ipc_rejects_malformed_wrong_version_and_oversized_responses_and_times_out() {
    let root = tempfile::tempdir().unwrap();
    let registry = installed(root.path());
    let release = registry.list().unwrap().remove(0);
    registry.enable("fixture", &release.release_digest).unwrap();
    let cancel = AtomicBool::new(false);
    for (entry, message) in [
        ("large", "size limit"),
        ("bad", "incompatible"),
        ("invalid", "Malformed"),
    ] {
        let error = registry
            .run("fixture", entry, &cancel)
            .unwrap_err()
            .to_string();
        assert!(error.contains(message), "{entry}: {error}");
        assert_reaped(wait_for_pid(&directory(root.path()).join("invoked")));
    }
    let started = Instant::now();
    let error = registry
        .run("fixture", "hang", &cancel)
        .unwrap_err()
        .to_string();
    assert!(error.contains("timed out"), "{error}");
    assert!(started.elapsed() < Duration::from_secs(7));
    assert_reaped(wait_for_pid(&directory(root.path()).join("invoked")));
}

#[test]
fn installed_binary_changes_are_rejected_before_launch() {
    let root = tempfile::tempdir().unwrap();
    let registry = installed(root.path());
    let release = registry.list().unwrap().remove(0);
    registry.enable("fixture", &release.release_digest).unwrap();
    let dir = directory(root.path());
    let executable = dir.join("bin/fixture");
    let mut bytes = fs::read(&executable).unwrap();
    bytes.push(0);
    fs::write(&executable, bytes).unwrap();
    assert!(
        registry
            .run("fixture", "check", &AtomicBool::new(false))
            .unwrap_err()
            .to_string()
            .contains("modified")
    );
    assert!(!dir.join("invoked").exists());
}
