import hashlib
import importlib.util
import json
from pathlib import Path
import stat
import tempfile
import unittest
import zipfile


ROOT = Path(__file__).resolve().parents[1]


def load():
    spec = importlib.util.spec_from_file_location("browser_intake", ROOT / "scripts/browser-intake.py")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class BrowserIntake(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tool = load()

    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.archive = self.root / "chromium.zip"
        self.spec = {
            "schema_version": 1, "scope": "qualification-only", "approved": False,
            "platform": "windows", "architecture": "x86_64", "version": "158.0.8127.88",
            "archive_name": self.archive.name, "archive_root": "chromium",
            "archive_url": "https://github.com/ungoogled-software/ungoogled-chromium-windows/releases/download/test/chromium.zip",
            "archive_sha256": "0" * 64, "archive_size": 1,
            "executable": "chrome.exe", "source_revision": "a" * 40,
            "source_url": "https://github.com/ungoogled-software/ungoogled-chromium-windows/tree/" + "a" * 40,
        }

    def zip(self, names=None):
        image = bytearray(256)
        image[:2] = b"MZ"
        image[60:64] = (128).to_bytes(4, "little")
        image[128:134] = b"PE\0\0\x64\x86"
        with zipfile.ZipFile(self.archive, "w") as bundle:
            for name, data in (names or {"chromium/chrome.exe": image, "chromium/icudtl.dat": b"data"}).items():
                bundle.writestr(name, data)
        self.spec["archive_sha256"] = hashlib.sha256(self.archive.read_bytes()).hexdigest()
        self.spec["archive_size"] = self.archive.stat().st_size

    def test_inspection_pins_every_regular_file_and_never_approves(self):
        self.zip()
        result = self.tool.inspect_archive(self.archive, self.spec)
        self.assertEqual([f["path"] for f in result["files"]], ["chrome.exe", "icudtl.dat"])
        self.assertEqual(result["files"][0]["machine"], "x86_64")
        self.assertFalse(result["approved"])
        self.assertEqual(result["scope"], "qualification-only")
        self.assertEqual(result["archive_sha256"], self.spec["archive_sha256"])

    def test_changed_archive_is_rejected_before_inventory(self):
        self.zip()
        self.spec["archive_sha256"] = "0" * 64
        with self.assertRaisesRegex(ValueError, "checksum"):
            self.tool.inspect_archive(self.archive, self.spec)

    def test_archive_paths_cannot_escape_or_alias_on_windows(self):
        for bad in ["../chrome.exe", "chromium/../escape", "chromium/a:b", "chromium/CON.txt", "chromium/a.", "chromium/a\\b", "/chrome.exe"]:
            with self.subTest(bad=bad):
                self.zip({bad: b"bad"})
                with self.assertRaises(ValueError):
                    self.tool.inspect_archive(self.archive, self.spec)
        self.zip({"chromium/chrome.exe": b"fake", "chromium/CHROME.EXE": b"alias"})
        with self.assertRaises(ValueError):
            self.tool.inspect_archive(self.archive, self.spec)

    def test_symlink_and_file_parent_are_rejected(self):
        link = zipfile.ZipInfo("chromium/chrome.exe")
        link.create_system = 3
        link.external_attr = (stat.S_IFLNK | 0o777) << 16
        with zipfile.ZipFile(self.archive, "w") as bundle:
            bundle.writestr(link, "other")
        self.spec["archive_sha256"] = hashlib.sha256(self.archive.read_bytes()).hexdigest()
        self.spec["archive_size"] = self.archive.stat().st_size
        with self.assertRaises(ValueError):
            self.tool.inspect_archive(self.archive, self.spec)
        self.zip({"chromium/a": b"file", "chromium/a/child": b"bad"})
        with self.assertRaises(ValueError):
            self.tool.inspect_archive(self.archive, self.spec)
        self.zip({"chromium/A": b"file", "chromium/a/child": b"bad"})
        with self.assertRaises(ValueError):
            self.tool.inspect_archive(self.archive, self.spec)

    def test_foreign_architecture_and_missing_executable_are_rejected(self):
        self.zip({"chromium/chrome.exe": b"not PE"})
        with self.assertRaisesRegex(ValueError, "PE"):
            self.tool.inspect_archive(self.archive, self.spec)
        self.zip({"chromium/other": b"data"})
        with self.assertRaisesRegex(ValueError, "executable"):
            self.tool.inspect_archive(self.archive, self.spec)

    def test_bound_is_checked_before_reading_archive_members(self):
        self.zip()
        with self.assertRaisesRegex(ValueError, "bound"):
            self.tool.inspect_archive(self.archive, self.spec, max_bytes=32)

    def test_candidate_can_never_be_an_approval_or_http_download(self):
        self.zip()
        for key, value in [("approved", True), ("scope", "release"), ("archive_url", "http://example.test/input.zip"), ("archive_name", "../bad.zip")]:
            candidate = {**self.spec, key: value}
            with self.subTest(key=key), self.assertRaises(ValueError):
                self.tool.validate_candidate(candidate)

    def test_placeholder_and_truncated_credits_never_count_as_notices(self):
        for data in [b"<html>sample text generate_about_credits=true</html>", b"<html>credits</html>", b"\xff" * 20000]:
            with self.assertRaises(ValueError):
                self.tool.credits_summary(data)
        data = ("<html><body>" + "<section>Copyright MIT license BSD license Apache license</section>" * 400 + "</body></html>").encode()
        result = self.tool.credits_summary(data)
        self.assertEqual(result["sha256"], hashlib.sha256(data).hexdigest())
        self.assertFalse(result["redistribution_approved"])


if __name__ == "__main__":
    unittest.main()
