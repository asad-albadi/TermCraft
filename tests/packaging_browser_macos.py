"""Offline framework layout and qualification tests for Apple Silicon Chromium."""
import importlib.util
import json
import os
from pathlib import Path
import stat
import subprocess
import tempfile
import unittest
import zipfile

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("chromium_macos_stage", ROOT / "packaging/browser/stage-macos.py")
stage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage)


def write_zip(path, target, extra=None):
    with zipfile.ZipFile(path, "w") as archive:
        binary = zipfile.ZipInfo("chrome-mac/Chromium.app/Framework/Versions/1/Binary")
        binary.create_system = 3
        binary.external_attr = (stat.S_IFREG | 0o644) << 16
        archive.writestr(binary, b"fixture")
        link = zipfile.ZipInfo("chrome-mac/Chromium.app/Framework/Versions/Current")
        link.create_system = 3
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        archive.writestr(link, target)
        if extra:
            archive.writestr(extra, b"unexpected")


class FrameworkLayout(unittest.TestCase):
    def test_exact_framework_link_is_preserved(self):
        with tempfile.TemporaryDirectory() as scratch:
            base = Path(scratch)
            archive = base / "fixture.zip"
            write_zip(archive, "1")
            link = "Chromium.app/Framework/Versions/Current"
            stage.extract_app(archive, base / "output",
                              {"Chromium.app/Framework/Versions/1/Binary": False}, {link: "1"})
            self.assertEqual(os.readlink(base / "output" / link), "1")
            self.assertEqual((base / "output" / link / "Binary").read_bytes(), b"fixture")

    def test_unsafe_link_and_unlisted_members_are_rejected(self):
        for target, extra in (("../../outside", None), ("/tmp", None), ("2", None),
                              ("1", "chrome-mac/Chromium.app/Framework/Versions/Current/Injected"),
                              ("1", "chrome-mac/Chromium.app/../outside")):
            with self.subTest(target=target, extra=extra), tempfile.TemporaryDirectory() as scratch:
                base = Path(scratch)
                archive = base / "fixture.zip"
                write_zip(archive, target, extra)
                link = "Chromium.app/Framework/Versions/Current"
                with self.assertRaises(ValueError):
                    stage.extract_app(archive, base / "output",
                                      {"Chromium.app/Framework/Versions/1/Binary": False}, {link: "1"})
                self.assertFalse(os.path.lexists(base / "output" / link))

    @unittest.skipUnless(os.environ.get("TERMCRAFT_MACOS_CHROMIUM_QA_ZIP"), "needs relocated QA ZIP")
    def test_qa_zip_preserves_exact_link_text_without_dereferencing(self):
        pin = json.loads(stage.MANIFEST.read_text())
        prefix = "TermCraft.app/Contents/Resources/" + pin["root"] + "/"
        with zipfile.ZipFile(os.environ["TERMCRAFT_MACOS_CHROMIUM_QA_ZIP"]) as archive:
            entries = [entry for entry in archive.infolist() if entry.filename.startswith(prefix)]
            actual = {entry.filename.removeprefix(prefix): archive.read(entry)
                      for entry in entries if stat.S_ISLNK(entry.external_attr >> 16)}
        self.assertEqual(actual, {item["path"]: item["target"].encode() for item in pin["links"]})
        self.assertEqual(len(actual), 5)

    @unittest.skipUnless(os.environ.get("TERMCRAFT_MACOS_CHROMIUM_INPUTS"), "needs pinned offline inputs")
    def test_real_snapshot_stages_with_strict_resource_seal_and_rejects_bad_archive(self):
        inputs = Path(os.environ["TERMCRAFT_MACOS_CHROMIUM_INPUTS"])
        with tempfile.TemporaryDirectory() as scratch:
            base = Path(scratch)
            clean = base / "clean"
            payload = stage.stage(clean, inputs)
            app = payload / "Chromium.app"
            subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", app], check=True)
            executable = app / "Contents/MacOS/Chromium"
            self.assertEqual(subprocess.check_output([executable, "--version"]).strip(),
                             b"Chromium 160.0.8185.0")
            with self.assertRaises(FileExistsError):
                stage.stage(clean, inputs)
            bad = base / "bad"
            bad.mkdir()
            (bad / "chrome-mac.zip").write_bytes(b"substitution")
            with self.assertRaisesRegex(ValueError, "checksum"):
                stage.stage(base / "rejected", bad)
            self.assertFalse((base / "rejected/browser/macos-aarch64").exists())


if __name__ == "__main__":
    unittest.main()
