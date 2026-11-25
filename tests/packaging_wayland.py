"""Focused optional-helper packaging checks, using private stub app/RDP executables."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('package_release',ROOT/'scripts/package-release.py')
package=importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)
@unittest.skipUnless("E03_HELPER" in os.environ, "Set E03_HELPER to explicitly qualify the optional native Wayland helper.")
class OptionalWayland(unittest.TestCase):
    def test_optional_helper_stages_hash_source_lock_notices_and_native_requirements(self):
        real=Path(os.environ['E03_HELPER']).resolve()
        with tempfile.TemporaryDirectory() as temporary:
            base=Path(temporary)
            app=base/'app';rdp=base/'rdp'
            for binary in (app,rdp):
                subprocess.run(['cc','-xc','-o',binary,'-'],input='int main(void){return 0;}\n',text=True,check=True)
            with patch.object(package,'cargo_notices',return_value={}) as notices:
                archive=package.package_release('linux',base/'selected',app,rdp,source=ROOT,cargo_settings={},wayland_helper_binary=real)
            self.assertEqual(notices.call_args.args[-1],(Path('engines/wayland/Cargo.toml'),))
            with tarfile.open(archive) as stream:
                manifest=json.load(stream.extractfile(next(m for m in stream.getmembers() if m.name.endswith('/PACKAGE-MANIFEST.json'))))
                root=next(m.name for m in stream.getmembers() if m.isdir() and '/' not in m.name)
                helper=stream.extractfile(root+'/termcraft-wayland-engine').read()
                self.assertEqual(hashlib.sha256(helper).hexdigest(),manifest['files']['termcraft-wayland-engine'])
                self.assertIn('wayland_helper_version',manifest)
                self.assertEqual(manifest['runtime']['managed_vnc_browser']['release'],'v1.7.0')
                self.assertTrue(stream.getmember(root+'/build-source/noVNC/core/rfb.js').isfile())
                self.assertIn('libpipewire-0.3.so.0',manifest['runtime']['linked_libraries']['termcraft-wayland-engine']['libraries'])
                self.assertIn('explicit prebuilt',manifest['runtime']['managed_wayland_sharing']['provenance'])
                self.assertTrue(stream.getmember(root+'/build-source/engines/wayland/src/capture.rs').isfile())
                self.assertTrue(stream.getmember(root+'/build-source/engines/wayland/Cargo.lock').isfile())
                self.assertTrue(stream.getmember(root+'/licenses/wayland/README.md').isfile())
            with patch.object(package,'cargo_notices',return_value={}) as notices:
                archive=package.package_release('linux',base/'ordinary',app,rdp,source=ROOT,cargo_settings={})
            self.assertEqual(notices.call_args.args[-1],())
            with tarfile.open(archive) as stream:
                manifest=json.load(stream.extractfile(next(m for m in stream.getmembers() if m.name.endswith('/PACKAGE-MANIFEST.json'))))
                self.assertNotIn('wayland_helper_version',manifest)
                self.assertNotIn('termcraft-wayland-engine',manifest['files'])
                for binary in manifest['runtime']['linked_libraries'].values():
                    self.assertNotIn('libpipewire-0.3.so.0',binary['libraries'])
            inputs=package.source_inputs(ROOT)
            self.assertIn('engines/wayland/src/capture.rs',inputs)
            self.assertIn('engines/wayland/Cargo.lock',inputs)
            self.assertIn('assets/vnc-browser/index.html',inputs)
            self.assertIn('vendor/objc2-screen-capture-kit/src/lib.rs',inputs)
    def test_rejects_app_or_rdp_as_wayland_helper(self):
        real=Path(os.environ['E03_HELPER']).resolve()
        with self.assertRaisesRegex(ValueError,'distinct'):
            package.package_release('linux',ROOT/'target-unused',real,Path('/bin/true'),source=ROOT,cargo_settings={},wayland_helper_binary=real)
if __name__=='__main__':unittest.main()
