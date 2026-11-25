import importlib.util, unittest
from pathlib import Path
spec=importlib.util.spec_from_file_location('runner',Path(__file__).with_name('check-protocols.py'));r=importlib.util.module_from_spec(spec);spec.loader.exec_module(r)
class RunnerContract(unittest.TestCase):
 def test_requires_exact_nonempty_success(self):
  ok='test x ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 9 filtered out;'
  self.assertTrue(r.accepted(0,ok,'x'))
  self.assertTrue(r.accepted(0,ok.replace('test x ... ok','test x ... observation\nok'),'x'))
  for code,text in [(1,ok),(0,ok.replace('1 passed','0 passed')),(0,ok.replace('0 ignored','1 ignored')),(0,ok.replace('test x','test y')),(0,ok+'\nfixture unavailable'),(0,ok+'\n'+ok),(0,'')]:self.assertFalse(r.accepted(code,text,'x'))
 def test_timeout_reaps_owned_group(self):
  code,text=r.run(['python3','-c','import time; time.sleep(30)'],Path.cwd(),{},0.05)
  self.assertEqual(code,124);self.assertIn('killed and reaped',text)
if __name__=='__main__':unittest.main()
