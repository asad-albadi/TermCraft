"""Focused offline-input safety checks for the Mosh dependency packager."""
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[1]
spec = importlib.util.spec_from_file_location("stage_mosh_dependencies", ROOT / "packaging/runtime/stage-mosh-dependencies.py")
stage = importlib.util.module_from_spec(spec)
spec.loader.exec_module(stage)

sys.path.insert(0, str(ROOT / "scripts"))
from portable_archive import extract_tar

class DependencyInputs(unittest.TestCase):
    def test_exact_input_is_accepted_but_replacement_is_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory)
            path = cache / "pinned.archive"
            payload = b"authenticated dependency package"
            pin = hashlib.sha256(payload).hexdigest()
            path.write_bytes(payload)
            self.assertEqual(stage.input_file(cache, path.name, pin), payload)
            path.write_bytes(b"replacement bytes")
            with self.assertRaisesRegex(ValueError, "checksum mismatch"):
                stage.input_file(cache, path.name, pin)

    def test_absent_and_symlinked_inputs_never_use_another_source(self):
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory)
            with self.assertRaisesRegex(ValueError, "bounded regular"):
                stage.input_file(cache, "absent", "0" * 64)
            actual = cache / "actual"
            actual.write_bytes(b"exact bytes")
            (cache / "linked").symlink_to(actual)
            with self.assertRaisesRegex(ValueError, "bounded regular"):
                stage.input_file(cache, "linked", hashlib.sha256(actual.read_bytes()).hexdigest())

    def test_output_does_not_overwrite_existing_files_or_escape(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stage.output(root, "bin/perl", b"first", True)
            with self.assertRaises(FileExistsError):
                stage.output(root, "bin/perl", b"replacement", True)
            for name in ["../escape", "/absolute"]:
                with self.assertRaisesRegex(ValueError, "Invalid pinned output"):
                    stage.output(root, name, b"bad")
            self.assertEqual((root / "bin/perl").read_bytes(), b"first")

    def test_pinned_source_notices_survive_strict_portable_extraction(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            cache = root / "cache"
            cache.mkdir()
            binary = io.BytesIO()
            with tarfile.open(fileobj=binary, mode="w"):
                pass

            packages = []
            expected = {}
            for package, filename in (
                ("epoch", "zlib-1:1.4-1-x86_64.pkg.tar.zst"),
                ("literal-escape", "zlib-1~3A1.4-1-x86_64.pkg.tar.zst"),
            ):
                package_info = {"name": package, "filename": filename, "inventory": {}}
                for role, name, data, pin_key in (
                    ("archive", filename, binary.getvalue(), "archive_sha256"),
                    ("signature", filename + ".sig", package.encode(), "signature_sha256"),
                    ("source", "shared-source.tar.gz", b"shared source", "source_sha256"),
                    ("recipe", "shared-recipe.tar.gz", b"shared recipe", "recipe_sha256"),
                ):
                    (cache / name).write_bytes(data)
                    pin = hashlib.sha256(data).hexdigest()
                    package_info[pin_key] = pin
                    if role in ("source", "recipe"):
                        package_info[role + "_filename"] = name
                    if role != "archive":
                        expected[package, role] = name, pin, data
                packages.append(package_info)
            info = {"schema_version": 1, "platform": "linux", "architecture": "x86_64",
                    "root": "mosh-deps", "packages": packages, "files": []}
            manifest = root / "pins.json"
            manifest.write_text(json.dumps(info))
            payload = root / "payload"
            with patch.object(stage.subprocess, "run", side_effect=lambda *args, **kw:
                              stage.subprocess.CompletedProcess(args[0], 0, stdout=kw["input"])):
                stage.stage(manifest, payload, cache)
            archive = root / "portable.tar"
            with tarfile.open(archive, "w") as bundle:
                bundle.add(payload / "mosh-deps", arcname="mosh-deps")
            extract_tar(archive, root / "extracted")
            runtime = root / "extracted" / "mosh-deps"

            self.assertEqual((runtime / "MOSH-DEPENDENCIES.json").read_bytes(), manifest.read_bytes())
            provenance = json.loads((runtime / "MOSH-SOURCES.json").read_bytes())
            self.assertEqual(provenance["schema_version"], 1)
            staged = set()
            for (package, role), (name, pin, data) in expected.items():
                record = provenance["packages"][package][role]
                self.assertEqual((record["original_filename"], record["sha256"]), (name, pin))
                self.assertEqual((runtime / record["path"]).read_bytes(), data)
                staged.add(record["path"])
            self.assertEqual(len(staged), 4)
            self.assertEqual(staged, {p.relative_to(runtime).as_posix() for p in (runtime / "sources").iterdir()})

if __name__ == "__main__":
    unittest.main()
