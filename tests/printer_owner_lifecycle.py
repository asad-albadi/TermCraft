"""Linux POSIX fixture for the exact printer owner watchdog; no native printing.

Run with --old-source to reproduce the reviewed startup race, then exercise the
production replacement. All blocking backend work is a fixture sleep loop.
"""
import argparse
import ctypes
import hashlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time


def until(predicate, timeout=4):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        value = predicate()
        if value:
            return value
        time.sleep(0.01)
    raise AssertionError("Controlled owner fixture timed out")


def owned_wait(pid, timeout=3):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        found, status = os.waitpid(pid, os.WNOHANG)
        if found:
            return os.waitstatus_to_exitcode(status)
        time.sleep(0.01)
    return None


def reap_owned(pid):
    # Reconfirm child ownership before signalling. No PID discovered by name,
    # and no signalling after waitpid has already reaped that identity.
    try:
        found, _ = os.waitpid(pid, os.WNOHANG)
        if found:
            return
        os.kill(pid, signal.SIGKILL)
        os.waitpid(pid, 0)
    except ChildProcessError:
        pass


LAUNCHER = r'''
import os, signal, subprocess, sys, time
from pathlib import Path
binary, prefix, case = sys.argv[1:]
p = subprocess.Popen([binary, str(os.getpid()), case, prefix], stdin=subprocess.PIPE)
Path(prefix + '.pid').write_text(str(p.pid))
if case == 'before_watch':
    deadline = time.monotonic() + 4
    while time.monotonic() < deadline:
        if '\nState:\tT' in Path('/proc/' + str(p.pid) + '/status').read_text():
            break
        time.sleep(.01)
    else:
        raise RuntimeError('Fixture did not stop before watch initialization')
p.stdin.write(b'\x07\0\0\0\x01Office'); p.stdin.flush()
Path(prefix + '.queued').write_text('INIT queued by actual owner')
if case == 'before_watch':
    os._exit(0)
if case == 'normal':
    Path(prefix + '.status').write_text(str(p.wait(timeout=4)))
sys.stdin.buffer.read(1)
os._exit(0)
'''


def fixture_source(watch, module=""):
    return r'''
extern crate self as libc;
#[allow(non_camel_case_types)] pub type pid_t = i32;
unsafe extern "C" {
    pub fn getppid() -> pid_t;
    pub fn _exit(code: i32) -> !;
    pub fn raise(sig: i32) -> i32;
}
use std::{io::Read, time::Duration};
''' + module + r'''
fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().collect();
    let case = &args[2]; let prefix = &args[3];
    // Deterministic scheduling delay immediately before production watch setup.
    if case == "before_watch" { unsafe { libc::raise(19); } }
''' + watch + r'''
    std::fs::write(format!("{prefix}.validated"),
        format!("{} {}", args[1], unsafe { libc::getppid() })).unwrap();
    let mut p = [0; 11]; std::io::stdin().read_exact(&mut p).unwrap();
    assert_eq!(&p, b"\x07\0\0\0\x01Office");
    // Only this stand-in for Backend::open is entered after the real owner guard.
    std::fs::write(format!("{prefix}.backend"), "fixture backend entered").unwrap();
    if case == "normal" { return Ok(()); }
    loop { std::thread::sleep(Duration::from_millis(50)); }
}
'''


