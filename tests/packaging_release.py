import importlib.util
import json
import os
import plistlib
from pathlib import Path
import shutil
import struct
import subprocess
import sys
import tarfile
import tempfile
import time
import unittest
import zipfile
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
SCRIPT = ROOT / "scripts/package-release.py"


def module_at(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


@unittest.skipUnless(sys.platform.startswith("linux") and shutil.which("cc"), "Linux and cc required")
class PortablePackage(unittest.TestCase):
    def test_archive_relocates_with_empty_path_and_keeps_workspace_data(self):
        package = module_at("package_release", SCRIPT)
        smoke = module_at("package_check_startup", ROOT / "scripts/package-check-startup.py")
        with tempfile.TemporaryDirectory(prefix="package path ") as temporary:
            base = Path(temporary)
            executable = base / "harbor-workspace"
            helper = base / "termcraft-rdp-engine"
            subprocess.run(["cc", "-xc", "-o", str(helper), "-"], input="int main(){return 0;}", text=True, check=True)
            # Run the helper beside argv[0] with an empty PATH and a different cwd.
            source = r"""
                #include <string.h>
                #include <stdio.h>
                #include <stdlib.h>
                #include <unistd.h>
                int main(int argc, char **argv) {
                    const char *marker = getenv("MENU_LAUNCHED");
                    if (marker) {
                        FILE *output = fopen(marker, "w");
                        if (!output) return 4;
                        fclose(output);
                    } else if (argc != 2 || strcmp(argv[1], "one argument")) {
                        return 3;
                    }
                    char path[4096];
                    if (strlen(argv[0]) + 21 >= sizeof(path)) return 5;
                    strcpy(path, argv[0]);
                    strcpy(strrchr(path, '/') + 1, "termcraft-rdp-engine");
                    return execl(path, path, (char *)0);
                }
            """
            subprocess.run(["cc", "-xc", "-o", str(executable), "-"], input=source, text=True, check=True)
            output = base / "output"
            with patch.object(package, "cargo_notices", return_value=[]):
                archive = package.package_release("linux", output, executable, helper)
                checksum = package.sha256(archive)
                with self.assertRaises(FileExistsError):
                    package.package_release("linux", output, executable, helper)
                self.assertEqual(package.sha256(archive), checksum)
            executable.unlink()
            helper.unlink()
            unpack = base / 'relocated "%$`\\'
            smoke.extract(archive, unpack)
            app = next(unpack.iterdir())
            config = base / "config" / "harbor"
            config.mkdir(parents=True)
            settings = config / "workspace.json"
            settings.write_bytes(b"existing settings")
            env = {"PATH": "", "HOME": str(base), "XDG_CONFIG_HOME": str(config.parent),
                   "XDG_DATA_HOME": str(base / "menu % data")}
            subprocess.run([app / "harbor-workspace", "one argument"], cwd=base, env=env, check=True)
            menu = app / "desktop-integration.py"
            command = [sys.executable, menu]
            subprocess.run([*command, "install"], env=env, check=True)
            desktop = base / "menu % data/applications/org.termcraft.TermCraft.desktop"
            contents = desktop.read_text()
            self.assertIn("%%", contents)
            self.assertIn("Name=TermCraft", contents)
            gio = shutil.which("gio")
            if gio:
                marker = base / "launched from menu"
                subprocess.run([gio, "launch", desktop], env={**env, "MENU_LAUNCHED": str(marker)}, check=True)
                for _ in range(40):
                    if marker.exists():
                        break
                    time.sleep(0.05)
                self.assertTrue(marker.exists())
            desktop.write_text("foreign desktop")
            self.assertNotEqual(subprocess.run([*command, "uninstall"], env=env).returncode, 0)
            self.assertNotEqual(subprocess.run([*command, "install"], env=env).returncode, 0)
            self.assertEqual(desktop.read_text(), "foreign desktop")
            desktop.write_text(contents)
            subprocess.run([*command, "uninstall"], env=env, check=True)
            self.assertFalse(desktop.exists())
            next_version = base / "upgraded installation"
            shutil.copytree(app, next_version)
            next_menu = [sys.executable, next_version / menu.name]
            subprocess.run([*next_menu, "install"], env=env, check=True)
            self.assertIn("termcraft/desktop/launch.py", desktop.read_text())
            self.assertTrue((base / "menu % data/termcraft/desktop/payload/harbor-workspace").is_file())
            subprocess.run([*next_menu, "uninstall"], env=env, check=True)
            shutil.rmtree(next_version)
            self.assertEqual(settings.read_bytes(), b"existing settings")
            manifest = json.loads((app / "PACKAGE-MANIFEST.json").read_text())
            self.assertEqual(manifest["build"]["provenance"], "prebuilt inputs")
            self.assertIn("engines/rdp/Cargo.lock", manifest["build"]["source_inputs"])
            self.assertTrue(manifest["runtime"]["linked_libraries"]["harbor-workspace"])
            files = [p.relative_to(app).as_posix() for p in app.rglob("*") if p.is_file()]
            self.assertEqual(set(files), set(manifest["files"]) | {"PACKAGE-MANIFEST.json"})
            for name, digest in manifest["files"].items():
                self.assertEqual(package.sha256(app / name), digest)
            self.assertIn("ssh", manifest["runtime"]["optional_installed_clients"])
            self.assertNotIn(str(ROOT), json.dumps(manifest))
            logo = app / "assets/termcraft-logo.png"
            self.assertTrue(logo.is_file())
            self.assertEqual(smoke.verify(app, app / "PACKAGE-MANIFEST.json")[0], "linux")
            logo.write_bytes(b"tampered")
            with self.assertRaisesRegex(ValueError, "differ from the manifest"):
                smoke.verify(app, app / "PACKAGE-MANIFEST.json")

    def test_xwayland_archive_rejects_missing_wrong_hash_and_wrong_target(self):
        package = module_at("package_release", SCRIPT)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "harbor-workspace"
            helper = root / "termcraft-rdp-engine"
            for binary in (app, helper):
                subprocess.run(["cc", "-xc", "-o", str(binary), "-"],
                               input="int main(){return 0;}", text=True, check=True)
            output = root / "out"
            payload = root / "xwayland.pkg.tar.zst"
            with self.assertRaisesRegex(ValueError, "regular pinned"):
                package.package_release("linux", output, app, helper, xwayland_archive=payload)
            payload.write_bytes(b"wrong package")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                package.package_release("linux", output, app, helper, xwayland_archive=payload)
            with patch.object(package, "binary_arch", return_value="x86_64"):
                with self.assertRaisesRegex(ValueError, "targets the Linux x86_64 Arch runtime ABI"):
                    package.package_release("windows", output, app, helper, xwayland_archive=payload)
            self.assertEqual(list(output.glob("*.tar.gz")), [])

    @unittest.skipUnless(os.environ.get("TERMCRAFT_TEST_XWAYLAND_ARCHIVE"),
                         "requires the pinned Arch archive and its runtime ABI")
    def test_xwayland_archive_copies_verified_executable_and_notices(self):
        package = module_at("package_release", SCRIPT)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "harbor-workspace"
            helper = root / "termcraft-rdp-engine"
            for binary in (app, helper):
                subprocess.run(["cc", "-xc", "-o", str(binary), "-"],
                               input="int main(){return 0;}", text=True, check=True)
            payload = Path(os.environ["TERMCRAFT_TEST_XWAYLAND_ARCHIVE"])
            with patch.object(package, "cargo_notices", return_value=[]):
                artifact = package.package_release("linux", root / "out", app, helper,
                                                   xwayland_archive=payload)
            self.assertTrue(artifact.name.endswith("-arch-xwayland.tar.gz"))
            module_at("portable_archive", ROOT / "scripts/portable_archive.py").extract_tar(artifact, root / "unpacked")
            package_root = next((root / "unpacked").iterdir())
            manifest = json.loads((package_root / "PACKAGE-MANIFEST.json").read_text())
            engine = manifest["runtime"]["managed_xwayland"]
            executable = engine["entry_point"]
            self.assertEqual(package.sha256(package_root / executable), engine["executable_sha256"])
            self.assertEqual(manifest["files"][executable], engine["executable_sha256"])
            self.assertTrue(os.access(package_root / executable, os.X_OK))
            for notice in ("license", "package_inventory", "build_inventory"):
                relative = engine[notice]
                self.assertEqual(manifest["files"][relative], package.sha256(package_root / relative))
            self.assertIn(executable, manifest["runtime"]["linked_libraries"])

    def test_source_tree_loader_and_runpath_are_rejected(self):
        package = module_at("package_release", SCRIPT)
        with tempfile.TemporaryDirectory(prefix="package build ") as temporary:
            base = Path(temporary)
            binary = base / "app"
            for flag in (f"-Wl,--dynamic-linker={base}/host-loader.so", f"-Wl,-rpath,{base}"):
                subprocess.run(["cc", "-xc", flag, "-o", str(binary), "-"],
                               input="int main(){return 0;}", text=True, check=True)
                with self.assertRaisesRegex(ValueError, "Unexpected ELF"):
                    package.native_libraries(binary, "linux")

    def test_architecture_mismatch_prevents_staging(self):
        package = module_at("package_release", SCRIPT)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            app = root / "harbor-workspace"
            helper = root / "termcraft-rdp-engine"
            subprocess.run(["cc", "-xc", "-o", str(app), "-"], input="int main(){return 0;}", text=True, check=True)
            elf = bytearray(app.read_bytes())
            # ELF e_machine sits at byte 18. Change the helper to the other supported ISA.
            self.assertEqual(elf[5], 1)
            elf[18:20] = (183 if elf[18] == 62 else 62).to_bytes(2, "little")
            helper.write_bytes(elf)
            dest = root / "out"
            with self.assertRaisesRegex(ValueError, "architectures differ"):
                package.package_release("linux", dest, app, helper)
            self.assertFalse(dest.exists())


class SourceSnapshot(unittest.TestCase):
    def test_release_build_reads_snapshot_while_live_sources_change(self):
        package = module_at("package_release", SCRIPT)
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            initial, _ = package.freeze_sources(ROOT, base / "initial")
            live = base / "live"
            shutil.copytree(initial, live)
            (live / "data").mkdir()
            (live / "data/workspace.json").write_text("private user workspace")
            build = base / "target"
            frozen, inputs = package.freeze_sources(live, build / "package-sources")
            self.assertFalse((frozen / "data").exists())
            self.assertEqual(frozen.name, package.source_digest(inputs))
            self.assertEqual(package.freeze_sources(live, build / "package-sources"), (frozen, inputs))

            def cargo(command, *, cwd, **kwargs):
                self.assertEqual(cwd, Path(frozen.anchor))
                self.assertTrue(command[command.index("--manifest-path") + 1].is_relative_to(frozen))
                (live / "src/main.rs").write_text("source edited during the build")
                self.assertEqual(package.source_inputs(frozen), inputs)

            with patch.object(package.subprocess, "run", side_effect=cargo) as calls:
                package.release_build(frozen, build)
            self.assertEqual(calls.call_count, 2)
            updated, _ = package.freeze_sources(live, build / "package-sources")
            self.assertNotEqual(updated, frozen)
            self.assertEqual(package.source_inputs(frozen), inputs)


class PublicationBoundary(unittest.TestCase):
    def test_asset_allowlist_excludes_private_files_and_rejects_symlinks(self):
        package = module_at("package_release", SCRIPT)
        with tempfile.TemporaryDirectory() as temporary:
            base = Path(temporary)
            live, _ = package.freeze_sources(ROOT, base / "initial")
            draft = live / "assets/private-draft.png"
            draft.write_bytes(b"private")
            frozen, inputs = package.freeze_sources(live, base / "frozen")
            self.assertNotIn("assets/private-draft.png", inputs)
            self.assertFalse((frozen / draft.relative_to(live)).exists())
            package.copy_release_assets(live, base / "stage")
            self.assertFalse((base / "stage/private-draft.png").exists())
            asset = live / "assets/termcraft-logo.png"
            if sys.platform != "win32":
                asset.unlink()
                asset.symlink_to(draft)
                with self.assertRaisesRegex(ValueError, "symlink"):
                    package.freeze_sources(live, base / "rejected")
                with self.assertRaisesRegex(ValueError, "symlink"):
                    package.copy_release_assets(live, base / "rejected-stage")

    @unittest.skipUnless(shutil.which("cargo"), "Cargo unavailable")
    def test_cargo_ignores_live_ancestor_configuration(self):
        package = module_at("package_release", SCRIPT)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            snapshot = root / "nested" / "snapshot"
            source = snapshot / "src"
            source.mkdir(parents=True)
            (source / "main.rs").write_text("fn main() {}")
            manifest = snapshot / "Cargo.toml"
            manifest.write_text('[package]\nname="release-isolation-fixture"\nversion="0.1.0"\nedition="2021"\n')
            settings = package.cargo_invocation(root / "isolated")
            config = root / ".cargo/config.toml"
            config.parent.mkdir()
            config.write_text('[build]\nrustc="termcraft-nonexistent-rustc"\n')
            command = ["cargo", "check", "--offline", "--manifest-path", manifest]
            normal = subprocess.run(command, cwd=snapshot, capture_output=True)
            self.assertNotEqual(normal.returncode, 0)
            subprocess.run(command, **settings, check=True, capture_output=True)
            self.assertEqual(settings, package.cargo_invocation(root / "isolated"))
            cargo_home = Path(settings["env"]["CARGO_HOME"])
            (cargo_home / "config.toml").write_text(config.read_text())
            with self.assertRaisesRegex(ValueError, "isolated package Cargo home"):
                package.cargo_invocation(root / "isolated")


class NativePackageLayout(unittest.TestCase):
    def test_macos_xquartz_staging_rejects_changed_source_bytes(self):
        package = module_at("package_release", SCRIPT)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            candidate = root / "candidate"
            inventory = ROOT / "packaging/x11/native/macos-arm64.sha256"
            first = inventory.read_text().splitlines()[0].split("  ", 1)[1]
            changed = candidate / first
            changed.parent.mkdir(parents=True)
            changed.write_bytes(b"changed source archive")
            with self.assertRaisesRegex(ValueError, "XQuartz checksum mismatch"):
                package.stage_xquartz(ROOT, root / "resources", candidate)

    def test_macos_build_metadata_records_sdk_and_deployment_target(self):
        package = module_at("package_release", SCRIPT)
        with patch.object(package, "capture", side_effect=["15.5\n", "Apple clang version 17.0.0\n"]), \
             patch.dict(os.environ, {"MACOSX_DEPLOYMENT_TARGET": "14.0"}, clear=False):
            self.assertEqual(
                package.macos_build_metadata("aarch64"),
                {
                    "sdk_version": "15.5",
                    "deployment_target": "14.0",
                    "compiler": "Apple clang version 17.0.0",
                    "architecture": "aarch64",
                },
            )

    def test_windows_zip_and_macos_bundle_keep_helper_beside_app(self):
        try:
            __import__("PIL")
        except ImportError:
            self.skipTest("Pillow required for native icon generation")
        package = module_at("package_release", SCRIPT)
        pe = bytearray(128)
        pe[:2] = b"MZ"
        struct.pack_into("<I", pe, 60, 64)
        struct.pack_into("<4sH", pe, 64, b"PE\0\0", 0x8664)
        macho = struct.pack("<IIII", 0xFEEDFACF, 0x0100000C, 0, 2)
        for target, executable_header in (("windows", pe), ("macos", macho)):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as temporary:
                base = Path(temporary)
                app, helper = (base / name for name in ("app", "helper"))
                app.write_bytes(executable_header + b"app")
                helper.write_bytes(executable_header + b"helper")
                def fixture_launcher(packaging, path, arch):
                    # Layout-only fixture: this does not execute native AppKit events.
                    path.write_bytes(macho + b"fixture launcher")
                    path.chmod(0o755)
                with (patch.object(package, "build_macos_launcher", side_effect=fixture_launcher),
                      patch.object(package, "cargo_notices", return_value=[]),
                      patch.object(package, "native_libraries", return_value={"libraries": []})):
                    archive = package.package_release(target, base / "out", app, helper)
                with zipfile.ZipFile(archive) as bundle:
                    self.assertIsNone(bundle.testzip())
                    names = bundle.namelist()
                    suffix = ".exe" if target == "windows" else ""
                    entry = next(item for item in names if item.endswith("/harbor-workspace" + suffix))
                    engine = entry.replace("harbor-workspace", "termcraft-rdp-engine")
                    self.assertEqual(bundle.read(entry), app.read_bytes())
                    self.assertEqual(bundle.read(engine), helper.read_bytes())
                    if os.name == "posix":
                        for name in (entry, engine):
                            self.assertEqual((bundle.getinfo(name).external_attr >> 16) & 0o777, 0o755)
                    if target == "macos":
                        plist = plistlib.loads(bundle.read("TermCraft.app/Contents/Info.plist"))
                        self.assertEqual(plist["CFBundleExecutable"], "termcraft-desktop-launcher")
                        self.assertIn("TermCraft.app/Contents/MacOS/termcraft-desktop-launcher", names)
                        self.assertEqual(plist["CFBundleURLTypes"][0]["CFBundleURLSchemes"], ["termcraft"])
                        icon = bundle.read("TermCraft.app/Contents/Resources/TermCraft.icns")
                        self.assertEqual(icon[:4], b"icns")
                    else:
                        icon = bundle.read(next(item for item in names if item.endswith("/TermCraft.ico")))
                        self.assertEqual(icon[:6], b"\0\0\1\0\1\0")
                        install_name = next(item for item in names if item.endswith("/install-per-user.ps1"))
                        manifest_name = next(item for item in names if item.endswith("/PACKAGE-MANIFEST.json"))
                        manifest = json.loads(bundle.read(manifest_name))
                        self.assertEqual(
                            manifest["files"]["install-per-user.ps1"],
                            package.hashlib.sha256(bundle.read(install_name)).hexdigest(),
                        )


if __name__ == "__main__":
    unittest.main()
