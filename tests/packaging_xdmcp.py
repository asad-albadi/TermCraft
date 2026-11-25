#!/usr/bin/env python3
import hashlib,importlib.util,json,os,pathlib,tempfile,unittest
ROOT=pathlib.Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('package_release',ROOT/'scripts/package-release.py');packager=importlib.util.module_from_spec(spec);spec.loader.exec_module(packager)
class XephyrPackage(unittest.TestCase):
    @unittest.skipUnless('TERMCRAFT_TEST_XEPHYR_ARCHIVE' in os.environ, 'requires explicit TERMCRAFT_TEST_XEPHYR_ARCHIVE pinned archive input')
    def test_actual_pinned_payload_stages_with_notices_and_rejects_corruption(self):
        archive=pathlib.Path(os.environ['TERMCRAFT_TEST_XEPHYR_ARCHIVE'])
        with tempfile.TemporaryDirectory() as temp:
            output=pathlib.Path(temp)/'good';output.mkdir()
            info,dependencies=packager.stage_xephyr(ROOT,output,archive)
            self.assertEqual(hashlib.sha256((output/info['entry_point']).read_bytes()).hexdigest(),info['executable_sha256'])
            for name in ['XEPHYR_BUILDINFO','XEPHYR_PKGINFO','XEPHYR_COPYING']:self.assertTrue((output/'x11/linux-x86_64'/name).is_file())
            self.assertTrue(dependencies)
            bad=pathlib.Path(temp)/'bad';bad.write_bytes(b'wrong archive')
            with self.assertRaisesRegex(ValueError,'checksum'):packager.stage_xephyr(ROOT,output,bad)
if __name__=='__main__':unittest.main()
