import importlib.util
import io
import json
import os
from pathlib import Path
import tarfile
import tempfile
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]


@unittest.skipUnless(os.name == "posix", "Debian package modes and symlinks require POSIX")
class DebianPackage(unittest.TestCase):
    def test_deb_layout_checksums_dependencies_and_no_replace(self):
        spec = importlib.util.spec_from_file_location("debian", ROOT / "scripts/package-debian.py")
        debian = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(debian)
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary).resolve()
            archive = root / "TermCraft-portable.tar.gz"
            files = {"harbor-workspace": b"app", "termcraft-rdp-engine": b"helper",
                     "CREDITS.md": b"credits", "assets/termcraft-logo.png": b"logo"}
            manifest = {
                "platform": "linux", "architecture": "x86_64", "profile": "release",
                "version": "0.1.0", "runtime": {"linked_libraries": {
                    "harbor-workspace": {"libraries": ["libgcc_s.so.1", "libc.so.6"],
                                         "highest_required_glibc_symbol_version": "2.43"},
                    "termcraft-rdp-engine": {"libraries": ["libc.so.6"],
                                             "highest_required_glibc_symbol_version": "2.34"},
                }},
                "files": {name: debian.digest(data) for name, data in files.items()},
            }
            files["PACKAGE-MANIFEST.json"] = json.dumps(manifest).encode()
            with tarfile.open(archive, "w:gz") as bundle:
                for name, data in files.items():
                    member = tarfile.TarInfo(f"TermCraft/{name}")
                    member.size = len(data)
                    bundle.addfile(member, io.BytesIO(data))
            package = debian.package_debian(archive, root / "out")
            revised = debian.package_debian(archive, root / "revised", debian_revision="1")
            self.assertEqual(revised.name, "termcraft_0.1.0-1_amd64.deb")
            ar_revision = {}
            revised_bytes = revised.read_bytes()
            cursor = 8
            while cursor < len(revised_bytes):
                header = revised_bytes[cursor:cursor + 60]
                length = int(header[48:58])
                ar_revision[header[:16].decode().strip().rstrip("/")] = revised_bytes[cursor + 60:cursor + 60 + length]
                cursor += 60 + length + length % 2
            with tarfile.open(fileobj=io.BytesIO(ar_revision["control.tar.gz"])) as control:
                self.assertIn("Version: 0.1.0-1\n", control.extractfile("./control").read().decode())
            with tarfile.open(fileobj=io.BytesIO(ar_revision["data.tar.gz"])) as data:
                packaged = json.load(data.extractfile("./usr/share/doc/termcraft/debian-package-manifest.json"))
                self.assertEqual(packaged["version"], "0.1.0-1")
                portable = json.load(data.extractfile("./opt/termcraft/PACKAGE-MANIFEST.json"))
                self.assertEqual(portable, manifest)
            for value in ("0", "01", "-1", "+1", "1.0", "1-1", "1000000000", "", "١", True, 1):
                output = root / ("invalid-" + str(value))
                with self.assertRaises(ValueError, msg=repr(value)):
                    debian.package_debian(archive, output, debian_revision=value)
                self.assertFalse(output.exists())
            maximum = debian.package_debian(archive, root / "maximum", debian_revision="999999999")
            self.assertEqual(maximum.name, "termcraft_0.1.0-999999999_amd64.deb")
            valid_cli = root / "valid-cli"
            result = subprocess.run([sys.executable, str(ROOT / "scripts/package-debian.py"), str(archive), "--output", str(valid_cli), "--debian-revision", "2"], capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr.decode())
            self.assertTrue((valid_cli / "termcraft_0.1.0-2_amd64.deb").is_file())
            cli_output = root / "invalid-cli"
            result = subprocess.run([sys.executable, str(ROOT / "scripts/package-debian.py"), str(archive), "--output", str(cli_output), "--debian-revision", "01"], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(cli_output.exists())
            manifest["files"]["harbor-workspace"] = "0" * 64
            with tarfile.open(root / "tampered.tar.gz", "w:gz") as bundle:
                for name, value in files.items():
                    if name == "PACKAGE-MANIFEST.json": value = json.dumps(manifest).encode()
                    member = tarfile.TarInfo(f"TermCraft/{name}"); member.size = len(value)
                    bundle.addfile(member, io.BytesIO(value))
            with self.assertRaisesRegex(ValueError, "differ from the manifest"):
                debian.package_debian(root / "tampered.tar.gz", root / "tampered-output", debian_revision="1")
            contents = package.read_bytes()
            self.assertEqual(contents[:8], b"!<arch>\n")
            pos = 8
            ar = {}
            while pos < len(contents):
                header = contents[pos:pos + 60]
                length = int(header[48:58])
                ar[header[:16].decode().strip().rstrip("/")] = contents[pos + 60:pos + 60 + length]
                pos += 60 + length + length % 2
            self.assertEqual(ar["debian-binary"], b"2.0\n")
            with tarfile.open(fileobj=io.BytesIO(ar["control.tar.gz"])) as control:
                fields = control.extractfile("./control").read().decode()
                self.assertIn("Version: 0.1.0\n", fields)
                self.assertIn("Architecture: amd64", fields)
                self.assertIn("libc6 (>= 2.43)", fields)
                self.assertIn("libgcc-s1", fields)
                self.assertNotIn("preinst", control.getnames())
            with tarfile.open(fileobj=io.BytesIO(ar["data.tar.gz"])) as data:
                self.assertEqual(data.extractfile("./opt/termcraft/termcraft-rdp-engine").read(), b"helper")
                self.assertEqual(data.getmember("./opt/termcraft/harbor-workspace").mode, 0o755)
                self.assertEqual(data.getmember("./usr/bin/termcraft").linkname, "/opt/termcraft/harbor-workspace")
                desktop = data.extractfile("./usr/share/applications/org.termcraft.TermCraft.desktop").read()
                self.assertIn(b"Exec=/usr/bin/termcraft", desktop)
                self.assertFalse(any("workspace.json" in name or "home/" in name for name in data.getnames()))
            self.assertEqual(debian.package_debian(archive, root / "second").read_bytes(), contents)
            with self.assertRaises(FileExistsError):
                debian.package_debian(archive, root / "out")


if __name__ == "__main__":
    unittest.main()
