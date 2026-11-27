import hashlib
import importlib.util
import json
import os
from pathlib import Path, PurePosixPath, PureWindowsPath
import shutil
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import unittest
import zipfile
import zlib

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SCRIPT = ROOT / "scripts/release-delivery.py"


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load(path):
    spec = importlib.util.spec_from_file_location(path.stem, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_json(path, value):
    path.write_text(json.dumps(value), encoding="utf-8")


def png(blank=False, transparent=False):
    def chunk(kind, value):
        return struct.pack(">I", len(value)) + kind + value + struct.pack(">I", zlib.crc32(kind + value))

    def pixel(y):
        rgb = [24, 53, 20] if blank or y < 200 else [54, 90, 20]
        return bytes(rgb + ([0] if transparent else []))

    rows = (b"\x00" + pixel(y) * 640 for y in range(400))
    return (
        b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 640, 400, 8, 6 if transparent else 2, 0, 0, 0))
        + chunk(b"IDAT", zlib.compress(b"".join(rows))) + chunk(b"IEND", b"")
    )


def executable(platform, marker):
    if platform in {"linux", "linux-arm64"}:
        machine = 62 if platform == "linux" else 183
        header = b"\x7fELF\x02\x01\x01" + b"\0" * 9 + struct.pack("<HHIQQQIHHHHHH", 2, machine, 1, 0, 0, 0, 0, 64, 0, 0, 0, 0, 0)
    elif platform == "windows":
        header = b"MZ" + b"\0" * 58 + struct.pack("<I", 64) + b"PE\0\0" + struct.pack("<H", 0x8664)
    else:
        header = struct.pack("<IIII", 0xfeedfacf, 0x0100000c, 0, 2)
    return header + marker.encode()


