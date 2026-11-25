"""Check the Linux startup probe against real child executable images."""
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import unittest


@unittest.skipUnless(sys.platform == "linux", "Linux memfd required")
class RdpPackagedImage(unittest.TestCase):
    def test_expected_bytes_and_all_seals_are_required_for_memfd(self):
        import fcntl

        helper = Path(shutil.which("sleep") or "/usr/bin/sleep").resolve(strict=True)
        script = Path(__file__).resolve().parents[1] / "scripts/package-check-startup.py"
        spec = importlib.util.spec_from_file_location("startup_check", script)
        probe = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(probe)
        expected = probe.sha256(helper)
        with subprocess.Popen([helper, "5"]) as child:
            try:
                actual = Path(f"/proc/{child.pid}/exe")
                self.assertFalse(probe.linux_rdp_helper_matches(actual, expected))
            finally:
                child.terminate()
        seals = fcntl.F_SEAL_WRITE | fcntl.F_SEAL_GROW | fcntl.F_SEAL_SHRINK | fcntl.F_SEAL_SEAL
        for name, flags, accepted in [
            ("termcraft-rdp-engine", seals, True),
            ("termcraft-rdp-engine", 0, False),
            ("other-engine", seals, False),
        ]:
            with self.subTest(name=name, seals=flags):
                descriptor = os.memfd_create(name, os.MFD_ALLOW_SEALING)
                try:
                    with helper.open("rb") as source, os.fdopen(os.dup(descriptor), "wb") as image:
                        shutil.copyfileobj(source, image)
                    os.fchmod(descriptor, 0o500)
                    if flags:
                        fcntl.fcntl(descriptor, fcntl.F_ADD_SEALS, flags)
                    with subprocess.Popen(
                        [f"/proc/self/fd/{descriptor}", "5"], pass_fds=(descriptor,),
                    ) as child:
                        try:
                            actual = Path(f"/proc/{child.pid}/exe")
                            self.assertEqual(
                                probe.linux_rdp_helper_matches(actual, expected),
                                accepted,
                            )
                            self.assertFalse(
                                probe.linux_rdp_helper_matches(actual, "0" * 64),
                            )
                        finally:
                            child.terminate()
                finally:
                    os.close(descriptor)


if __name__ == "__main__":
    unittest.main()
