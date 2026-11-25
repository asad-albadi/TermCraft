import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
VALIDATOR = ROOT / "scripts" / "validate-macos-evidence.py"


def record(**overrides):
    value = {
        "schema_version": 1,
        "status": "unavailable",
        "source_revision": "0123456789abcdef0123456789abcdef01234567",
        "host": {"os": "macOS", "architecture": "arm64"},
        "target": {
            "architecture": "arm64",
            "deployment_target": "14.0",
            "sdk_version": "15.0",
            "toolchain": "Apple clang 17.0.0",
        },
        "lockfiles": ["Cargo.lock"],
        "artifacts": [],
        "workflows": [{"name": "unit", "result": "not-run"}],
        "unavailable": [{"requirement": "native artifact", "reason": "No pinned fixture"}],
    }
    value.update(overrides)
    return value


class MacOSEvidenceContractTests(unittest.TestCase):
    def run_validator(self, value):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "evidence.json"
            path.write_text(json.dumps(value), encoding="utf-8")
            return subprocess.run(
                [sys.executable, str(VALIDATOR), str(path)],
                capture_output=True,
                text=True,
                check=False,
            )

    def test_valid_unavailable_record_is_accepted(self):
        result = self.run_validator(record())
        self.assertEqual(result.returncode, 0, result.stderr)

    def test_missing_identity_and_unavailable_reason_is_rejected(self):
        value = record()
        del value["target"]["sdk_version"]
        value["unavailable"] = []
        result = self.run_validator(value)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("target.sdk_version", result.stderr)
        self.assertIn("unavailable", result.stderr)

    def test_artifact_hash_and_path_are_checked(self):
        value = record(
            status="qualified",
            unavailable=[],
            artifacts=[{"path": "TermCraft.app/Contents/MacOS/TermCraft", "sha256": "bad"}],
        )
        result = self.run_validator(value)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("artifacts[0].sha256", result.stderr)


if __name__ == "__main__":
    unittest.main()
