"""Offline browser closure and packaging contracts; native app stubs are layout fixtures."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
ROOT = Path(__file__).resolve().parents[1]
INPUTS = Path(os.environ['TERMCRAFT_TEST_BROWSER_INPUTS']) if 'TERMCRAFT_TEST_BROWSER_INPUTS' in os.environ else None

def module(path):
    spec = importlib.util.spec_from_file_location('browser_package_test', path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result

@unittest.skipUnless(INPUTS and INPUTS.is_dir(), 'Requires explicitly supplied pinned Chromium inputs')
class BrowserRelease(unittest.TestCase):
    def test_real_closure_in_archive_and_no_overwrite(self):
        package = module(ROOT/'scripts/package-release.py')
        with tempfile.TemporaryDirectory(prefix='browser release layout ') as tmp:
            base=Path(tmp); app=base/'app'; helper=base/'helper'
            for exe in (app,helper):
                subprocess.run(['cc','-xc','-o',exe,'-'],input='int main(){return 0;}\n',text=True,check=True)
            with patch.object(package,'cargo_notices',return_value=[]):
                archive=package.package_release('linux',base/'out',app,helper,browser_inputs=INPUTS)
                before=package.sha256(archive)
                with self.assertRaises(FileExistsError):
                    package.package_release('linux',base/'out',app,helper,browser_inputs=INPUTS)
                self.assertEqual(package.sha256(archive),before)
            with tarfile.open(archive) as bundle:
                manifest=json.load(bundle.extractfile(next(m for m in bundle if m.name.endswith('/PACKAGE-MANIFEST.json'))))
                root=manifest['runtime']['managed_browser']['root']
                self.assertEqual(root,'browser/linux-x86_64')
                self.assertIn(root+'/chromium',manifest['runtime']['linked_libraries'])
                self.assertTrue(any(p.endswith('THIRD-PARTY-NOTICES.html') for p in manifest['files']))
                self.assertTrue(any(p.endswith('SOURCE-BUILDINFO') for p in manifest['files']))
                self.assertEqual(sum(p.startswith('assets/') for p in manifest['files']),6)
                self.assertEqual(len(manifest['files']),len([m for m in bundle if m.isfile()])-1)
    def test_pinned_staging_checks_inventory_tampering_and_atomic_failure(self):
        stage=module(ROOT/'packaging/browser/stage-runtime.py')
        info=json.loads((ROOT/'packaging/browser/runtime-manifest.json').read_text())
        with tempfile.TemporaryDirectory() as tmp:
            base=Path(tmp); archive=INPUTS/info['archive_name']; signature=INPUTS/(info['archive_name']+'.sig'); notices=INPUTS/'THIRD-PARTY-NOTICES.html'
            stage.stage(ROOT/'packaging/browser/runtime-manifest.json',base/'good',archive,signature,notices)
            for item in info['files']:
                data=base/'good'/info['root']/item['path']
                self.assertEqual(hashlib.sha256(data.read_bytes()).hexdigest(),item['sha256'])
            with self.assertRaises(FileExistsError):
                stage.stage(ROOT/'packaging/browser/runtime-manifest.json',base/'good',archive,signature,notices)
            bad=base/'bad';bad.write_bytes(b'bad notices')
            with self.assertRaisesRegex(ValueError,'checksum'):
                stage.stage(ROOT/'packaging/browser/runtime-manifest.json',base/'missing',archive,signature,bad)
            self.assertFalse((base/'missing').exists())

if __name__=='__main__':unittest.main()
