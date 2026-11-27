import importlib.util
from pathlib import Path
import os
import subprocess
import shutil
import tempfile
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
SPEC = importlib.util.spec_from_file_location("native", ROOT / "scripts/package-native-lifecycle.py")


def native():
    module = importlib.util.module_from_spec(SPEC)
    SPEC.loader.exec_module(module)
    return module


class NativeQualification(unittest.TestCase):
    def test_private_version_is_numeric_and_strictly_lower(self):
        n = native()
        for original, previous in (("0.1.0", "0.0.0"), ("0.0.1", "0.0.0"), ("12.3.4", "12.3.3")):
            self.assertEqual(n.previous_version(original), previous)
        for invalid in ("0.0.0", "01.0.0", "0.1.0-beta"):
            with self.assertRaises(ValueError):
                n.previous_version(invalid)

    def test_patch_only_exact_root_version_and_lock_self_entry(self):
        n = native()
        with tempfile.TemporaryDirectory() as work:
            root = Path(work)
            for name in ("Cargo.toml", "Cargo.lock"):
                data = (ROOT / name).read_bytes()
                (root / name).write_bytes(data)
            original = {name: (root / name).read_bytes() for name in ("Cargo.toml", "Cargo.lock")}
            version = n.DELIVERY.package_versions(ROOT)["harbor-workspace"]
            old = n.previous_version(version)
            patches = n.patch_version(root, version, old)
            for name, before in original.items():
                changed = (root / name).read_bytes()
                self.assertEqual(patches[name], {"before": n.digest(before), "after": n.digest(changed)})
                anchor = (('name = "harbor-workspace"\nlicense = "MIT"\n' if name == "Cargo.toml" else 'name = "harbor-workspace"\n')
                          + f'version = "{version}"').encode()
                self.assertEqual(changed, before.replace(anchor, anchor.replace(version.encode(), old.encode()), 1))

    def test_aggregate_refuses_missing_native_evidence(self):
        n = native()
        with tempfile.TemporaryDirectory() as work:
            root = Path(work)
            with self.assertRaisesRegex(ValueError, "Lifecycle evidence"):
                n.DELIVERY.verify_native_evidence(ROOT, root / "missing", None, "linux", "0" * 40, "0.1.0", {}, {}, {})


    def test_trace_requires_successful_connect_to_specific_private_unix_socket(self):
        n = native()
        socket = Path("/tmp/owned/termcraft-release-wayland")
        line = b'  42 connect(9, {sa_family=AF_UNIX, sun_path="/tmp/owned/termcraft-release-wayland"}, 54) = 0'
        self.assertEqual(n.STARTUP.successful_wayland_connect(b"prefix\n" + line + b"\n", socket), line)
        for invalid in (line.replace(b"= 0", b"= -1 ECONNREFUSED"), line.replace(b"/owned/", b"/other/"),
                        line.replace(b"AF_UNIX", b"AF_INET")):
            self.assertIsNone(n.STARTUP.successful_wayland_connect(invalid, socket))

    def test_native_installer_refuses_a_non_hosted_environment_before_installing(self):
        n = native()
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            output = root / "absent"
            env = {key: value for key, value in os.environ.items() if key not in {"GITHUB_ACTIONS", "RUNNER_ENVIRONMENT"}}
            result = subprocess.run([sys.executable, n.__file__, "qualify", "--source", "0" * 40,
                                     "--platform", "linux", "--version", "0.1.0", "--dist", root,
                                     "--evidence", output, "--build-root", root, "--disposable-runner"],
                                    capture_output=True, env=env, timeout=15)
            self.assertEqual(result.returncode, 1)
            self.assertIn(b"requires a disposable GitHub-hosted runner", result.stderr)
            self.assertFalse(output.exists())


    @unittest.skipUnless(shutil.which("pwsh"), "requires PowerShell to run the standalone driver")
    def test_standalone_windows_driver_refuses_before_resolving_paths_without_qualification_state(self):
        env = {key: value for key, value in os.environ.items() if key not in {"GITHUB_ACTIONS", "RUNNER_ENVIRONMENT"}}
        for extra in ([], ["-PreviousArchive", "nonexistent.zip", "-UsePackageVersions"]):
            with self.subTest(extra=extra):
                result = subprocess.run(["pwsh", "-NoProfile", "-File", ROOT / "tests/windows-package-lifecycle.ps1",
                                         "-Archive", "standalone-archive-that-does-not-exist.zip", *extra],
                                        env=env, capture_output=True, timeout=20)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(b"disposable GitHub-hosted Windows runner", result.stderr)
                self.assertNotIn(b"Cannot find path", result.stderr)

    def test_fixture_values_are_fresh_and_passed_only_by_pipe(self):
        from unittest.mock import patch

        n = native()
        first, other = n.fixture_input(), n.fixture_input()
        self.assertTrue(set(first).isdisjoint(other))
        self.assertTrue(all(len(value) >= 32 for value in (*first, *other)))
        with tempfile.TemporaryDirectory() as work:
            with patch.object(n.STARTUP, "run", return_value=n.DELIVERY.FIXTURE_PROOF) as launch:
                with patch.object(n, "persistent", return_value={}):
                    n.fixture("fixture", Path(work), "verify", Path(work) / "log", first)
                args, kwargs = launch.call_args
                self.assertEqual(kwargs["input_bytes"], n.encode_fixture_input(first))
                self.assertTrue(all(value not in repr(args) for value in first))
