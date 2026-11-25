import importlib.util, pathlib, tempfile, unittest
P=pathlib.Path(__file__).parent
class StageTests(unittest.TestCase):
 def test_rejects_wrong_payload_without_output(self):
  spec=importlib.util.spec_from_file_location("stage",P/"stage-runtime.py"); m=importlib.util.module_from_spec(spec); spec.loader.exec_module(m)
  with tempfile.TemporaryDirectory() as tmp:
   root=pathlib.Path(tmp); bad=root/"bad";bad.write_bytes(b"wrong")
   with self.assertRaisesRegex(ValueError,"checksum"):
    m.stage(P/"runtime-manifest.json",root/"output",bad,bad)
   self.assertFalse((root/"output").exists())
if __name__=="__main__": unittest.main()
