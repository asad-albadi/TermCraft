#!/usr/bin/env python3
"""Verify a local TermCraft archive and capture native startup evidence."""

import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import socket
import subprocess
import tempfile
import time


# Load from this script's directory even when imported by file-location fixtures.
import importlib.util
_archive_spec = importlib.util.spec_from_file_location(
    "termcraft_portable_archive", Path(__file__).with_name("portable_archive.py"))
_archive_module = importlib.util.module_from_spec(_archive_spec)
_archive_spec.loader.exec_module(_archive_module)
extract_tar = _archive_module.extract_tar
extract_zip = _archive_module.extract_zip

def windows_software_gl_files():
    fixture = Path(__file__).resolve().parents[1] / "tests/windows-software-gl.json"
    return json.loads(fixture.read_text())["files"]


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


def run(command, cwd, environment, log_path, timeout=120, *, input_bytes=None):
    with log_path.open("xb") as log, subprocess.Popen(
        command, cwd=cwd, env=environment,
        stdin=subprocess.PIPE if input_bytes is not None else subprocess.DEVNULL,
        stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=os.name == "posix",
    ) as process:
        try:
            stdout, stderr = process.communicate(input=input_bytes, timeout=timeout)
        except subprocess.TimeoutExpired:
            # The macOS wrapper may still own a GUI child holding the pipes.
            if os.name == "posix":
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            else:
                process.kill()
            stdout, stderr = process.communicate(timeout=10)
            log.write(stdout)
            log.write(stderr)
            raise
        log.write(stdout)
        log.write(stderr)
    if process.returncode:
        raise subprocess.CalledProcessError(process.returncode, command, stdout, stderr)
    return stdout


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


def windows_rdp_child(parent_pid, helper, expected_digest, software_gl=False):
    """Observe the Windows process image while its loopback connection is open."""
    powershell = Path(os.environ["SystemRoot"]) / "System32/WindowsPowerShell/v1.0/powershell.exe"
    # Windows PowerShell 5.1 has no -AsArray parameter. Wrap the query results
    # and pass them as a single value to ConvertTo-Json.
    modules = (f"$modules = @((Get-Process -Id {parent_pid}).Modules | Select-Object -ExpandProperty FileName); "
               if software_gl else "$modules = @(); ")
    command = (
        "[Console]::OutputEncoding = [Text.Encoding]::UTF8; "
        f"$children = @(Get-CimInstance Win32_Process -Filter 'ParentProcessId = {parent_pid}' "
        "| Select-Object ProcessId,ExecutablePath); "
        + modules + "ConvertTo-Json -InputObject @{children=$children; modules=$modules} -Compress"
    )
    completed = subprocess.run(
        [powershell, "-NoProfile", "-NonInteractive", "-Command", command],
        check=True, capture_output=True, text=True, encoding="utf-8-sig", timeout=15,
    )
    observed = json.loads(completed.stdout)
    if software_gl:
        actual = {Path(path) for path in observed["modules"]}
        if not {helper.parent / name for name in windows_software_gl_files()} <= actual:
            raise ValueError("The GUI did not load the app-local Windows software GL fixture.")
    for child in observed["children"]:
        if child["ExecutablePath"]:
            actual = Path(child["ExecutablePath"])
            if actual == helper and sha256(actual) == expected_digest:
                return child["ProcessId"]
    raise ValueError("RDP session did not start the verified adjacent Windows package helper.")


def linux_children(pid):
    children = set()
    try:
        for thread in Path(f"/proc/{pid}/task").iterdir():
            try:
                children.update(map(int, (thread / "children").read_text().split()))
            except FileNotFoundError:
                continue
    except FileNotFoundError:
        pass
    return children


def appimage_gui(parent, image):
    """Identify the actual GUI under this invocation's private runtime extraction."""
    pending = [parent]
    seen = set()
    matches = []
    while pending:
        pid = pending.pop()
        if pid in seen:
            continue
        seen.add(pid)
        pending.extend(linux_children(pid))
        proc = Path(f"/proc/{pid}")
        try:
            path = Path(os.readlink(proc / "exe"))
            relative = path.relative_to(image["tmp"])
        except (OSError, ValueError):
            continue
        if (len(relative.parts) == 5 and re.fullmatch(r"appimage_extracted_[0-9a-f]{32}", relative.parts[0])
                and relative.parts[1:] == ("usr", "lib", "termcraft", "harbor-workspace")
                and os.getpgid(pid) == parent and sha256(proc / "exe") == image["gui_sha256"]):
            matches.append((pid, image["tmp"] / relative.parts[0]))
    if len(matches) != 1:
        raise ValueError("Expected one verified GUI in the owned AppImage process tree")
    return matches[0]


