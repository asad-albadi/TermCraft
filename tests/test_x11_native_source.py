import importlib.util
from pathlib import Path
import tempfile
import unittest
spec=importlib.util.spec_from_file_location('prepare_native',Path(__file__).parents[1]/'packaging/x11/native/prepare-source.py')
prepare=importlib.util.module_from_spec(spec)
spec.loader.exec_module(prepare)
class OfflineNativeSourceTests(unittest.TestCase):
    def test_wrong_offline_input_never_creates_destination(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); archive=root/'wrong.tar.gz'; archive.write_bytes(b'not pinned source'); target=root/'result'
            with self.assertRaisesRegex(ValueError,'checksum'):
                prepare.prepare('windows',archive,target)
            self.assertFalse(target.exists())
    def test_existing_destination_survives(self):
        with tempfile.TemporaryDirectory() as directory:
            root=Path(directory); target=root/'existing'; target.mkdir(); marker=target/'marker'; marker.write_text('keep')
            with self.assertRaises(FileExistsError):
                prepare.prepare('windows',root/'absent',target)
            self.assertEqual(marker.read_text(),'keep')
if __name__=='__main__':unittest.main()
