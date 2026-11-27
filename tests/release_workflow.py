import os
from pathlib import Path
import subprocess
import tempfile
import unittest

import yaml


ROOT = Path(__file__).resolve().parents[1]


def workflow(name):
    # BaseLoader preserves 'on' and boolean-looking scalars, unlike the YAML
    # 1.1 SafeLoader, while still constructing the workflow's mapping/sequence.
    return yaml.load((ROOT / ".github/workflows" / name).read_text(), Loader=yaml.BaseLoader)


class ReleaseWorkflowContract(unittest.TestCase):
    def test_mac_package_step_traverses_native_helper_tests(self):
        release = workflow("release.yml")
        step = next(step for step in release["jobs"]["package"]["steps"]
                    if step.get("name") == "Prepare macOS native package toolchain")
        self.assertEqual(step["if"], "runner.os == 'macOS'")
        self.assertEqual(step["shell"], "bash")
        with tempfile.TemporaryDirectory() as temporary:
            work = Path(temporary)
            output = work / "calls"
            outputs = work / "github.env"
            for command, code in {
                "uname": "printf 'arm64\\n'",
                "sw_vers": ':',
                "xcodebuild": 'printf "xcodebuild %s\\n" "$*" >> "$CALLS"',
                "xcrun": 'printf "xcrun %s\\n" "$*" >> "$CALLS"',
                "cargo": 'printf "cargo %s\\n" "$*" >> "$CALLS"',
            }.items():
                program = work / command
                program.write_text(f"#!/bin/sh\n{code}\n")
                program.chmod(0o755)
            subprocess.run(["bash", "-euo", "pipefail", "-c", step["run"]], check=True,
                           env={**os.environ, "PATH": f'{work}{os.pathsep}{os.environ["PATH"]}',
                                "GITHUB_ENV": str(outputs), "CALLS": str(output), "CARGO_BUILD_JOBS": "2"})
            self.assertEqual(output.read_text().splitlines(), [
                "xcodebuild -version", "xcrun --sdk macosx --show-sdk-version",
                "cargo test --locked --manifest-path engines/rdp/Cargo.toml",
            ])
            self.assertEqual(outputs.read_text().strip(), "MACOSX_DEPLOYMENT_TARGET=14.0")

    def test_release_dependencies_cover_all_native_runners_and_isolate_write_permission(self):
        release = workflow("release.yml")
        ci = workflow("ci.yml")
        self.assertIn("workflow_dispatch", release["on"])
        self.assertEqual(release["on"]["push"]["tags"], ["v*"])
        self.assertIn("workflow_call", ci["on"])
        self.assertEqual(release["permissions"], {"contents": "read"})
        jobs = release["jobs"]
        self.assertEqual(jobs["regression"]["uses"], "./.github/workflows/ci.yml")
        self.assertEqual(jobs["regression"]["needs"], "preflight")
        variants = jobs["package"]["strategy"]["matrix"]["include"]
        targets = {(item["runner"], item["platform"]) for item in variants}
        self.assertEqual(targets, {("ubuntu-24.04", "linux"), ("windows-2025", "windows"), ("macos-15", "macos")})
        self.assertEqual({job["runs-on"] for job in ci["jobs"].values()}, {runner for runner, _ in targets})
        self.assertEqual(ci["jobs"]["regression-macos-arm64"]["env"]["TERMCRAFT_REQUIRE_CHROMIUM"], "1")
        self.assertEqual(set(jobs["aggregate"]["needs"]), {"preflight", "regression", "package"})
        self.assertEqual(set(jobs["publish"]["needs"]), {"preflight", "aggregate"})
        self.assertEqual([name for name, job in jobs.items() if job.get("permissions") == {"contents": "write"}],
                         ["publish"])
        for job in [*jobs.values(), *ci["jobs"].values()]:
            for step in job.get("steps", []):
                if step.get("uses", "").startswith("actions/checkout@"):
                    self.assertEqual(step["with"]["persist-credentials"], "false")


if __name__ == "__main__":
    unittest.main()
