#!/usr/bin/env python3
"""Verify a local TermCraft archive and capture native startup evidence."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import socket
import subprocess
import tempfile


# Load from this script's directory even when imported by file-location fixtures.
import importlib.util
_archive_spec = importlib.util.spec_from_file_location(
    "termcraft_portable_archive", Path(__file__).with_name("portable_archive.py"))
_archive_module = importlib.util.module_from_spec(_archive_spec)
_archive_spec.loader.exec_module(_archive_module)
extract_tar = _archive_module.extract_tar
extract_zip = _archive_module.extract_zip


def extract(archive, directory):
    if archive.name.endswith(".tar.gz"):
        extract_tar(archive, directory)
    else:
        extract_zip(archive, directory)


def sha256(path):
    with path.open("rb") as file:
        return hashlib.file_digest(file, "sha256").hexdigest()


def verify(root, manifest_path):
    manifest = json.loads(manifest_path.read_text())
    host = {"Linux": "linux", "Windows": "windows", "Darwin": "macos"}[platform.system()]
    if manifest["platform"] != host:
        raise ValueError("The startup check must run on the package's native platform.")
    actual = {
        path.relative_to(root).as_posix(): sha256(path)
        for path in root.rglob("*") if not path.is_symlink() and path.is_file() and path != manifest_path
    }
    if actual != manifest["files"]:
        raise ValueError("Package files differ from the manifest.")
    links = {path.relative_to(root).as_posix(): os.readlink(path)
             for path in root.rglob("*") if path.is_symlink()}
    if links != manifest.get("symlinks", {}):
        raise ValueError("Package links differ from the manifest.")
    return host, manifest


def run(command, cwd, environment, log_path, timeout=120):
    with log_path.open("xb") as log:
        completed = subprocess.run(command, check=False, cwd=cwd, env=environment,
                                   stdin=subprocess.DEVNULL, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, timeout=timeout)
        log.write(completed.stdout)
        log.write(completed.stderr)
    completed.check_returncode()
    return completed.stdout


def linux_rdp_helper_matches(image, expected_digest):
    """Compare the actual child executable with the verified package entry."""
    import fcntl

    try:
        if os.readlink(image) != "/memfd:termcraft-rdp-engine (deleted)":
            return False
        required = fcntl.F_SEAL_WRITE | fcntl.F_SEAL_GROW | fcntl.F_SEAL_SHRINK | fcntl.F_SEAL_SEAL
        with image.open("rb") as actual:
            return (
                fcntl.fcntl(actual, fcntl.F_GET_SEALS) & required == required
                and hashlib.file_digest(actual, "sha256").hexdigest() == expected_digest
            )
    except OSError:
        return False


def macos_rdp_helper_matches(parent_pid, expected_digest):
    import ctypes

    libproc = ctypes.CDLL("/usr/lib/libproc.dylib")
    libproc.proc_pidpath.argtypes = [ctypes.c_int, ctypes.c_void_p, ctypes.c_uint32]
    libproc.proc_pidpath.restype = ctypes.c_int
    children = subprocess.run(
        ["/usr/bin/pgrep", "-P", str(parent_pid)],
        text=True, capture_output=True, timeout=5,
    )
    for pid in children.stdout.split():
        path = ctypes.create_string_buffer(4096)
        if libproc.proc_pidpath(int(pid), path, len(path)) <= 0:
            continue
        executable = Path(os.fsdecode(path.value))
        if executable.name == "termcraft-rdp-engine" and sha256(executable) == expected_digest:
            return True
    return False


def rdp_session_probe(executable, helper_digest, isolated, environment, evidence):
    workspace = isolated / "RDP test workspace"
    workspace.mkdir()
    config = workspace / "workspace.json"
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)
        listener.settimeout(30)
        profile = {
            "id": "package-rdp-test", "name": "Packaged helper startup", "protocol": "RDP",
            "host": "127.0.0.1", "user": "package-test", "port": listener.getsockname()[1],
        }
        preferences = {"font_size": 19.0}
        config.write_text(json.dumps({"sessions": [profile], "preferences": preferences}))
        with (evidence / "rdp-session.log").open("xb") as log:
            app = subprocess.Popen(
                [executable, "--data-dir", workspace, "--session", "package-rdp-test"],
                cwd=isolated, env=environment, stdin=subprocess.DEVNULL,
                stdout=log, stderr=subprocess.STDOUT,
            )
            try:
                connection, _ = listener.accept()
                with connection:
                    connection.settimeout(10)
                    with connection.makefile("rb") as incoming:
                        header = incoming.read(4)
                        if len(header) != 4 or header[:2] != b"\x03\x00":
                            raise ValueError("Packaged RDP session did not send a TPKT header.")
                        if platform.system() == "Linux":
                            threads = Path(f"/proc/{app.pid}/task")
                            children = {
                                pid for thread in threads.iterdir()
                                if (thread / "children").exists()
                                for pid in (thread / "children").read_text().split()
                            }
                            if not any(
                                linux_rdp_helper_matches(Path(f"/proc/{pid}/exe"), helper_digest)
                                for pid in children
                            ):
                                raise ValueError("RDP session did not start the verified package helper.")
                        elif platform.system() == "Darwin":
                            if not macos_rdp_helper_matches(app.pid, helper_digest):
                                raise ValueError("RDP session did not start the verified macOS helper.")
            finally:
                if app.poll() is None:
                    app.terminate()
                    try:
                        app.wait(5)
                    except subprocess.TimeoutExpired:
                        app.kill()
                        app.wait(5)
    # The GUI may persist its calculated sidebar width and expand default fields.
    stored = json.loads(config.read_text())
    sessions = stored["sessions"]
    if len(sessions) != 1 or any(sessions[0].get(key) != value for key, value in profile.items()):
        raise ValueError("RDP startup modified the saved session in the workspace.")
    if any(stored["preferences"].get(key) != value for key, value in preferences.items()):
        raise ValueError("RDP startup modified the saved workspace preferences.")


def smoke(archive, evidence, display):
    from PIL import Image, ImageStat

    evidence.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="TermCraft startup ") as temporary:
        # Resolve the trusted temp root (e.g. macOS /var -> /private/var);
        # the extractor still rejects symlinks in the destination below it.
        isolated = Path(temporary).resolve()
        package_dir = isolated / "package files"
        package_dir.mkdir()
        extract(archive, package_dir)
        roots = list(package_dir.iterdir())
        if len(roots) != 1 or not roots[0].is_dir():
            raise ValueError("Expected one package directory.")
        root = roots[0]
        manifests = list(root.rglob("PACKAGE-MANIFEST.json"))
        if len(manifests) != 1:
            raise ValueError("Expected one package manifest.")
        host, manifest = verify(root, manifests[0])
        bindir = root / "Contents/MacOS" if host == "macos" else root
        suffix = ".exe" if host == "windows" else ""
        executable = bindir / f"harbor-workspace{suffix}"
        helper = bindir / f"termcraft-rdp-engine{suffix}"
        environment = {
            **os.environ, "PATH": "", "HOME": str(isolated), "USERPROFILE": str(isolated),
            "XDG_CONFIG_HOME": str(isolated / "config"), "XDG_DATA_HOME": str(isolated / "data"),
            "LOCALAPPDATA": str(isolated / "local"),
        }
        for variable in tuple(environment):
            if variable.startswith(("LD_", "DYLD_", "CARGO_", "TERMCRAFT_")):
                environment.pop(variable)
        if display:
            environment["DISPLAY"] = display
            environment.pop("WAYLAND_DISPLAY", None)
        for name, program, args in (
            ("help", executable, ["--help"]),
            ("rdp-helper", helper, []),
        ):
            result = run([program, *args], isolated, environment, evidence / f"{name}.log")
            if name == "help" and b"TermCraft remote workspace" not in result:
                raise ValueError("Packaged executable returned unexpected help output.")
            if name == "rdp-helper":
                if len(result) < 5 or result[0] != 1:
                    raise ValueError("RDP helper returned no control packet.")
                end = 5 + int.from_bytes(result[1:5], "big")
                if json.loads(result[5:end]) != {"Ready": {"version": 1}}:
                    raise ValueError("RDP helper returned an unexpected handshake.")
        screenshot = evidence / "startup.png"
        launcher = bindir / "termcraft-desktop-launcher" if host == "macos" else executable
        run([launcher, "--data-dir", isolated / "workspace data", "--screenshot", screenshot],
            isolated, environment, evidence / "startup.log")
        with Image.open(screenshot) as image:
            stats = ImageStat.Stat(image.convert("RGB"))
            if image.width < 640 or image.height < 400 or max(stats.stddev) < 2:
                raise ValueError("Screenshot is too small or blank.")
            size = image.size
        helper_digest = manifest["files"][helper.relative_to(root).as_posix()]
        rdp_session_probe(executable, helper_digest, isolated, environment, evidence)
        (evidence / "verification.json").write_text(json.dumps({
            "archive": archive.name, "archive_sha256": sha256(archive),
            "manifest_schema": manifest["schema_version"], "platform": host,
            "display_backend": (
                "Wayland" if environment.get("WAYLAND_DISPLAY") else "X11"
            ) if host == "linux" else host,
            "start_environment": "relocated package, empty PATH, isolated workspace, existing native display",
            "screen_size": size, "rdp_helper_wire_version": 1,
            "rdp_session": "Received initial RDP negotiation on a loopback-only fixture; workspace preserved",
            "rdp_helper_child_location_verified": host in {"linux", "macos"},
        }, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--display", help="Optional X11 display, e.g. :99")
    args = parser.parse_args()
    try:
        smoke(args.archive.resolve(strict=True), args.evidence.resolve(), args.display)
    except (OSError, ValueError, ImportError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Startup verification failed: {error}\n")


if __name__ == "__main__":
    main()