def appimage_libraries(pid, directory, image):
    private = directory / "usr/lib/termcraft-appimage"
    libraries = image["metadata"]["libraries"]
    found = set()
    for line in Path(f"/proc/{pid}/maps").read_text().splitlines():
        fields = line.split(None, 5)
        if len(fields) < 6:
            continue
        mapped = Path(fields[5])
        if mapped.name in libraries:
            if mapped != private / mapped.name or sha256(mapped) != libraries[mapped.name]["sha256"]:
                raise ValueError("AppImage loaded an unexpected display-client library")
            found.add(mapped.name)
    backend = "libwayland-client.so.0" if image["wayland"] else "libX11.so.6"
    if backend not in found:
        raise ValueError("AppImage GUI did not map the private backend client")
    return sorted(found)


def require_extraction_cleanup(image):
    if any(image["tmp"].glob("appimage_extracted_*")):
        raise ValueError("AppImage runtime left its private extraction behind")


def stop_appimage(process, image):
    if process.poll() is None:
        try:
            gui, _ = appimage_gui(process.pid, image)
        except (OSError, ValueError):
            # The GUI may already be gone while its parent removes the AppDir.
            gui = None
        if gui is not None:
            try:
                os.kill(gui, signal.SIGTERM)
            except ProcessLookupError:
                pass
        try:
            process.wait(timeout=8)
        except subprocess.TimeoutExpired:
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=8)
    require_extraction_cleanup(image)


def initial_rdp_request(header, stream):
    length = int.from_bytes(header[2:], "big")
    if not 19 <= length <= 4096:
        raise ValueError("Invalid initial RDP request length")
    body = stream.read(length - 4)
    if (len(body) != length - 4 or body[0] != len(body) - 1 or body[1] != 0xe0
            or body[2:4] != b"\x00\x00" or body[-8] != 1 or body[-6:-4] != b"\x08\x00"):
        raise ValueError("Missing X.224 connection request or RDP negotiation request")
    return header + body


def rdp_session_probe(executable, helper, helper_digest, isolated, environment, evidence, software_gl=False, appimage=None):
    host = platform.system()
    windows_workspace = host == "Windows"
    workspace = (Path(environment["LOCALAPPDATA"]) / "Harbor" if windows_workspace
                 else isolated / "RDP test workspace")
    workspace.mkdir(parents=True)
    config = workspace / "workspace.json"
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen(1)
        listener.settimeout(0.25)
        profile = {
            "id": "package-rdp-test", "name": "Packaged helper startup", "protocol": "RDP",
            "host": "127.0.0.1", "user": "package-test", "port": listener.getsockname()[1],
        }
        preferences = {"font_size": 19.0}
        config.write_text(json.dumps({"sessions": [profile], "preferences": preferences}))
        args = [executable, *(["--appimage-extract-and-run"] if appimage else [])]
        if not windows_workspace:
            args.extend(["--data-dir", workspace])
        args.extend(["--session", "package-rdp-test"])
        with (evidence / "rdp-session.log").open("xb") as log:
            app = subprocess.Popen(
                args,
                cwd=isolated, env=environment, stdin=subprocess.DEVNULL,
                stdout=log, stderr=subprocess.STDOUT, start_new_session=bool(appimage),
            )
            try:
                deadline = time.monotonic() + 30
                while True:
                    try:
                        connection, _ = listener.accept()
                        break
                    except socket.timeout:
                        if app.poll() is not None:
                            details = (evidence / "rdp-session.log").read_text(errors="replace").strip()
                            raise ValueError(f"Packaged GUI exited before RDP negotiation ({app.returncode}): {details}")
                        if time.monotonic() >= deadline:
                            raise TimeoutError("Packaged GUI did not start RDP negotiation within 30 seconds.")
                with connection:
                    connection.settimeout(10)
                    with connection.makefile("rb") as incoming:
                        header = incoming.read(4)
                        if len(header) != 4 or header[:2] != b"\x03\x00":
                            raise ValueError("Packaged RDP session did not send a TPKT header.")
                        if appimage:
                            request = initial_rdp_request(header, incoming)
                            appimage["negotiation_sha256"] = hashlib.sha256(request).hexdigest()
                            log.write(b"\n[AppImage loopback X.224/RDP negotiation] " + request.hex().encode("ascii") + b"\n")
                            log.flush()
                        if host == "Windows":
                            windows_rdp_child(app.pid, helper, helper_digest, software_gl)
                        elif host == "Linux":
                            gui_pid = app.pid
                            if appimage:
                                gui_pid, appdir = appimage_gui(app.pid, appimage)
                                appimage["mapped"] = appimage_libraries(gui_pid, appdir, appimage)
                            children = linux_children(gui_pid)
                            if not any(
                                linux_rdp_helper_matches(Path(f"/proc/{pid}/exe"), helper_digest)
                                for pid in children
                            ):
                                raise ValueError("RDP session did not start the verified package helper.")
                        elif platform.system() == "Darwin":
                            if not macos_rdp_helper_matches(app.pid, helper_digest):
                                raise ValueError("RDP session did not start the verified macOS helper.")
            finally:
                if appimage:
                    stop_appimage(app, appimage)
                elif app.poll() is None:
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


