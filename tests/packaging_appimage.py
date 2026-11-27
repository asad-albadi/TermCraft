import importlib.util
from pathlib import Path
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[1]
SPEC = importlib.util.spec_from_file_location("appimage", ROOT / "scripts/package-appimage.py")
appimage = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(appimage)


class AppImageTests(unittest.TestCase):
    def test_prefix_is_bound_to_bytes_length_and_squashfs(self):
        with tempfile.TemporaryDirectory() as temp:
            image = Path(temp) / "sample.AppImage"
            prefix = bytearray(64)
            prefix[:7] = b"\x7fELF\x02\x01\x01"
            prefix[8:11] = b"AI\x02"
            prefix[18:20] = b"\x3e\x00"
            image.write_bytes(prefix)
            expected = appimage.digest(image)
            image.write_bytes(prefix + b"hsqs" + bytes(96))
            appimage.verify_prefix(image, expected, len(prefix))
            for digest, size in [(expected, 60), ("f" * 64, len(prefix))]:
                with self.assertRaises(ValueError):
                    appimage.verify_prefix(image, digest, size)
            image.write_bytes(prefix + b"sqsh")
            with self.assertRaises(ValueError):
                appimage.verify_prefix(image, expected, len(prefix))

    def test_apprun_preserves_cwd_literal_arguments_and_private_search_path(self):
        import json
        import os
        import shutil
        import subprocess
        import sys
        with tempfile.TemporaryDirectory(prefix="App Image test ") as temp:
            appdir = Path(temp) / "package"
            binary = appdir / appimage.PORTABLE / "harbor-workspace"
            binary.parent.mkdir(parents=True)
            executable = appdir / "AppRun"
            shutil.copy2(appimage.RECIPE / "AppRun", executable)
            binary.write_text(f"#!{sys.executable}\nimport os,json,sys\n"
                              "print(json.dumps([os.getcwd(), sys.argv[1:], os.environ['LD_LIBRARY_PATH']]))\n")
            binary.chmod(0o755)
            args = ["", "space ' and %", "$(literal);*", "--data-dir"]
            result = subprocess.check_output([executable, *args], cwd=temp,
                                             env={"PATH": "", "LD_LIBRARY_PATH": "/unexpected"}, text=True)
            self.assertEqual(json.loads(result), [os.path.realpath(temp), args, str(appdir / appimage.LIBRARY)])

    def test_source_rejects_unknown_missing_changed_and_link_members(self):
        import io
        import tarfile
        with tempfile.TemporaryDirectory() as temp:
            source = Path(temp) / "source.tar.gz"
            for name, kind, content in [
                    ("../outside", tarfile.REGTYPE, b""),
                    ("runtime/notices/musl.txt", tarfile.SYMTYPE, b""),
                    ("runtime/notices/musl.txt", tarfile.REGTYPE, b"changed"),
                    ("runtime/notices/musl.txt", tarfile.REGTYPE, None)]:
                with self.subTest(name=name, kind=kind, content=content):
                    with tarfile.open(source, "w:gz") as tar:
                        member = tarfile.TarInfo(name)
                        member.type = kind
                        if content is not None:
                            member.size = len(content)
                            member.linkname = "../outside" if kind == tarfile.SYMTYPE else ""
                            tar.addfile(member, io.BytesIO(content))
                    with self.assertRaises(ValueError):
                        appimage.verify_source(source)

    def test_changed_tool_and_library_input_fail_before_execution(self):
        import json
        import stat
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            inputs = work / "inputs"
            inputs.mkdir()
            (inputs / "appimagetool").write_bytes(b"incorrect tool")
            with self.assertRaisesRegex(ValueError, "Incorrect appimagetool"):
                appimage.stage_tool(inputs, work)
            self.assertFalse(stat.S_IMODE((work / "appimagetool").stat().st_mode) & 0o111)
            lock = json.loads((appimage.RECIPE / "ubuntu-lock.json").read_text())
            first = next(iter(lock["packages"].values()))
            (inputs / Path(first["path"]).name).write_bytes(b"incorrect Ubuntu package")
            with self.assertRaisesRegex(ValueError, "Incorrect pinned package"):
                appimage.MATERIALS.libraries(inputs, work / "library-stage")

    def test_existing_output_is_never_overwritten(self):
        with tempfile.TemporaryDirectory() as temp:
            out = Path(temp)
            core = out / "TermCraft-0.1.0-linux-x86_64.tar.gz"
            for name in appimage.names("0.1.0"):
                sentinel = out / name
                sentinel.write_bytes(b"keep")
                with self.assertRaises(FileExistsError):
                    appimage.package(core, out, out, out)
                self.assertEqual(sentinel.read_bytes(), b"keep")
                sentinel.unlink()


