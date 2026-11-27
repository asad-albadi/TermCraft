import unittest
from pathlib import Path


ROOT = Path(__file__).parents[1]


class ReleaseWorkflowContract(unittest.TestCase):
    def test_macos_setup_block_is_valid_bash_and_runs_native_helper_tests(self):
        workflow = (ROOT / ".github/workflows/release.yml").read_text()
        start = workflow.index("      - if: runner.os == 'macOS'")
        end = workflow.index("      - run: cargo test --locked --test package_manifest", start)
        block = workflow[start:end]
        self.assertIn("shell: bash", block)
        self.assertIn("xcodebuild -version", block)
        self.assertIn(
            "cargo test --locked --manifest-path engines/rdp/Cargo.toml", block
        )
        self.assertNotRegex(block, r"^\s*\}\s*$", msg="macOS block must be valid bash")


if __name__ == "__main__":
    unittest.main()
