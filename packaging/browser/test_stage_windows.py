"""Exercise exact Windows browser staging with synthetic archive members."""
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import zipfile

HERE = Path(__file__).resolve().parent
SPEC = importlib.util.spec_from_file_location("stage", HERE / "stage-runtime.py")
STAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(STAGE)


class WindowsBrowserStaging(unittest.TestCase):
    def test_unapproved_pin_refuses_staging_before_input_access(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            with self.assertRaisesRegex(ValueError, "No qualified bundled Chromium"):
                STAGE.stage_windows(HERE / "runtime-manifest-windows.json", root / "out", root)
            self.assertFalse((root / "out").exists())

    def test_windows_release_gate_uses_repository_pin(self):
        repo = HERE.parents[1]
        spec = importlib.util.spec_from_file_location("package", repo / "scripts/package-release.py")
        package = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(package)
        with self.assertRaisesRegex(ValueError, "No qualified bundled Chromium"):
            package.browser_runtime_manifest(repo, "windows", "x86_64")
        _, linux = package.browser_runtime_manifest(repo, "linux", "x86_64")
        self.assertEqual(linux["root"], "browser/linux-x86_64")

    def test_pinned_zip_and_notice_inventory_publish_atomically(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            manifest = root / "pin.json"
            archive = root / "chromium.zip"
            files = {"chrome.exe": b"image", "THIRD-PARTY-NOTICES.html": b"notices"}
            pin = json.loads((HERE / "runtime-manifest-windows.json").read_text())
            pin.update(approved=True, archive_name=archive.name, files=[
                {"path": name, "size": len(data), "sha256": hashlib.sha256(data).hexdigest(),
                 "executable": name.endswith(".exe")}
                for name, data in files.items()
            ])
            with zipfile.ZipFile(archive, "w") as payload:
                for name, data in files.items():
                    payload.writestr(name, data)
                payload.writestr("Chrome.exe", b"duplicate case")
            pin["archive_sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
            manifest.write_text(json.dumps(pin))
            with self.assertRaisesRegex(ValueError, "Unexpected Chromium ZIP member"):
                STAGE.stage_windows(manifest, root / "bad", root)
            self.assertFalse((root / "bad" / pin["root"]).exists())
            with zipfile.ZipFile(archive, "w") as payload:
                payload.writestr("chrome.exe", b"other")
                payload.writestr("THIRD-PARTY-NOTICES.html", files["THIRD-PARTY-NOTICES.html"])
            pin["archive_sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
            manifest.write_text(json.dumps(pin))
            with self.assertRaisesRegex(ValueError, "checksum"):
                STAGE.stage_windows(manifest, root / "tampered", root)
            self.assertFalse((root / "tampered" / pin["root"]).exists())
            with zipfile.ZipFile(archive, "w") as payload:
                for name, data in files.items():
                    payload.writestr(name, data)
            pin["archive_sha256"] = hashlib.sha256(archive.read_bytes()).hexdigest()
            manifest.write_text(json.dumps(pin))
            STAGE.stage_windows(manifest, root / "out", root)
            staged = root / "out" / pin["root"]
            for name, data in files.items():
                self.assertEqual((staged / name).read_bytes(), data)
            self.assertEqual((staged / "BROWSER-MANIFEST.json").read_bytes(), manifest.read_bytes())