def run_case(binary, out, label, case, expect_old_race=False):
    prefix = out / label
    for path in out.glob(label + ".*"):
        path.unlink()
    owner = subprocess.Popen(
        [sys.executable, "-c", LAUNCHER, str(binary), str(prefix), case],
        stdin=subprocess.PIPE,
    )
    pid = None
    reaped = False
    try:
        until(lambda: Path(str(prefix) + ".queued").exists())
        pid = int(Path(str(prefix) + ".pid").read_text())
        if case == "before_watch":
            assert owner.wait(timeout=4) == 0
            # The real owner has exited with valid INIT buffered, before any
            # child watchdog statement or backend operation has been executed.
            os.kill(pid, signal.SIGCONT)
            if expect_old_race:
                until(lambda: Path(str(prefix) + ".backend").exists())
                time.sleep(0.8)
                found, _ = os.waitpid(pid, os.WNOHANG)
                assert found == 0, "Original source unexpectedly contained the race"
                captured = int(Path(str(prefix) + ".validated").read_text().split()[1])
                assert captured == os.getpid(), "Original source did not trust its adopted parent"
                return {"case": label, "owner_pid": owner.pid,
                        "captured_adopted_parent": captured, "red_reproduced": True,
                        "backend_entered": True, "cleanup": "owned child killed and reaped in finally"}
            code = owned_wait(pid)
            assert code == 1, "New helper survived owner death before watchdog setup"
            reaped = True
            assert not Path(str(prefix) + ".backend").exists()
            assert not Path(str(prefix) + ".validated").exists()
            return {"case": label, "owner_pid": owner.pid, "child_exit": code,
                    "backend_entered": False, "reaped": True}
        until(lambda: Path(str(prefix) + ".backend").exists())
        expected, actual = map(int, Path(str(prefix) + ".validated").read_text().split())
        assert expected == actual == owner.pid
        assert owner.poll() is None
        if case == "normal":
            until(lambda: Path(str(prefix) + ".status").exists())
            assert Path(str(prefix) + ".status").read_text() == "0"
            assert owner.poll() is None, "Normal startup unexpectedly lost its owner"
            reaped = True  # The owner itself waited for the normal child.
            return {"case": label, "owner_pid": owner.pid, "backend_entered": True,
                    "child_exit": 0, "reaped_by_live_owner": True}
        owner.stdin.write(b"x"); owner.stdin.flush()
        assert owner.wait(timeout=4) == 0
        start = time.monotonic()
        code = owned_wait(pid)
        assert code == 1, "Watchdog did not contain a blocked callback after owner death"
        reaped = True
        return {"case": label, "owner_pid": owner.pid, "backend_entered": True,
                "child_exit": code, "reaped": True,
                "seconds_after_owner_exit": round(time.monotonic() - start, 3)}
    finally:
        if owner.poll() is None:
            try:
                owner.stdin.write(b"x"); owner.stdin.flush()
                owner.wait(timeout=4)
            except (BrokenPipeError, subprocess.TimeoutExpired):
                owner.terminate(); owner.wait(timeout=4)
        if pid is None and Path(str(prefix) + ".pid").exists():
            pid = int(Path(str(prefix) + ".pid").read_text())
        if pid is not None and not reaped:
            reap_owned(pid)


def main():
    if sys.platform != "linux":
        raise SystemExit("This controlled POSIX scheduling fixture requires Linux; it is not a macOS runtime test")
    parser = argparse.ArgumentParser()
    root = Path(__file__).resolve().parents[1]
    parser.add_argument("--owner-source", type=Path, default=root / "engines/rdp/src/printer_owner.rs")
    parser.add_argument("--old-source", type=Path)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    out = args.output.resolve(); out.mkdir(parents=True, exist_ok=True)
    assert ctypes.CDLL(None).prctl(36, 1, 0, 0, 0) == 0
    owner_source = args.owner_source.resolve()
    module = "#[path = " + json.dumps(str(owner_source)) + "] mod owner;\n"
    watch = "    let expected = owner::expected_owner(Some(std::ffi::OsStr::new(&args[1])))?;\n    owner::watch(expected)?;\n"
    source = out / "owner-new-fix1936.rs"
    source.write_text(fixture_source(watch, module))
    binary = out / "owner-new-fix1936"
    subprocess.run(["rustc", "--edition=2024", str(source), "-o", str(binary)], check=True)
    cases = []
    unrelated = subprocess.Popen([sys.executable, "-c", "import time; time.sleep(30)"])
    old_hash = None
    try:
        if args.old_source:
            text = args.old_source.read_text()
            start = text.index("        let parent = unsafe { libc::getppid() };")
            end = text.index("    }\n    serve::<platform::Native>", start)
            old_watch = text[start:end]
            old_hash = hashlib.sha256(old_watch.encode()).hexdigest()
            old_src = out / "owner-old-fix1936.rs"
            old_src.write_text(fixture_source(old_watch))
            old_bin = out / "owner-old-fix1936"
            subprocess.run(["rustc", "--edition=2024", str(old_src), "-o", str(old_bin)], check=True)
            cases.append(run_case(old_bin, out, "old-before-watch", "before_watch", True))
            assert unrelated.poll() is None
        for label, case in [("new-before-watch", "before_watch"),
                            ("new-normal", "normal"),
                            ("new-after-init", "after_init")]:
            cases.append(run_case(binary, out, label, case))
            assert unrelated.poll() is None, "Unrelated child was affected by owner cleanup"
        report = {"platform": "Linux controlled POSIX watchdog fixture", "native_print_calls": False,
                  "old_watch_exact_source_sha256": old_hash,
                  "new_owner_exact_source_sha256": hashlib.sha256(owner_source.read_bytes()).hexdigest(),
                  "unrelated_child_survived_all_cases": True, "cases": cases,
                  "all_owned_fixture_children_reaped": True}
    finally:
        if unrelated.poll() is None:
            unrelated.terminate()
        unrelated.wait(timeout=4)
    report["unrelated_control_child_reaped"] = True
    (out / "owner-lifecycle.json").write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))


if __name__ == "__main__":
    main()
