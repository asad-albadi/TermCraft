import hashlib
import importlib.util
import json
from pathlib import Path
import stat
import struct
import subprocess
import sys
import tarfile
import tempfile
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
    def setUp(self):
        self.assertTrue(SCRIPT.is_file(), "Functional release delivery helper is not implemented")
        self.release = load(SCRIPT)
        self.debian = load(ROOT / "scripts/package-debian.py")
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.work = Path(self.tmp.name)
        self.source = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
        self.version = "0.1.0"

    def platform_bundle(self, platform, *, profile="release", blank=False, optional=False,
                        wrong_file=False, wrong_helper=False, mismatched_startup=False):
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
        for src in (ROOT / "vendor/noVNC").rglob("*"):
            if src.is_file():
                originals["build-source/noVNC/" + src.relative_to(ROOT / "vendor/noVNC").as_posix()] = src.relative_to(ROOT).as_posix()
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
                **plistlib.loads((ROOT / "packaging/macos/Info.plist").read_bytes()),
                "CFBundleVersion": self.version, "CFBundleShortVersionString": self.version,
            }))
            stage_file(resources / "TermCraft.icns", b"icon")
            programs.append("termcraft-desktop-launcher")
        for relative, source in originals.items():
            stage_file(resources / relative, (ROOT / source).read_bytes())
        for name in programs:
            machine = "linux-arm64" if wrong_helper and name.startswith("termcraft-rdp") else platform
            stage_file(binaries / name, executable(machine, name))
            (binaries / name).chmod(0o755)
        notice = "licenses/crates/example-1.0.0/LICENSE"
        stage_file(resources / notice, b"Example dependency license")
        if optional:
            stage_file(resources / "browser/unapproved/Chromium.exe", b"unapproved")
        runtime = json.loads((ROOT / "packaging/runtime-requirements.json").read_text())
        for key in ("platform_requirements", "optional_native_providers"):
            runtime[key] = runtime[key][platform]
        if platform != "linux":
            del runtime["linux_display_libraries"]
        vnc = json.loads((ROOT / "vendor/noVNC/UPSTREAM.json").read_text())
        runtime.update(
            linked_libraries={name: {"libraries": ["libc.so.6"], "highest_required_glibc_symbol_version": "2.34"}
                              for name in programs},
            artifact_declarations="RUNTIME-MANIFEST.json",
            managed_vnc_browser={
                "release": vnc["release"], "source_url": vnc["source"], "source_archive_sha256": vnc["sha256"],
                "embedded_runtime": True, "source_and_notices": "build-source/noVNC",
            },
        )
        build_inputs = load(ROOT / "scripts/package-release.py").source_inputs(ROOT)
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
            rdp_helper_version=self.version, platform=platform, architecture=arch, signing="unsigned",
            runtime=runtime, files=files, build=build,
            licenses={
                "project": (resources / "LICENSE").relative_to(root).as_posix(),
                "credits": (resources / "CREDITS.md").relative_to(root).as_posix(),
                "crate_notice_paths_relative_to": resources.relative_to(root).as_posix(),
                "crates": [{"name": "example", "version": "1.0.0", "notice_paths": [notice]}],
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
            "windows_software_gl_fixture": json.loads((ROOT / "tests/windows-software-gl.json").read_text())["files"] if platform == "windows" else None,
        })
        return case, archive

    def candidate(self, platform, **kwargs):
        case, archive = self.platform_bundle(platform, **kwargs)
        candidate = case / ("candidate-" + {"linux": "linux-amd64", "windows": "windows-amd64", "macos": "macos-arm64"}[platform])
        self.release.stage_candidate(ROOT, self.source, self.version, platform, case / "dist", case / "startup-evidence", candidate)
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
        self.assertEqual(value["artifacts"]["termcraft_0.1.0_amd64.deb"],
                         digest(destination / "dist/termcraft_0.1.0_amd64.deb"))
        self.assertEqual(value["startup"]["observation"]["archive_sha256"], digest(archive))
        self.assertEqual(value["startup"]["files"]["startup.png"], digest(destination / "startup-evidence/startup.png"))
        self.assertEqual(value["source_sha"], self.source)
        self.assertEqual(value["scope"], "unsigned-core")

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
            self.release.stage_candidate(ROOT, self.source, self.version, "linux", case / "dist",
                                         case / "startup-evidence", case / "candidate")
        self.assertFalse((case / "candidate").exists())

    def test_aggregate_revalidates_all_platforms_and_emits_exact_tracked_source(self):
        candidates = [self.candidate(p)[0] for p in ("linux", "windows", "macos")]
        out = self.work / "assembled"
        value = self.release.assemble(ROOT, self.source, self.version, candidates, out)
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

    def test_aggregate_refuses_missing_duplicate_or_tampered_platform_inventory(self):
        candidates = [self.candidate(p)[0] for p in ("linux", "windows", "macos")]
        out = self.work / "assembled"
        for selected in (candidates[:2], [candidates[0], candidates[0], candidates[2]]):
            with self.assertRaisesRegex(ValueError, "exactly one"):
                self.release.assemble(ROOT, self.source, self.version, selected, out)
        inventory = candidates[2] / "candidate.json"
        saved = json.loads(inventory.read_text())
        for change in (
            {"version": "0.2.0"}, {"source_sha": "0" * 40},
            {"artifacts": {"../escape": "0" * 64}}, {"artifacts": {"termcraft_0.1.0_amd64.deb": "0" * 64}},
        ):
            with self.subTest(change=change):
                write_json(inventory, {**saved, **change})
                with self.assertRaises(ValueError):
                    self.release.assemble(ROOT, self.source, self.version, candidates, out)
        write_json(inventory, saved)
        with next((candidates[2] / "dist").glob("*.zip")).open("ab") as file:
            file.write(b"extra bytes")
        with self.assertRaisesRegex(ValueError, "inventory"):
            self.release.assemble(ROOT, self.source, self.version, candidates, out)
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