class ProducedImageTests(unittest.TestCase):
    """Release CI provides the real produced AppImage and independent core."""
    @classmethod
    def setUpClass(cls):
        import os
        if "TERMCRAFT_APPIMAGE_TEST_IMAGE" not in os.environ:
            raise unittest.SkipTest("Requires built AppImage and matching-source sidecar")
        cls.image = Path(os.environ["TERMCRAFT_APPIMAGE_TEST_IMAGE"]).resolve(strict=True)
        cls.core = Path(os.environ["TERMCRAFT_APPIMAGE_TEST_CORE"]).resolve(strict=True)
        cls.source = cls.image.with_name(cls.image.name.removesuffix(".AppImage") + "-AppImage-source.tar.gz")

    def test_real_outer_envelope_matches_complete_independent_core(self):
        with tempfile.TemporaryDirectory() as tmp:
            info, appdir = appimage.verify_image(self.core, self.image, self.source, Path(tmp))
            self.assertEqual(info["core"]["sha256"], appimage.digest(self.core))
            self.assertEqual(info["corresponding_source"]["sha256"], appimage.digest(self.source))
            self.assertEqual((appdir / ".DirIcon").readlink(), Path("termcraft.png"))
            self.assertTrue((appdir / appimage.PORTABLE / "termcraft-rdp-engine").is_file())

    def test_real_outer_rejects_changed_payload_or_unsafe_link(self):
        import subprocess
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            info, appdir = appimage.verify_image(self.core, self.image, self.source, work / "clean")
            link = appdir / ".DirIcon"
            core_member = appdir / appimage.PORTABLE / "README.txt"
            extra = appdir / "surprise"
            for change in ("changed-core", "extra", "link"):
                if change == "changed-core":
                    saved = core_member.read_bytes()
                    core_member.write_bytes(saved + b"changed")
                elif change == "extra":
                    extra.touch()
                else:
                    link.unlink()
                    link.symlink_to("../../outside")
                directory = work / change
                directory.mkdir()
                modified = directory / self.image.name
                offset = info["runtime"]["size"]
                subprocess.run(["mksquashfs", appdir, modified, "-offset", str(offset), "-processors", "2",
                                "-noappend", "-no-progress", "-all-root"], check=True, stdout=subprocess.DEVNULL)
                with self.image.open("rb") as original, modified.open("r+b") as target:
                    target.write(original.read(offset))
                with self.subTest(change=change), self.assertRaises(ValueError):
                    appimage.verify_image(self.core, modified, self.source, directory / "inspection")
                if change == "changed-core":
                    core_member.write_bytes(saved)
                elif change == "extra":
                    extra.unlink()

    def test_undeclared_squashfs_xattr_table_is_rejected(self):
        import os
        import subprocess
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            info, tree = appimage.verify_image(self.core, self.image, self.source, work / "verified")
            os.setxattr(tree / "termcraft.png", "user.termcraft_fixture", b"qualification")
            changed = work / self.image.name
            offset = info["runtime"]["size"]
            subprocess.run(["mksquashfs", tree, changed, "-offset", str(offset), "-processors", "2",
                            "-noappend", "-no-progress", "-all-root", "-xattrs"], check=True, stdout=subprocess.DEVNULL)
            with self.image.open("rb") as original, changed.open("r+b") as target:
                target.write(original.read(offset))
                target.seek(offset + 56)
                self.assertNotEqual(target.read(8), b"\xff" * 8)
            with self.assertRaisesRegex(ValueError, "xattr"):
                appimage.verify_image(self.core, changed, self.source, work / "rejected")

    def test_real_core_with_wrong_architecture_is_rejected(self):
        import json
        import tarfile
        with tempfile.TemporaryDirectory() as tmp:
            work = Path(tmp)
            root, manifest = appimage.core(self.core, work / "original")
            manifest["architecture"] = "aarch64"
            (root / "PACKAGE-MANIFEST.json").write_text(json.dumps(manifest))
            changed = work / self.core.name
            with tarfile.open(changed, "w:gz", compresslevel=1) as tar:
                tar.add(root, arcname=root.name)
            with self.assertRaisesRegex(ValueError, "x86_64"):
                appimage.core(changed, work / "bad")