def successful_wayland_connect(trace, socket_path):
    expected = re.escape(os.fsencode(socket_path))
    pattern = rb'^[ \t]*[0-9]+ +connect\([0-9]+, \{sa_family=AF_UNIX, sun_path="' + expected + rb'"\}, [0-9]+\) += 0$'
    return next((line for line in trace.splitlines() if re.fullmatch(pattern, line)), None)


def smoke(archive, evidence, display, software_gl=None, *, native_wayland=None, appimage_core=None):
    from PIL import Image, ImageStat

    evidence.mkdir(parents=True, exist_ok=False)
    with tempfile.TemporaryDirectory(prefix="TermCraft startup ") as temporary:
        # Resolve the trusted temp root (e.g. macOS /var -> /private/var);
        # the extractor still rejects symlinks in the destination below it.
        isolated = Path(temporary).resolve()
        package_dir = isolated / "package files"
        package_dir.mkdir()
        appimage = None
        if appimage_core:
            spec = importlib.util.spec_from_file_location("appimage", Path(__file__).with_name("package-appimage.py"))
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            info, appdir = module.verify_image(appimage_core, archive, archive.with_name(
                archive.name.removesuffix(".AppImage") + "-AppImage-source.tar.gz"), package_dir)
            root = appdir / module.PORTABLE
            local_image = isolated / archive.name
            shutil.copyfile(archive, local_image)
            if sha256(local_image) != sha256(archive):
                raise ValueError("Relocated AppImage differs from the inspected file")
            local_image.chmod(0o700)
            appimage = {"metadata": info, "tmp": isolated / "runtime", "wayland": bool(native_wayland),
                        "gui_sha256": sha256(root / "harbor-workspace")}
            appimage["tmp"].mkdir(mode=0o700)
        else:
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
            **(native_wayland["environment"] if native_wayland else os.environ), "PATH": "", "HOME": str(isolated), "USERPROFILE": str(isolated),
            "XDG_CONFIG_HOME": str(isolated / "config"), "XDG_DATA_HOME": str(isolated / "data"),
            "LOCALAPPDATA": str(isolated / "local"),
        }
        for variable in tuple(environment):
            if variable.startswith(("LD_", "DYLD_", "CARGO_", "TERMCRAFT_")):
                environment.pop(variable)
        if appimage:
            executable = local_image
            for key in tuple(environment):
                if key.startswith("APPIMAGE") or key in {"APPDIR", "ARGV0", "TARGET_APPIMAGE", "NO_CLEANUP"}:
                    environment.pop(key)
            environment.update(TMPDIR=str(appimage["tmp"]), XDG_CACHE_HOME=str(isolated / "cache"),
                               XDG_STATE_HOME=str(isolated / "state"))
            if not native_wayland:
                environment["XDG_RUNTIME_DIR"] = str(isolated / "xdg-runtime")
                Path(environment["XDG_RUNTIME_DIR"]).mkdir(mode=0o700)
        if display:
            environment["DISPLAY"] = display
            environment.pop("WAYLAND_DISPLAY", None)
            environment.pop("WAYLAND_SOCKET", None)
        if software_gl is not None:
            if host != "windows" or manifest["architecture"] != "x86_64":
                raise ValueError("This software GL fixture requires a Windows x64 package.")
            for name, digest in windows_software_gl_files().items():
                source, target = software_gl / name, bindir / name
                if target.exists() or sha256(source) != digest:
                    raise ValueError(f"Cannot stage verified software GL fixture: {name}")
                shutil.copyfile(source, target)
                if sha256(target) != digest:
                    raise ValueError(f"Staged software GL fixture changed: {name}")
            environment["GALLIUM_DRIVER"] = "llvmpipe"
        for name, program, args in (
            ("help", executable, [*(["--appimage-extract-and-run"] if appimage else []), "--help"]),
            ("rdp-helper", helper, []),
        ):
            result = run([program, *args], isolated, environment, evidence / f"{name}.log")
            if appimage and name == "help":
                require_extraction_cleanup(appimage)
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
        launch = [launcher, *(["--appimage-extract-and-run"] if appimage else []), "--data-dir", isolated / "workspace data", "--screenshot", screenshot]
        if native_wayland:
            socket_path = native_wayland["socket"]
            tracer = shutil.which("strace")
            if (host != "linux" or tracer is None or any(key in environment for key in ("DISPLAY", "WAYLAND_SOCKET"))
                    or Path(environment["XDG_RUNTIME_DIR"]) / environment["WAYLAND_DISPLAY"] != socket_path):
                raise ValueError("Native Wayland needs strace and the owned socket without X11 fallback")
            trace = isolated / "wayland-connect.trace"
            launch = [tracer, "--kill-on-exit", "-f", "-e", "trace=connect", "-s", "256", "-o", trace, *launch]
        run(launch, isolated, environment, evidence / "startup.log")
        if appimage:
            require_extraction_cleanup(appimage)
        wayland = {}
        if native_wayland:
            connected = successful_wayland_connect(trace.read_bytes(), socket_path)
            if not connected:
                raise ValueError("The screenshot process did not connect to the owned Weston socket")
            with (evidence / "startup.log").open("ab") as output:
                output.write(b"\n[successful native Wayland connect]\n" + connected + b"\n")
            wayland = {"source_sha": native_wayland["source_sha"], "weston": {
                "backend": "headless", "socket": str(socket_path), "pid": native_wayland["weston_pid"],
                "socket_inode": native_wayland["socket_inode"], "runtime_mode": "0700",
                "display_absent": True, "wayland_socket_absent": True,
            }}
        with Image.open(screenshot) as image:
            if image.width > 8192 or image.height > 8192:
                raise ValueError("Screenshot exceeds the size limit")
            stats = ImageStat.Stat(image.convert("RGB"))
            if image.width < 640 or image.height < 400 or max(stats.stddev) < 2:
                raise ValueError("Screenshot is too small or blank.")
            size = image.size
        helper_digest = manifest["files"][helper.relative_to(root).as_posix()]
        rdp_session_probe(executable, helper, helper_digest, isolated, environment, evidence,
                          software_gl is not None, appimage)
        (evidence / "verification.json").write_text(json.dumps({
            **wayland,
            **({"appimage": {"mode": "extract-and-run", "core_sha256": info["core"]["sha256"],
                              "runtime_sha256": info["runtime"]["sha256"], "source_sha256": info["corresponding_source"]["sha256"],
                              "highest_required_glibc": info["highest_required_glibc"], "gui_sha256": appimage["gui_sha256"],
                              "helper_sha256": helper_digest, "mapped_private_libraries": appimage["mapped"],
                              "negotiation_sha256": appimage["negotiation_sha256"],
                              "runtime_reaped_and_clean": True}} if appimage else {}),
            "archive": archive.name, "archive_sha256": sha256(archive),
            "manifest_schema": manifest["schema_version"], "platform": host,
            "display_backend": (
                "Wayland" if environment.get("WAYLAND_DISPLAY") else "X11"
            ) if host == "linux" else host,
            "start_environment": "relocated package, empty PATH, isolated workspace, existing native display",
            "screen_size": size, "rdp_helper_wire_version": 1,
            "rdp_session": "Received initial RDP negotiation on a loopback-only fixture; workspace preserved",
            "rdp_workspace": (
                "isolated LOCALAPPDATA/Harbor default" if host == "windows" else "explicit isolated workspace"
            ),
            "rdp_helper_child_location_verified": host in ("linux", "windows", "macos"),
            "windows_software_gl_fixture": windows_software_gl_files() if software_gl else None,
        }, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", type=Path)
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--display", help="Optional X11 display, e.g. :99")
    parser.add_argument("--windows-software-gl", type=Path,
                        help="Directory with the pinned x64 Mesa 26.3.1 test DLLs")
    parser.add_argument("--appimage-core", type=Path, help="Independently verified core tarball for outer AppImage qualification")
    args = parser.parse_args()
    try:
        smoke(args.archive.resolve(strict=True), args.evidence.resolve(), args.display,
              args.windows_software_gl, appimage_core=args.appimage_core)
    except (OSError, ValueError, ImportError, subprocess.SubprocessError) as error:
        parser.exit(1, f"Startup verification failed: {error}\n")


if __name__ == "__main__":
    main()
