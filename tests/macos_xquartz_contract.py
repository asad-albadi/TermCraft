import unittest
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]


class PrivateXQuartzBuildContract(unittest.TestCase):
    def test_private_builder_uses_pinned_source_and_never_runs_install_hooks(self):
        script = ROOT / "packaging/x11/build-private-macos.sh"
        self.assertTrue(script.is_file())
        text = script.read_text()
        for flag in (
            "--wrap-mode=nodownload",
            "-Dxquartz=true",
            "-Dxorg=false",
            "-Dxephyr=false",
            "-Dxnest=false",
            "-Dxvfb=false",
            "-Dxcsecurity=true",
            "-Dbundle-id-prefix=org.termcraft.private",
            "-Dsparkle-feed-url=",
            "-Dsparkle-public-edkey=",
            "meson compile",
            "install_name_tool",
            "codesign",
        ):
            self.assertIn(flag, text)
        self.assertNotIn("meson install", text)
        self.assertNotIn("pkgutil --expand-full", text)

    def test_private_builder_requires_an_explicit_source_archive(self):
        text = (ROOT / "packaging/x11/build-private-macos.sh").read_text()
        self.assertIn("--source-archive", text)
        self.assertIn("prepare-source.py", text)
        self.assertIn("x86_64", text)
        self.assertIn("arm64", text)

    def test_private_builder_walks_the_complete_macho_dependency_closure(self):
        text = (ROOT / "packaging/x11/build-private-macos.sh").read_text()
        self.assertIn('queue=("$destination/opt/X11/bin/X11.bin")', text)
        self.assertIn("while ((${#queue[@]})); do", text)
        self.assertIn('queue+=("$target")', text)
        self.assertIn("Missing Mach-O dependency", text)


if __name__ == "__main__":
    unittest.main()