class ReleaseDelivery(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.release = load(SCRIPT)
        scratch = tempfile.TemporaryDirectory()
        cls.addClassCleanup(scratch.cleanup)
        cls.fixture = Path(scratch.name) / "fixture"
        for name in (*cls.release.PACKAGER.source_inputs(ROOT), "tests/windows-software-gl.json", "tests/package_workspace_fixture.rs"):
            target = cls.fixture / name
            target.parent.mkdir(parents=True, exist_ok=True)
            shutil.copyfile(ROOT / name, target)
        # Exercise real Cargo metadata on a small path-only graph, independently
        # of network access or the application's large native build.
        for crate, license_text in {
            "release-rdp": b"RDP-only fixture license",
            "release-shared": b"Shared dependency license",
            "release-windows": b"Windows-only fixture license",
        }.items():
            folder = cls.fixture / "vendor" / crate
            (folder / "src").mkdir(parents=True)
            (folder / "src/lib.rs").touch()
            (folder / "Cargo.toml").write_text(
                f'[package]\nname = "{crate}"\nversion = "1.0.0"\n'
                'license = "MIT"\nrepository = "https://example.invalid/project"\n'
            )
            (folder / "LICENSE").write_bytes(license_text)

        for path, name, dependency_root, extra in (
            ("Cargo.toml", "harbor-workspace", "vendor",
             '[target."cfg(windows)".dependencies]\nrelease-windows = {path = "vendor/release-windows"}\n'),
            ("engines/rdp/Cargo.toml", "termcraft-rdp-engine", "../../vendor",
             'release-rdp = {path = "../../vendor/release-rdp"}\n'),
        ):
            version = tomllib.loads((ROOT / path).read_text())["package"]["version"]
            manifest = cls.fixture / path
            manifest.write_text(f'[package]\nname = "{name}"\nversion = "{version}"\nedition = "2021"\n'
                                f'[dependencies]\nrelease-shared = {{path = "{dependency_root}/release-shared"}}\n'
                                + extra)
            cls.lock(cls.fixture, path)
        for command in (("init", "-q"), ("add", "."), ("-c", "user.name=Release Fixture", "-c",
                         "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false",
                         "commit", "-qm", "Locked release fixture")):
            subprocess.run(["git", "-C", str(cls.fixture), *command], check=True)

    @staticmethod
    def lock(root, manifest):
        subprocess.run(["cargo", "generate-lockfile", "--offline", "--manifest-path", str(root / manifest)],
                       cwd=root, env={**os.environ, "CARGO_BUILD_JOBS": "2"}, check=True, capture_output=True)

    def setUp(self):
        self.debian = load(ROOT / "scripts/package-debian.py")
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.work = Path(tmp.name)
        self.root = self.fixture
        self.source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=self.root, text=True).strip()
        self.read_fixture_versions()

    def read_fixture_versions(self):
        self.version = tomllib.loads((self.root / "Cargo.toml").read_text())["package"]["version"]
        self.rdp_version = tomllib.loads((self.root / "engines/rdp/Cargo.toml").read_text())["package"]["version"]
        self.debian_name = f"termcraft_{self.version}_amd64.deb"

    def platform_bundle(self, platform, *, profile="release", blank=False, optional=False,
                        wrong_file=False, wrong_helper=False, mismatched_startup=False, notice_forgery=None):
        arch = "aarch64" if platform == "macos" else "x86_64"
        basename = f"TermCraft-{self.version}-{platform}-{arch}"
        case = Path(tempfile.mkdtemp(dir=self.work))
        stage = case / "tree"
        root = stage / ("TermCraft.app" if platform == "macos" else basename)
        resources = root / "Contents/Resources" if platform == "macos" else root
        binaries = root / "Contents/MacOS" if platform == "macos" else root
        files = {}
        suffix = ".exe" if platform == "windows" else ""
        programs = ["harbor-workspace" + suffix, "termcraft-rdp-engine" + suffix]

        def stage_file(path, raw):
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(raw)
            files[path.relative_to(root).as_posix()] = digest(path)

        originals = {
            "LICENSE": "LICENSE", "PRIVACY.md": "PRIVACY.md", "CREDITS.md": "CREDITS.md",
            "RUNTIME-MANIFEST.json": "packaging/runtime-manifest.json",
            "build-source/Cargo.lock": "Cargo.lock",
            "build-source/engines/rdp/Cargo.lock": "engines/rdp/Cargo.lock",
            "README.txt": f"packaging/{platform}/README.txt",
        }
        for src in (self.root / "vendor/noVNC").rglob("*"):
            if src.is_file():
                originals["build-source/noVNC/" + src.relative_to(self.root / "vendor/noVNC").as_posix()] = src.relative_to(self.root).as_posix()
        for name in ("Interface.ttf", "Terminal.ttf", "Interface-LICENSE", "Terminal-LICENSE",
                     "termcraft-mascot.png", "termcraft-logo.png"):
            originals["assets/" + name] = "assets/" + name
        if platform == "windows":
            originals["install-per-user.ps1"] = "packaging/windows/install-per-user.ps1"
            stage_file(resources / "TermCraft.ico", b"icon")
        else:
            originals["desktop-integration.py"] = f"packaging/{platform}/desktop-integration.py"
        if platform == "macos":
            import plistlib
            stage_file(root / "Contents/Info.plist", plistlib.dumps({
                **plistlib.loads((self.root / "packaging/macos/Info.plist").read_bytes()),
                "CFBundleVersion": self.version, "CFBundleShortVersionString": self.version,
            }))
            stage_file(resources / "TermCraft.icns", b"icon")
            programs.append("termcraft-desktop-launcher")
        for relative, source in originals.items():
            stage_file(resources / relative, (self.root / source).read_bytes())
        for name in programs:
            machine = "linux-arm64" if wrong_helper and name.startswith("termcraft-rdp") else platform
            stage_file(binaries / name, executable(machine, name))
            (binaries / name).chmod(0o755)
        crates = []
        fixture_notices = [("release-rdp", b"RDP-only fixture license"),
                           ("release-shared", b"Shared dependency license")]
        if platform == "windows":
            fixture_notices.append(("release-windows", b"Windows-only fixture license"))
        for name, data in fixture_notices:
            if name == "release-shared" and notice_forgery == "omission":
                continue
            leaf = "arbitrary-payload.exe" if notice_forgery == "path" else "LICENSE"
            notice = f"licenses/crates/{name}-1.0.0/{leaf}"
            stage_file(resources / notice, b"Substituted notice" if notice_forgery == "bytes" else data)
            crates.append({"name": name, "version": "1.0.0", "license": "MIT", "notice_paths": [notice],
                           "source": "vendored in build source", "repository": "https://example.invalid/project"})
        if optional:
            stage_file(resources / "browser/unapproved/Chromium.exe", b"unapproved")
        runtime = json.loads((self.root / "packaging/runtime-requirements.json").read_text())
        for key in ("platform_requirements", "optional_native_providers"):
            runtime[key] = runtime[key][platform]
        if platform != "linux":
            del runtime["linux_display_libraries"]
        vnc = json.loads((self.root / "vendor/noVNC/UPSTREAM.json").read_text())
        runtime.update(
            linked_libraries={name: {"libraries": ["libc.so.6"], "highest_required_glibc_symbol_version": "2.34"}
                              for name in programs},
            artifact_declarations="RUNTIME-MANIFEST.json",
            managed_vnc_browser={
                "release": vnc["release"], "source_url": vnc["source"], "source_archive_sha256": vnc["sha256"],
                "embedded_runtime": True, "source_and_notices": "build-source/noVNC",
            },
        )
        build_inputs = self.release.PACKAGER.source_inputs(self.root)
        build = {
            "provenance": "local locked release build",
            "snapshot_sha256": hashlib.sha256(json.dumps(build_inputs, sort_keys=True, separators=(",", ":")).encode()).hexdigest(),
            "source_inputs": build_inputs,
            "cargo": "cargo 1.96.1 (example)", "rustc": "rustc 1.96.1 (example)",
        }
        if platform == "macos":
            build["macos"] = {"architecture": "aarch64", "deployment_target": "14.0", "compiler": "Apple clang", "sdk_version": "15.0"}
        write_json(resources / "PACKAGE-MANIFEST.json", dict(
            schema_version=1, application="TermCraft", version=self.version, profile=profile,
            rdp_helper_version=self.rdp_version, platform=platform, architecture=arch, signing="unsigned",
            runtime=runtime, files=files, build=build,
            licenses={
                "project": (resources / "LICENSE").relative_to(root).as_posix(),
                "credits": (resources / "CREDITS.md").relative_to(root).as_posix(),
                "crate_notice_paths_relative_to": resources.relative_to(root).as_posix(),
                "crates": crates,
            },
        ))
        if wrong_file:
            (resources / "PRIVACY.md").write_bytes(b"tampered")
        output = case / "dist"
        output.mkdir()
        archive = output / (basename + (".tar.gz" if platform == "linux" else ".zip"))
        if platform == "linux":
            with tarfile.open(archive, "w:gz") as bundle:
                bundle.add(root, arcname=root.name)
            if not wrong_file and profile == "release":
                self.debian.package_debian(archive, output)
        else:
            with zipfile.ZipFile(archive, "w", zipfile.ZIP_DEFLATED) as bundle:
                for file in sorted(root.rglob("*")):
                    if file.is_file():
                        member = zipfile.ZipInfo(file.relative_to(stage).as_posix())
                        member.create_system = 3
                        member.external_attr = (stat.S_IFREG | (file.stat().st_mode & 0o777)) << 16
                        bundle.writestr(member, file.read_bytes())
        evidence = case / "startup-evidence"
        evidence.mkdir()
        (evidence / "startup.png").write_bytes(png(blank))
        (evidence / "help.log").write_bytes(b"TermCraft remote workspace\n")
        ready = b'{"Ready":{"version":1}}'
        (evidence / "rdp-helper.log").write_bytes(b"\1" + len(ready).to_bytes(4, "big") + ready)
        (evidence / "startup.log").touch()
        (evidence / "rdp-session.log").touch()
        write_json(evidence / "verification.json", {
            "archive": archive.name, "archive_sha256": "0" * 64 if mismatched_startup else digest(archive),
            "manifest_schema": 1, "platform": platform,
            "display_backend": "X11" if platform == "linux" else platform,
            "screen_size": [640, 400], "rdp_helper_wire_version": 1,
            "rdp_session": "Received initial RDP negotiation on a loopback-only fixture; workspace preserved",
            "rdp_helper_child_location_verified": True,
            "rdp_workspace": "isolated LOCALAPPDATA/Harbor default" if platform == "windows" else "explicit isolated workspace",
            "start_environment": "relocated package, empty PATH, isolated workspace, existing native display",
            "windows_software_gl_fixture": json.loads((self.root / "tests/windows-software-gl.json").read_text())["files"] if platform == "windows" else None,
        })
        if profile == "release" and not wrong_file:
            self.native_evidence(platform, case, archive, json.loads((resources / "PACKAGE-MANIFEST.json").read_text()))
        return case, archive

    def native_evidence(self, platform, case, archive, manifest):
        """Construct test-only inputs to exercise the aggregate validators."""
        lifecycle = case / "lifecycle-evidence"
        lifecycle.mkdir()
        r = self.release
        old = r.previous_version(self.version)
        fixture = {name: "5" * 64 for name in r.FIXTURE_FILES}
        build = manifest["build"]
        patch = {}
        for name in ("Cargo.toml", "Cargo.lock"):
            content = (self.root / name).read_bytes()
            patch[name] = {"before": hashlib.sha256(content).hexdigest(),
                           "after": hashlib.sha256(r.patched_version_bytes(name, content, self.version, old)).hexdigest()}
        old_inputs = {**build["source_inputs"], **{name: item["after"] for name, item in patch.items()}}
        old_snapshot = r.PACKAGER.source_digest(old_inputs)
        app = r.app_member(platform)
        old_manifest = lifecycle / "baseline-manifest.json"
        write_json(old_manifest, {**manifest, "version": old, "files": {**manifest["files"], app: "6" * 64},
                                  "build": {**build, "source_inputs": old_inputs, "snapshot_sha256": old_snapshot}})
        roles = {
            "baseline": {"archive": r.portable_name(platform, old), "archive_sha256": "7" * 64,
                         "manifest_sha256": digest(old_manifest), "snapshot_sha256": old_snapshot,
                         "app_sha256": "6" * 64, "version": old, "embedded_version": f"TermCraft {old}"},
            "candidate": {"archive": archive.name, "archive_sha256": digest(archive),
                          "manifest_sha256": digest(case / "tree" / ("TermCraft.app/Contents/Resources"
                               if platform == "macos" else archive.name.removesuffix(r.TARGETS[platform][2]))
                               / "PACKAGE-MANIFEST.json"),
                          "snapshot_sha256": build["snapshot_sha256"], "app_sha256": manifest["files"][app],
                          "version": self.version, "embedded_version": f"TermCraft {self.version}"},
        }
        debian = None
        installations, removals = {}, {}
        if platform == "linux":
            debian = {role: {"name": f"termcraft_{info['version']}_amd64.deb", "version": info["version"],
                             "sha256": digest(case / "dist" / self.debian_name) if role == "candidate" else "8" * 64}
                      for role, info in roles.items()}
            for name, role, _ in r.LIFECYCLE_STAGES[platform][:3]:
                installations[name] = ("/opt/termcraft", ["/usr/bin/termcraft"],
                                       {"target": "/usr/bin/termcraft", "version": roles[role]["version"]})
            removals["deb-purge"] = {"removed": sorted(r.DEBIAN_PATHS)}
            data = "/tmp/owned/data"
            desktop = data + "/termcraft/desktop"
            entry, receipt = data + "/applications/org.termcraft.TermCraft.desktop", desktop + "/owned.json"
            installations["desktop-start-candidate"] = (desktop + "/payload", ["/usr/bin/python3", desktop + "/launch.py"],
                                                         {"target": entry, "receipt": receipt})
            canaries = {"mimeapps.list": "9" * 64, "unrelated.desktop": "a" * 64}
            removals["desktop-remove"] = {
                "removed": [entry, receipt, desktop + "/launch.py", desktop + "/payload/PACKAGE-MANIFEST.json",
                            desktop + "/payload/harbor-workspace", data + "/icons/hicolor/256x256/apps/org.termcraft.TermCraft.png",
                            data + "/mime/packages/org.termcraft.TermCraft.xml"],
                "unrelated_before": canaries, "unrelated_after": canaries,
            }
        elif platform == "windows":
            def start(role):
                version = roles[role]["version"] + "-x86_64"
                path = rf"C:\Users\runner\Local\Programs\TermCraft\{version}"
                program = path + "\\" + app
                shortcut = rf"C:\Users\runner\Start Menu\TermCraft {version}.lnk"
                return path, [program], {"target": program, "working_directory": path, "shortcut": shortcut}
            baseline, candidate = start("baseline"), start("candidate")
            installations = {"windows-install-baseline": baseline, "windows-install-candidate": candidate,
                             "windows-reinstall-candidate": candidate}
            owned = lambda info: [info[0], info[2]["shortcut"]]
            removals = {"windows-remove-baseline": {"removed": owned(baseline), "retained": owned(candidate)},
                        "windows-remove-candidate": {"removed": owned(candidate)},
                        "windows-final-remove": {"removed": owned(candidate)}}
        else:
            install = "/Users/runner/Applications/Owned/TermCraft.app"
            move = "/Users/runner/Applications/Owned/Relocated 'QA' 100%.app"
            receipt = "/tmp/owned/receipt.json"
            for name, bundle in (("mac-install-baseline", install), ("mac-replace-candidate", install),
                                 ("mac-relocate-candidate", move)):
                installations[name] = (bundle, [bundle + "/Contents/MacOS/termcraft-desktop-launcher"],
                                       {"target": bundle, "receipt": receipt})
            removals["mac-unregister-remove"] = {"removed": [install, move, receipt], "unrelated_registrations": 1,
                                                  "unrelated_before": "f" * 64, "unrelated_after": "f" * 64}
        stages = []
        pixels = png()
        pure = PureWindowsPath if platform == "windows" else PurePosixPath
        (lifecycle / "fixture-seed.log").write_bytes(r.FIXTURE_PROOF)
        for name, role, starts in r.LIFECYCLE_STAGES[platform]:
            stage = {"stage": name, "role": role, "preserved": fixture}
            (lifecycle / f"{name}-fixture.log").write_bytes(r.FIXTURE_PROOF)
            if starts:
                path, launcher, registration = installations[name]
                stage.update(paths={"package": path}, launcher=launcher, registration=registration,
                             executable=str(pure(path) / app), app_sha256=roles[role]["app_sha256"],
                             embedded_version=roles[role]["embedded_version"])
                if name == "desktop-start-candidate":
                    stage["paths"]["original_absent"] = "/tmp/owned/extracted-source"
                for label in ("workspace", "malformed"):
                    screenshot = f"{name}-{label}.png"
                    (lifecycle / screenshot).write_bytes(pixels)
                    (lifecycle / f"{name}-{label}.log").touch()
                    stage[label] = {"name": screenshot, "sha256": digest(lifecycle / screenshot), "size": [640, 400]}
            else:
                stage["paths"] = removals[name]
            stages.append(stage)
        write_json(lifecycle / "verification.json", {
            "schema": 1, "source_sha": self.source, "platform": platform, "architecture": r.TARGETS[platform][0],
            "archive_sha256": digest(archive), "version": self.version, "roles": roles,
            "private_version_patch": {"from": self.version, "to": old, "files": patch},
            "debian": debian, "fixture_before": fixture, "stages": stages,
            "fixture_source_sha256": digest(self.root / "tests/package_workspace_fixture.rs"),
        })
        if platform == "linux":
            wayland = case / "wayland-evidence"
            shutil.copytree(case / "startup-evidence", wayland)
            obs = json.loads((wayland / "verification.json").read_text())
            socket = "/tmp/private/termcraft-release-wayland"
            obs.update(source_sha=self.source, display_backend="Wayland", weston={
                "backend": "headless", "socket": socket, "pid": 42, "socket_inode": 1,
                "runtime_mode": "0700", "display_absent": True, "wayland_socket_absent": True,
            })
            write_json(wayland / "verification.json", obs)
            (wayland / "startup.log").write_text(f'37 connect(4, {{sa_family=AF_UNIX, sun_path="{socket}"}}, 44) = 0\n')

    def candidate(self, platform, **kwargs):
        case, archive = self.platform_bundle(platform, **kwargs)
        candidate = case / ("candidate-" + {"linux": "linux-amd64", "windows": "windows-amd64", "macos": "macos-arm64"}[platform])
        self.release.stage_candidate(self.root, self.source, self.version, platform, case / "dist", case / "startup-evidence", candidate,
                                     case / "lifecycle-evidence", case / "wayland-evidence" if platform == "linux" else None)
        return candidate, archive

    def test_preflight_checks_all_three_package_versions_and_ref(self):
        for path in ("Cargo.toml", "engines/rdp/Cargo.toml", "engines/wayland/Cargo.toml"):
            target = self.work / path
            target.parent.mkdir(parents=True, exist_ok=True)
            name = {"Cargo.toml": "harbor-workspace", "engines/rdp/Cargo.toml": "termcraft-rdp-engine",
                    "engines/wayland/Cargo.toml": "termcraft-wayland-engine"}[path]
            target.write_text(f'[package]\nname = "{name}"\nversion = "1.2.3"\n')
        self.assertEqual(self.release.version_for(self.work, "push", "refs/tags/v1.2.3"), "1.2.3")
        self.assertFalse(self.release.publish_event("workflow_dispatch", "refs/tags/v1.2.3", "1.2.3"))
        self.assertTrue(self.release.publish_event("push", "refs/tags/v1.2.3", "1.2.3"))
        for tag in ("v01.2.3", "v1.2.2", "v1.2.3\n", "v1.2.3;false", "v1.2.3-rc.1", "v1.2.3+build"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                self.release.version_for(self.work, "push", "refs/tags/" + tag)
        with self.assertRaises(ValueError):
            self.release.version_for(self.work, "push", "refs/heads/v1.2.3")
        helper = self.work / "engines/wayland/Cargo.toml"
        helper.write_text('[package]\nname = "termcraft-wayland-engine"\nversion = "1.2.4"\n')
        self.assertEqual(self.release.version_for(self.work, "workflow_dispatch", "refs/heads/main"), "1.2.3")
        helper.write_text(helper.read_text().replace("1.2.4", "1.02.4"))
        with self.assertRaisesRegex(ValueError, "version"):
            self.release.version_for(self.work, "workflow_dispatch", "refs/heads/main")

    def test_candidate_hashes_portable_debian_and_startup_observations(self):
        destination, archive = self.candidate("linux")
        value = json.loads((destination / "candidate.json").read_text())
        self.assertEqual(value["artifacts"][archive.name], digest(archive))
        self.assertEqual(value["artifacts"][self.debian_name],
                         digest(destination / "dist" / self.debian_name))
        self.assertEqual(value["startup"]["observation"]["archive_sha256"], digest(archive))
        self.assertEqual(value["startup"]["files"]["startup.png"], digest(destination / "startup-evidence/startup.png"))
        self.assertEqual(value["source_sha"], self.source)
        self.assertEqual(value["scope"], "unsigned-core")
        self.assertEqual(value["package_manifest"]["dependency_notices"]["crates"], 2)
        self.assertIn("locked-dependency-notices", value["checks_passed"])

    def test_candidate_rejects_skipped_native_stages_and_wayland_without_socket_trace(self):
        case, archive = self.platform_bundle("linux")
        lifecycle, wayland = case / "lifecycle-evidence", case / "wayland-evidence"
        verification = lifecycle / "verification.json"
        value = json.loads(verification.read_text())
        inputs = self.release.PACKAGER.source_inputs(self.root)
        manifest_path = case / "tree" / archive.name.removesuffix(".tar.gz") / "PACKAGE-MANIFEST.json"
        manifest = json.loads(manifest_path.read_text())
        package_manifest = {"sha256": digest(manifest_path), "build": manifest["build"],
                            "app_sha256": manifest["files"]["harbor-workspace"]}
        artifacts = {archive.name: digest(archive), self.debian_name: digest(case / "dist" / self.debian_name)}
        def verify():
            return self.release.verify_native_evidence(self.root, lifecycle, wayland, "linux",
                                                       self.source, self.version, artifacts, package_manifest, inputs)
        self.assertIn("wayland", verify())
        for changed, message in (
            ({**value, "stages": value["stages"][:-1]}, "required stages"),
            ({**value, "archive_sha256": "0" * 64}, "does not bind"),
            ({**value, "fixture_before": {}}, "fixture"),
            ({**value, "private_version_patch": {}}, "independently versioned"),
        ):
            write_json(verification, changed)
            with self.assertRaisesRegex(ValueError, message):
                verify()
        write_json(verification, value)
        (wayland / "startup.log").write_text("DISPLAY is gone and backend says Wayland")
        with self.assertRaisesRegex(ValueError, "socket connection missing"):
            verify()

    def test_candidate_rejects_unverified_policies_noncore_and_false_startup(self):
        for changes, message in [
            ({"profile": "unverified"}, "identity"),
            ({"profile": "qualification"}, "identity"),
            ({"wrong_file": True}, "files"),
            ({"wrong_helper": True}, "executable"),
            ({"optional": True}, "core"),
            ({"blank": True}, "blank"),
            ({"mismatched_startup": True}, "Startup"),
        ]:
            with self.subTest(changes=changes), self.assertRaisesRegex(ValueError, message):
                self.candidate("linux" if changes.get("wrong_helper") else "windows", **changes)

    def set_manifest_version(self, path, version):
        manifest = self.root / path
        text = manifest.read_text()
        previous = tomllib.loads(text)["package"]["version"]
        manifest.write_text(text.replace(f'version = "{previous}"', f'version = "{version}"', 1))
        self.lock(self.root, path)
        return previous

    def commit_fixture_versions(self, message):
        subprocess.run(["git", "-C", str(self.root), "-c", "user.name=Release Fixture",
                        "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false",
                        "commit", "-qam", message], check=True)
        return subprocess.check_output(["git", "-C", str(self.root), "rev-parse", "HEAD"], text=True).strip()

    def check_candidate_with_replacement_helper(self):
        replacement = next(version for version in ("9.2.1", "9.2.2", "9.2.3")
                           if version not in {self.version, self.rdp_version})
        previous = self.set_manifest_version("engines/rdp/Cargo.toml", replacement)
        self.read_fixture_versions()
        self.assertEqual(self.rdp_version, replacement)
        self.assertNotEqual(self.rdp_version, previous)
        self.source = self.commit_fixture_versions("Independent RDP version")
        self.release.verify_checkout(self.root, self.source, self.version)
        candidate, _ = self.candidate("windows")
        packages = json.loads((candidate / "candidate.json").read_text())["package_versions"]
        self.assertEqual(packages["harbor-workspace"], self.version)
        self.assertEqual(packages["termcraft-rdp-engine"], self.rdp_version)
        self.assertNotEqual(packages["harbor-workspace"], packages["termcraft-rdp-engine"])

    def test_candidate_accepts_independent_helper_version(self):
        self.root = self.work / "different-helper"
        shutil.copytree(self.fixture, self.root)
        self.check_candidate_with_replacement_helper()

    def test_candidate_accepts_existing_helper_at_preferred_replacement_version(self):
        self.root = self.work / "existing-preferred-helper"
        shutil.copytree(self.fixture, self.root)
        self.set_manifest_version("Cargo.toml", "1.0.0")
        self.set_manifest_version("engines/rdp/Cargo.toml", "9.2.1")
        # A fixture starting at the collision already has a clean, committed
        # checkout. Do not require an empty setup commit in that case either.
        if subprocess.check_output(["git", "-C", str(self.root), "status", "--porcelain"]):
            self.source = self.commit_fixture_versions("Set up version collision")
        self.read_fixture_versions()
        self.assertEqual((self.version, self.rdp_version), ("1.0.0", "9.2.1"))
        self.check_candidate_with_replacement_helper()

    def test_candidate_rejects_consistently_forged_dependency_notices(self):
        for forgery in ("omission", "path", "bytes"):
            with self.subTest(forgery=forgery), self.assertRaisesRegex(ValueError, "dependency notice"):
                self.candidate("windows", notice_forgery=forgery)

    def test_transparent_png_does_not_satisfy_startup(self):
        path = self.work / "startup.png"
        path.write_bytes(png(transparent=True))
        with self.assertRaisesRegex(ValueError, "blank"):
            self.release.png_observation(path)

    def test_candidate_rejects_a_different_debian_payload(self):
        case, _ = self.platform_bundle("linux")
        deb = next((case / "dist").glob("*.deb"))
        deb.write_bytes(deb.read_bytes() + b"tampered")
        with self.assertRaisesRegex(ValueError, "Debian"):
            self.release.stage_candidate(self.root, self.source, self.version, "linux", case / "dist",
                                         case / "startup-evidence", case / "candidate")
        self.assertFalse((case / "candidate").exists())

    def test_aggregate_revalidates_all_platforms_and_emits_exact_tracked_source(self):
        candidates = [self.candidate(p)[0] for p in ("linux", "windows", "macos")]
        out = self.work / "assembled"
        value = self.release.assemble(self.root, self.source, self.version, candidates, out)
        self.assertFalse(value["publication_ready"])
        self.assertEqual(set(value["platforms"]), {"linux", "macos", "windows"})
        source_file = out / value["source"]["archive"]
        self.assertEqual(digest(source_file), value["source"]["sha256"])
        import gzip
        with gzip.open(source_file, "rb") as decompressed:
            commit = subprocess.check_output(["git", "get-tar-commit-id"], input=decompressed.read(1024))
        self.assertEqual(commit.strip().decode(), self.source)
        listed = (out / "SHA256SUMS").read_text().splitlines()
        names = [line.split("  ", 1)[1] for line in listed]
        self.assertEqual(names, sorted(set(names)))
        for line in listed:
            expected, name = line.split("  ", 1)
            self.assertEqual(digest(out / name), expected)

    def test_native_gates_and_regression_make_local_assembly_publication_eligible(self):
        candidates = [self.candidate(platform)[0] for platform in ("linux", "windows", "macos")]
        regression = {"workflow": ".github/workflows/ci.yml", "result": "success", "run_id": "123", "run_attempt": "1"}
        out = self.work / "assembled"
        value = self.release.assemble(self.root, self.source, self.version, candidates, out, regression=regression)
        self.assertTrue(value["publication_ready"])
        self.assertEqual(value["pending_gates"], [])
        self.assertTrue(set(self.release.PENDING_GATES).issubset(value["checks_passed"]))
        self.assertEqual(len(self.release.publication_files(out, self.source, self.version)), 7)
        (candidates[0] / "wayland-evidence/verification.json").unlink()
        withheld = self.work / "withheld"
        with self.assertRaisesRegex(ValueError, "Startup evidence is incomplete"):
            self.release.assemble(self.root, self.source, self.version, candidates, withheld, regression=regression)
        self.assertFalse(withheld.exists())

    def test_aggregate_rejects_forged_notices_even_with_updated_download_hashes(self):
        candidates = [self.candidate(p)[0] for p in ("linux", "windows", "macos")]
        windows = candidates[1]
        forged, archive = self.platform_bundle("windows", notice_forgery="path")
        shutil.copyfile(archive, windows / "dist" / archive.name)
        shutil.copytree(forged / "startup-evidence", windows / "startup-evidence", dirs_exist_ok=True)
        inventory = windows / "candidate.json"
        value = json.loads(inventory.read_text())
        value["artifacts"][archive.name] = digest(archive)
        value["startup"]["observation"] = json.loads((forged / "startup-evidence/verification.json").read_text())
        value["startup"]["files"] = {file.name: digest(file) for file in (forged / "startup-evidence").iterdir()}
        with zipfile.ZipFile(archive) as bundle:
            manifest = next(name for name in bundle.namelist() if name.endswith("/PACKAGE-MANIFEST.json"))
            value["package_manifest"]["sha256"] = hashlib.sha256(bundle.read(manifest)).hexdigest()
        write_json(inventory, value)
        with self.assertRaisesRegex(ValueError, "locked dependency notice"):
            self.release.assemble(self.root, self.source, self.version, candidates, self.work / "assembled")
        self.assertFalse((self.work / "assembled").exists())

    def test_aggregate_refuses_missing_duplicate_or_tampered_platform_inventory(self):
        candidates = [self.candidate(p)[0] for p in ("linux", "windows", "macos")]
        out = self.work / "assembled"
        for selected in (candidates[:2], [candidates[0], candidates[0], candidates[2]]):
            with self.assertRaisesRegex(ValueError, "exactly one"):
                self.release.assemble(self.root, self.source, self.version, selected, out)
        inventory = candidates[2] / "candidate.json"
        saved = json.loads(inventory.read_text())
        for change in (
            {"version": self.version + ".1"}, {"source_sha": "0" * 40},
            {"artifacts": {"../escape": "0" * 64}}, {"artifacts": {self.debian_name: "0" * 64}},
        ):
            with self.subTest(change=change):
                write_json(inventory, {**saved, **change})
                with self.assertRaises(ValueError):
                    self.release.assemble(self.root, self.source, self.version, candidates, out)
        write_json(inventory, saved)
        with next((candidates[2] / "dist").glob("*.zip")).open("ab") as file:
            file.write(b"extra bytes")
        with self.assertRaisesRegex(ValueError, "inventory"):
            self.release.assemble(self.root, self.source, self.version, candidates, out)
        self.assertFalse(out.exists())


class MockGitHub:
    def __init__(self, source, release, assets):
        self.source, self.existing, self.assets = source, release, assets
        self.calls = []

    def tag_commit(self, tag):
        self.calls.append(("tag", tag))
        return self.source

    def release_for_tag(self, tag):
        self.calls.append(("release", tag))
        return self.existing

    def release_assets(self, identifier):
        self.calls.append(("assets", identifier))
        return self.assets

    def create_draft(self, tag, source, paths):
        self.calls.append(("create_draft", tag, source))
        self.existing = {"id": 51, "draft": True}
        self.assets = [{"name": path.name, "digest": "sha256:" + digest(path), "state": "uploaded"} for path in paths]

    def publish_draft(self, identifier):
        self.calls.append(("publish_draft", identifier))
        self.existing["draft"] = False


class ReleasePublication(unittest.TestCase):
    def setUp(self):
        self.assertTrue(SCRIPT.is_file(), "Functional release delivery helper is not implemented")
        self.release = load(SCRIPT)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.asset = Path(self.temp.name) / "example.zip"
        self.asset.write_bytes(b"public-release")
        self.source = "1" * 40
        self.ref = "refs/tags/v1.2.3"

    def test_only_a_qualified_validated_tag_consults_github(self):
        github = MockGitHub(self.source, None, None)
        for event, ref, sha, ready in (
            ("workflow_dispatch", self.ref, self.source, True),
            ("push", self.ref, self.source, False),
            ("push", "refs/tags/--bad", self.source, True),
            ("push", self.ref, "HEAD", True),
        ):
            with self.subTest(event=event, ref=ref, sha=sha, ready=ready), self.assertRaises(ValueError):
                self.release.publish(github, event, ref, "1.2.3", sha, ready, [self.asset])
            self.assertEqual(github.calls, [])

    def test_existing_exact_release_is_read_only_and_conflicts_are_never_clobbered(self):
        existing = {"id": 21, "draft": False}
        assets = [{"name": self.asset.name, "digest": "sha256:" + digest(self.asset), "state": "uploaded"}]
        github = MockGitHub(self.source, existing, assets)
        self.release.publish(github, "push", self.ref, "1.2.3", self.source, True, [self.asset])
        self.assertEqual([call[0] for call in github.calls], ["tag", "release", "assets"])
        for source, conflicting in (
            ("2" * 40, assets), (self.source, [{**assets[0], "digest": "sha256:" + "0" * 64}]),
            (self.source, []), (self.source, assets * 2),
        ):
            with self.subTest(conflicting=conflicting):
                github = MockGitHub(source, existing, conflicting)
                with self.assertRaises(ValueError):
                    self.release.publish(github, "push", self.ref, "1.2.3", self.source, True, [self.asset])
                self.assertTrue(all(call[0] in {"tag", "release", "assets"} for call in github.calls))

    def test_first_publication_reverifies_the_uploaded_draft_before_publishing(self):
        github = MockGitHub(self.source, None, None)
        self.release.publish(github, "push", self.ref, "1.2.3", self.source, True, [self.asset])
        self.assertEqual([call[0] for call in github.calls],
                         ["tag", "release", "create_draft", "release", "assets", "tag", "publish_draft"])
        self.assertFalse(github.existing["draft"])


if __name__ == "__main__":
    unittest.main()
