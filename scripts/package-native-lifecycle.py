#!/usr/bin/env python3
"""Qualify actual two-version packages on hosted disposable native desktops."""

import argparse
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import platform
import plistlib
import secrets
import shutil
import subprocess
import sys
import tarfile
import tempfile

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[1]


def load(path):
    spec = importlib.util.spec_from_file_location(path.stem, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


DELIVERY = load(ROOT / "scripts/release-delivery.py")
STARTUP = load(ROOT / "scripts/package-check-startup.py")
require = DELIVERY.require
read_json = DELIVERY.read_json
write_json = DELIVERY.write_json
sha256 = DELIVERY.sha256


def digest(data):
    return hashlib.sha256(data).hexdigest()


previous_version = DELIVERY.previous_version
patched_version_bytes = DELIVERY.patched_version_bytes


def patch_version(root, version, baseline):
    patch = {}
    for name in ("Cargo.toml", "Cargo.lock"):
        path = root / name
        before = path.read_bytes()
        after = patched_version_bytes(name, before, version, baseline)
        path.write_bytes(after)
        patch[name] = {"before": digest(before), "after": digest(after)}
    return patch


def command(*argv, timeout=120, **kwargs):
    return subprocess.run(argv, check=True, timeout=timeout, **kwargs)


def capture(*argv, **kwargs):
    return command(*argv, stdout=subprocess.PIPE, text=True, encoding="utf-8", **kwargs).stdout.strip()


def fixture_binary():
    result = capture("cargo", "test", "--locked", "--test", "package_workspace_fixture", "--no-run",
                     "--message-format=json", timeout=2400, cwd=ROOT,
                     env={**os.environ, "CARGO_BUILD_JOBS": "2"})
    records = [json.loads(line) for line in result.splitlines()]
    paths = [record["executable"] for record in records if record.get("reason") == "compiler-artifact"
             and record.get("executable") and record["target"]["name"] == "package_workspace_fixture"]
    require(len(paths) == 1, "Expected one compiled workspace fixture")
    return Path(paths[0]).resolve(strict=True)


def persistent(fixture_root):
    return {name: sha256(fixture_root / name) for name in DELIVERY.FIXTURE_FILES}


def fixture_input():
    return [secrets.token_hex(32), secrets.token_hex(32)]


def encode_fixture_input(values):
    return json.dumps(values).encode("ascii")


def fixture(binary, root, mode, log, values):
    output = STARTUP.run([binary, "--ignored", "--exact", "qualification_workspace", "--nocapture"],
                         root, {**os.environ, "PACKAGE_FIXTURE_ROOT": str(root),
                                "PACKAGE_FIXTURE_MODE": mode}, log, input_bytes=encode_fixture_input(values))
    require(DELIVERY.FIXTURE_PROOF in output, "Workspace fixture did not verify native library state")
    return persistent(root)


def isolated_env(work):
    environment = {key: value for key, value in os.environ.items()
                   if not key.startswith(("TERMCRAFT_", "LD_", "DYLD_"))}
    for name, directory in (("HOME", "home"), ("USERPROFILE", "home"), ("XDG_CONFIG_HOME", "config"),
                            ("XDG_DATA_HOME", "data"), ("XDG_CACHE_HOME", "cache"),
                            ("APPDATA", "roaming"), ("LOCALAPPDATA", "local")):
        path = work / directory
        path.mkdir(exist_ok=True)
        environment[name] = str(path)
    if sys.platform.startswith("linux"):
        require(environment.get("DISPLAY"),
                "Run installed Linux lifecycle in the owned Xvfb display")
        environment.pop("WAYLAND_SOCKET", None)
        environment.pop("WAYLAND_DISPLAY", None)
    return environment


def manifest_path(root, host):
    return root / ("Contents/Resources/PACKAGE-MANIFEST.json" if host == "macos" else "PACKAGE-MANIFEST.json")


def app_path(root, host):
    return root / DELIVERY.app_member(host)


def expanded(archive, directory, host):
    directory.mkdir()
    STARTUP.extract(archive, directory)
    (root,) = directory.iterdir()
    found, manifest = STARTUP.verify(root, manifest_path(root, host))
    require(found == host, "Package is not native to the requested lifecycle")
    return root, manifest


def verify_installed(root, host, role):
    path = manifest_path(root, host)
    require(sha256(path) == role["manifest_sha256"], "Installed manifest differs from the archive")
    STARTUP.verify(root, path)


def embedded_version(executable, version, environment):
    output = capture(executable, "--version", env=environment, cwd=executable.parent, timeout=30)
    require(output == f"TermCraft {version}", f"Compiled application version differs: {executable}")
    return output


def package_role(archive, root, manifest, environment):
    program = app_path(root, manifest["platform"])
    return {
        "archive": archive.name, "archive_sha256": sha256(archive),
        "manifest_sha256": sha256(manifest_path(root, manifest["platform"])),
        "snapshot_sha256": manifest["build"]["snapshot_sha256"],
        "app_sha256": sha256(program), "version": manifest["version"],
        "embedded_version": embedded_version(program, manifest["version"], environment),
    }


class Lifecycle:
    def __init__(self, evidence, work, fixtures, binary, environment, roles):
        self.evidence, self.work, self.fixtures = evidence, work, fixtures
        self.binary, self.environment, self.roles = binary, environment, roles
        self.fixture_values = fixture_input()
        self.original = fixture(binary, fixtures, "seed", evidence / "fixture-seed.log", self.fixture_values)
        self.stages = []

    def stage(self, name, role, paths, *, program=None, launcher=None, registration=None):
        observed = {"stage": name, "role": role, "paths": paths}
        if program is not None:
            initial = self.roles[role]
            executable = program.resolve(strict=True)
            require(sha256(executable) == initial["app_sha256"], "Installed app bytes differ from the archive")
            observed.update(executable=str(executable), launcher=[str(part) for part in launcher],
                            app_sha256=sha256(executable),
                            embedded_version=embedded_version(executable, initial["version"], self.environment),
                            registration=registration)
            for label, workspace in (("workspace", "workspace"), ("malformed", "malformed workspace")):
                stem = f"{name}-{label}"
                png, output = self.evidence / f"{stem}.png", self.evidence / f"{stem}.log"
                require(not os.path.lexists(png), "Screenshot already exists")
                argv = [*launcher, "--data-dir", self.fixtures / workspace, "--screenshot", png]
                STARTUP.run(argv, Path(registration.get("working_directory", self.work)), self.environment, output)
                require(png.is_file() and not png.is_symlink(), "Launch did not create a regular screenshot")
                observed[label] = {"name": png.name, "sha256": sha256(png),
                                   "size": DELIVERY.png_observation(png)}
                write_json(self.evidence / f"{stem}-launch.json", {
                    "stage": name, "role": role, "executable": str(executable),
                    "executable_sha256": initial["app_sha256"], "argv": [str(part) for part in argv],
                    "exit_code": 0, "screenshot_preexisting": False, "screenshot": observed[label],
                    "output_sha256": sha256(output),
                })
        check = self.evidence / f"{name}-fixture.log"
        observed["preserved"] = fixture(self.binary, self.fixtures, "verify", check, self.fixture_values)
        require(observed["preserved"] == self.original, f"Persistent fixture changed at {name}")
        self.stages.append(observed)


def linux(life, roots, debs):
    installed = Path("/opt/termcraft")
    link = Path("/usr/bin/termcraft")
    records = {}
    for role, deb in debs.items():
        version = capture("dpkg-deb", "-f", deb, "Version")
        require(version == life.roles[role]["version"], "Debian package version differs")
        records[role] = {"name": deb.name, "sha256": sha256(deb), "version": version}
    command("dpkg", "--compare-versions", records["candidate"]["version"], "gt", records["baseline"]["version"])
    state = subprocess.run(["dpkg-query", "-W", "termcraft"], capture_output=True, timeout=30)
    require(state.returncode == 1, "The disposable runner already has a TermCraft package record")
    paths = tuple(map(Path, sorted(DELIVERY.DEBIAN_PATHS)))
    require(not any(os.path.lexists(path) for path in paths), "Debian fixture collides with existing owned paths")
    apt = ("sudo", "-n", "apt-get", "-y", "--no-install-recommends")
    try:
        for name, role, reinstall in (("deb-install-baseline", "baseline", False),
                                      ("deb-upgrade-candidate", "candidate", False),
                                      ("deb-reinstall-candidate", "candidate", True)):
            command(*apt, *(["--reinstall"] if reinstall else []), "install", debs[role], timeout=300)
            require(capture("dpkg-query", "-W", "-f=${Status} ${Version}", "termcraft") ==
                    f'install ok installed {records[role]["version"]}', "Debian installed version mismatch")
            require(not capture("dpkg", "-V", "termcraft"), "Debian installed file integrity check failed")
            program = app_path(installed, "linux")
            require(link.resolve(strict=True) == program, "Debian launcher target differs")
            verify_installed(installed, "linux", life.roles[role])
            life.stage(name, role, {"package": str(installed)}, program=program, launcher=[link],
                       registration={"target": str(link), "version": records[role]["version"]})
    finally:
        command(*apt, "purge", "termcraft", timeout=300)
    require(not any(os.path.lexists(path) for path in paths), "Debian purge left owned paths")
    life.stage("deb-purge", "candidate", {"removed": sorted(map(str, paths))})

    source = roots["candidate"]
    data = Path(life.environment["XDG_DATA_HOME"])
    defaults = Path(life.environment["XDG_CONFIG_HOME"]) / "mimeapps.list"
    defaults.write_text("[Default Applications]\ntext/plain=unrelated.desktop;\n", encoding="utf-8")
    menus = data / "applications"
    menus.mkdir()
    unrelated = menus / "unrelated.desktop"
    unrelated.write_text("[Desktop Entry]\nName=Unrelated\nType=Application\nExec=false\n", encoding="utf-8")
    before = {"mimeapps.list": sha256(defaults), "unrelated.desktop": sha256(unrelated)}
    portable = load(source / "desktop-integration.py")
    integration = portable.DesktopIntegration(data, source)
    try:
        integration.install_user()
        owned = list(integration._owned())
        moved = source.rename(source.with_name("Original source moved offline"))
        require(not source.exists() and integration.status(), "Portable install depends on its extracted source")
        receipt = integration.manifest
        launcher = data / "termcraft/desktop/launch.py"
        payload = launcher.parent / "payload"
        program = app_path(payload, "linux")
        verify_installed(payload, "linux", life.roles["candidate"])
        entry = data / "applications/org.termcraft.TermCraft.desktop"
        require(entry.read_bytes() == portable.entry_for(launcher.parent), "Portable desktop entry target differs")
        life.stage("desktop-start-candidate", "candidate", {"package": str(payload), "original_absent": str(source)},
                   program=program, launcher=[sys.executable, launcher],
                   registration={"target": str(entry), "receipt": str(receipt)})
    finally:
        integration.remove_user()
    removed = sorted([str(data / name) for name in owned] + [str(receipt)])
    require(not any(os.path.lexists(name) for name in removed) and moved.is_dir(), "Portable uninstall left owned files")
    after = {"mimeapps.list": sha256(defaults), "unrelated.desktop": sha256(unrelated)}
    require(after == before, "Portable uninstall changed unrelated desktop settings")
    life.stage("desktop-remove", "candidate", {"removed": removed, "unrelated_before": before,
                                                "unrelated_after": after})
    return records


def macos(life, roots):
    require(platform.machine() == "arm64", "Native macOS lifecycle requires ARM64")
    applications = Path.home() / "Applications"
    applications.mkdir(exist_ok=True)
    receipt = life.work / "launchservices-receipt.json"
    with tempfile.TemporaryDirectory(prefix="TermCraft qualification ", dir=applications) as scratch:
        parent = Path(scratch).resolve()
        installation, relocated = parent / "TermCraft.app", parent / "Relocated 'QA' 100%.app"
        old = load(roots["baseline"] / "Contents/Resources/desktop-integration.py")
        class Integration(old.DesktopIntegration):
            def _run(self, *args):
                return capture(old.REGISTER, *args, timeout=60)

        original = Integration(roots["baseline"], receipt)._registered()
        attempted = set()
        integration = None
        try:
            for name, role, target in (("mac-install-baseline", "baseline", installation),
                                       ("mac-replace-candidate", "candidate", installation),
                                       ("mac-relocate-candidate", "candidate", relocated)):
                if target == relocated:
                    installation.rename(relocated)
                else:
                    if target.exists():
                        shutil.rmtree(target)
                    shutil.copytree(roots[role], target, symlinks=True)
                verify_installed(target, "macos", life.roles[role])
                info = plistlib.loads((target / "Contents/Info.plist").read_bytes())
                require(all(info[key] == life.roles[role]["version"] for key in
                            ("CFBundleVersion", "CFBundleShortVersionString")), "Bundle version differs")
                integration = Integration(target, receipt)
                attempted.add(str(target))
                integration.install_user()
                require(integration.status(), "Owned LaunchServices registration is missing")
                program = app_path(target, "macos")
                life.stage(name, role, {"package": str(target)}, program=program,
                           launcher=[program.with_name("termcraft-desktop-launcher")],
                           registration={"target": str(target), "receipt": str(receipt)})
        finally:
            if integration:
                try:
                    integration.remove_user()
                finally:
                    registered = integration._registered()
                    failures = []
                    for target in sorted(attempted & registered):
                        try:
                            command(old.REGISTER, "-u", target, timeout=30)
                        except subprocess.SubprocessError as error:
                            failures.append(str(error))
                    require(not failures, "Targeted LaunchServices cleanup failed: " + "; ".join(failures))
        registered = integration._registered()
        require(not attempted & registered and original <= registered and not receipt.exists(),
                "LaunchServices removal left owned registration or changed unrelated registrations")
        shutil.rmtree(relocated)
        life.stage("mac-unregister-remove", "candidate",
                   {"removed": sorted(attempted | {str(receipt)}), "unrelated_registrations": len(original),
                    "unrelated_before": digest(json.dumps(sorted(original)).encode()),
                    "unrelated_after": digest(json.dumps(sorted(registered & original)).encode())})


def windows(life, args, archives):
    # The installer uses actual KnownFolders, not the scratch environment.
    records = life.work / "windows-stage-records.json"
    command("pwsh", "-NoProfile", "-File", ROOT / "tests/windows-package-lifecycle.ps1",
            "-Archive", archives["candidate"], "-PreviousArchive", archives["baseline"],
            "-UsePackageVersions", "-SoftwareGlDirectory", args.software_gl,
            "-QualificationState", life.work / "windows-state.json", "-StageRecords", records,
            input=encode_fixture_input(life.fixture_values), timeout=1200)
    life.stages.extend(read_json(records))


def windows_stage(args):
    state = read_json(args.state)
    lifecycle = object.__new__(Lifecycle)
    for name in ("evidence", "work", "fixtures", "binary"):
        setattr(lifecycle, name, Path(state[name]))
    lifecycle.fixture_values = json.load(sys.stdin)
    lifecycle.environment = isolated_env(lifecycle.work)
    lifecycle.original, lifecycle.roles, lifecycle.stages = state["original"], state["roles"], []
    paths = read_json(args.paths)
    registration = paths.pop("registration", None)
    if args.program:
        lifecycle.environment["GALLIUM_DRIVER"] = "llvmpipe"
    lifecycle.stage(args.name, args.role, paths, program=args.program,
                    launcher=[args.program] if args.program else None, registration=registration)
    write_json(args.result, lifecycle.stages[0])


def qualify(args):
    host = args.platform
    require(os.environ.get("GITHUB_ACTIONS") == "true" and os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted"
            and args.disposable_runner, "Native install qualification requires a disposable GitHub-hosted runner")
    require(sys.platform == {"linux": "linux", "macos": "darwin", "windows": "win32"}[host],
            "Cross-platform lifecycle qualification is unsupported")
    if host == "linux":
        require(os.getuid() != 0, "Never run native GUI qualification as root")
    elif host == "windows":
        require(args.software_gl is not None, "Windows qualification requires verified software GL")
    DELIVERY.verify_checkout(ROOT, args.source, args.version)
    old_version = previous_version(args.version)
    args.evidence.mkdir(parents=True)
    with tempfile.TemporaryDirectory(prefix="termcraft-lifecycle-", dir=os.environ["RUNNER_TEMP"]) as temp:
        work = Path(temp).resolve()
        baseline = work / "src"
        baseline.mkdir()
        snapshot = work / "source.tar"
        command("git", "-C", ROOT, "archive", "--format=tar", f"--output={snapshot}", args.source)
        with tarfile.open(snapshot) as archive:
            archive.extractall(baseline, filter="data")
        patch = patch_version(baseline, args.version, old_version)
        private = work / "private"
        command(sys.executable, baseline / "scripts/package-release.py", "--output", private,
                "--build-root", args.build_root, timeout=3000)
        archives = {"candidate": args.dist / DELIVERY.portable_name(host, args.version),
                    "baseline": private / DELIVERY.portable_name(host, old_version)}
        roots, roles = {}, {}
        environment = isolated_env(work)
        for role, archive in archives.items():
            root, manifest = expanded(archive, work / role, host)
            expect = args.version if role == "candidate" else old_version
            require(manifest["version"] == expect and manifest["profile"] == "release"
                    and manifest["build"]["provenance"] == "local locked release build",
                    "Lifecycle versions must be independent locked release builds")
            roots[role], roles[role] = root, package_role(archive, root, manifest, environment)
            if role == "baseline":
                shutil.copyfile(manifest_path(root, host), args.evidence / "baseline-manifest.json")
        require(all(roles["baseline"][key] != roles["candidate"][key] for key in
                    ("archive_sha256", "snapshot_sha256", "app_sha256")), "Lifecycle roles must have distinct source and payloads")
        fixtures = work / "fixtures"
        fixtures.mkdir()
        life = Lifecycle(args.evidence, work, fixtures, fixture_binary(), environment, roles)
        debian = None
        if host == "linux":
            old_deb = DELIVERY.DEBIAN.package_debian(archives["baseline"], private)
            debian = linux(life, roots, {
                "baseline": old_deb, "candidate": args.dist / f"termcraft_{args.version}_amd64.deb"})
        elif host == "macos":
            macos(life, roots)
        else:
            write_json(work / "windows-state.json", {
                **{name: str(getattr(life, name)) for name in ("evidence", "work", "fixtures", "binary")},
                "roles": roles, "original": life.original,
            })
            windows(life, args, archives)
        write_json(args.evidence / "verification.json", {
            "schema": 2, "source_sha": args.source, "platform": host, "architecture": DELIVERY.TARGETS[host][0],
            "fixture_root": str(fixtures), "evidence_origin": str(args.evidence),
            "archive_sha256": roles["candidate"]["archive_sha256"], "version": args.version,
            "roles": roles, "private_version_patch": {"from": args.version, "to": old_version, "files": patch},
            "debian": debian, "fixture_before": life.original, "stages": life.stages,
            "fixture_source_sha256": sha256(ROOT / "tests/package_workspace_fixture.rs"),
        })


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    native = commands.add_parser("qualify")
    for name in ("source", "version", "platform"):
        native.add_argument("--" + name, required=True)
    for name in ("dist", "evidence", "build-root"):
        native.add_argument("--" + name, type=lambda p: Path(p).resolve(), required=True)
    native.add_argument("--software-gl", type=lambda p: Path(p).resolve())
    native.add_argument("--disposable-runner", action="store_true")
    stage = commands.add_parser("windows-stage")
    for name in ("state", "paths", "result"):
        stage.add_argument("--" + name, type=Path, required=True)
    stage.add_argument("--program", type=Path)
    stage.add_argument("--name", required=True)
    stage.add_argument("--role", required=True)
    args = parser.parse_args()
    try:
        (qualify if args.command == "qualify" else windows_stage)(args)
    except (OSError, ValueError, KeyError, subprocess.SubprocessError, tarfile.TarError) as error:
        parser.exit(1, f"Native lifecycle qualification failed: {error}\n")


if __name__ == "__main__":
    main()
