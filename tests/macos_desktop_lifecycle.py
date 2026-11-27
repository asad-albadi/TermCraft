import importlib.util
import json
import plistlib
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


def module_at(path):
    spec = importlib.util.spec_from_file_location("macos_desktop_integration", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class DesktopLifecycle(unittest.TestCase):
    def test_removal_rejects_same_identifier_with_wrong_executable(self):
        integration_module = module_at(ROOT / "packaging/macos/desktop-integration.py")
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            bundle = root / "TermCraft.app"
            contents = bundle / "Contents"
            (contents / "MacOS").mkdir(parents=True)
            (contents / "MacOS" / "termcraft-desktop-launcher").write_bytes(b"launcher")
            (contents / "Info.plist").write_bytes(plistlib.dumps({
                "CFBundleIdentifier": "org.termcraft.TermCraft",
                "CFBundleExecutable": "termcraft-desktop-launcher",
            }))
            receipt = root / "registration.json"
            receipt.write_text(json.dumps({
                "owner": "org.termcraft.TermCraft",
                "version": 1,
                "bundles": [str(bundle.resolve())],
            }))
            integration = integration_module.DesktopIntegration(bundle, receipt)
            integration._run = lambda *args: f"path: {bundle.resolve()} (0x2520)\n"
            self.assertTrue(integration.status())
            (contents / "Info.plist").write_bytes(plistlib.dumps({
                "CFBundleIdentifier": "org.termcraft.TermCraft",
                "CFBundleExecutable": "different-launcher",
            }))
            with self.assertRaisesRegex(ValueError, "different application"):
                integration.remove_user()
            self.assertTrue(receipt.exists())


if __name__ == "__main__":
    unittest.main()
