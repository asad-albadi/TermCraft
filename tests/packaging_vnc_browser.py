"""Real pinned noVNC source inventory, release source freeze, and offline notices."""
import importlib.util, json, pathlib, shutil, tempfile, unittest
ROOT=pathlib.Path(__file__).resolve().parents[1]
spec=importlib.util.spec_from_file_location('vnc_browser_package',ROOT/'scripts/package-release.py')
packager=importlib.util.module_from_spec(spec);spec.loader.exec_module(packager)
class VncBrowserPackage(unittest.TestCase):
    def test_pinned_source_notices_and_changed_viewer_source_are_hashed(self):
        with tempfile.TemporaryDirectory() as tmp:
            resources=pathlib.Path(tmp)/'resources';resources.mkdir()
            info=packager.stage_vnc_browser(ROOT,resources)
            self.assertTrue(info['embedded_runtime']);self.assertEqual(info['release'],'v1.7.0')
            staged=resources/info['source_and_notices']
            for rel in ['LICENSE.txt','AUTHORS','vendor/pako/LICENSE','core/rfb.js','UPSTREAM.json','SHA256SUMS.json']:
                self.assertEqual((staged/rel).read_bytes(),(ROOT/'vendor/noVNC'/rel).read_bytes())
            with self.assertRaises(FileExistsError):packager.stage_vnc_browser(ROOT,resources)
            inputs=packager.source_inputs(ROOT)
            for rel in ['assets/vnc-browser/index.html','assets/vnc-browser/viewer.js','assets/vnc-browser/viewer.css','vendor/noVNC/core/rfb.js']:
                self.assertEqual(inputs[rel],packager.sha256(ROOT/rel))
    def test_tampered_pinned_source_refused_before_staging(self):
        with tempfile.TemporaryDirectory() as tmp:
            root=pathlib.Path(tmp)/'source';shutil.copytree(ROOT/'vendor/noVNC',root/'vendor/noVNC')
            (root/'vendor/noVNC/core/rfb.js').write_text('tampered')
            output=pathlib.Path(tmp)/'resources';output.mkdir()
            with self.assertRaisesRegex(ValueError,'checksum mismatch'):packager.stage_vnc_browser(root,output)
            self.assertFalse((output/'build-source').exists())
if __name__=='__main__':unittest.main()
