"""Adversarial disposable-corpus tests: semantics independently rejects forged self-consistent metadata."""
from pathlib import Path
import unittest,tempfile,shutil,json,importlib.util,contextlib
S=Path(__file__).parent
spec=importlib.util.spec_from_file_location('gate',S/'validate.py');gate=importlib.util.module_from_spec(spec);spec.loader.exec_module(gate)
class GateTests(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  cls.tmp=tempfile.TemporaryDirectory(prefix='v08-fix1-green-');base=Path(cls.tmp.name)
  cls.hist=base/'historical';cls.fixed=base/'fixed'
  shutil.copytree(S.parents[1]/'clean-run',cls.hist);shutil.copytree(S.parent/'evidence',cls.fixed)
 @classmethod
 def tearDownClass(cls):cls.tmp.cleanup()
 def test_valid_partial_is_explicitly_incomplete(self):
  result=gate.semantics(self.hist,self.fixed)
  self.assertEqual(result['validation_status'],'VALID_PARTIAL_EVIDENCE');self.assertFalse(result['qualification_complete']);self.assertTrue(result['browser_native'].startswith('UNAVAILABLE'))
 def test_falsified_semantics_are_rejected(self):
  mutants=[('hist','clean-iperf-results.json',lambda v:[]),('hist','clean-unix-results.json',lambda v:[]),('hist','clean-unix-results.json',lambda v:[dict(v[0],version='9.9.9'),v[1]]),('hist','clean-bundled-browser.json',lambda v:dict(v,exit_code=99)),('hist','input-image-inventory.json',lambda v:dict(v,exit_code='timeout')),('hist','runtime-image-inventory.json',lambda v:dict(v,exit_code=False)),('fixed','iperf-results.json',lambda v:[]),('fixed','iperf-results.json',lambda v:[dict(v[0],version=9),v[1]]),('fixed','iperf-results.json',lambda v:[dict(v[0],client_exit=7),v[1]]),('fixed','iperf-results.json',lambda v:[dict(v[0],actual_receiver_bytes=131072),v[1]]),('fixed','iperf-results.json',lambda v:[dict(v[0],requested_bytes=1),v[1]]),('fixed','mosh-negative-results.json',lambda v:[]),('fixed','mosh-negative-results.json',lambda v:[dict(v[0],installed_mosh_absent=False),v[1]]),('fixed','browser-minimal-outcome.json',lambda v:dict(v,version=dict(v['version'],exit_code=0))),('fixed','browser-runtime-outcome.json',lambda v:dict(v,version=dict(v['version'],exit_code=127))),('fixed','browser-runtime-outcome.json',lambda v:dict(v,native=dict(v['native'],exit_code=0))),('fixed','browser-runtime-outcome.json',lambda v:dict(v,native=dict(v['native'],timed_out=True))),('fixed','browser-runtime-outcome.json',lambda v:dict(v,qualification='PASSED')),('fixed','browser-ui-results.json',lambda v:[])]
  for group,name,mutate in mutants:
   with self.subTest(group=group,name=name):
    path=(self.hist if group=='hist' else self.fixed)/name;original=path.read_bytes()
    try:
     path.write_text(json.dumps(mutate(json.loads(original))))
     with self.assertRaises((ValueError,KeyError,TypeError)):gate.semantics(self.hist,self.fixed)
    finally:path.write_bytes(original)
 def test_missing_each_required_record_rejected(self):
  for directory,names in [(self.hist,gate.SCENARIOS),(self.fixed,gate.FIXED)]:
   for name in names:
    with self.subTest(name=name):
     path=directory/(name+'.json');original=path.read_bytes();path.unlink()
     try:
      with self.assertRaises(OSError):gate.semantics(self.hist,self.fixed)
     finally:path.write_bytes(original)
 def test_wrong_truncated_empty_raw_reports_and_ui_rejected(self):
  mutants=[(self.hist/'clean-runtime-browser-version.log',b'BROKEN VERSION PROBE'),(self.fixed/'browser-runtime-version.log',b'Chromium 0.0.0.0'),(self.fixed/'browser-minimal-version.log',b'other unrelated error'),(self.fixed/'browser-runtime-native.log',b''),(self.fixed/'browser-runtime-native.log',b'timed out'),(self.fixed/'mosh-system-ocr.txt',b'all good'),(self.fixed/'mosh-bundled-ocr.txt',b'system provider unavailable'),(self.fixed/'browser-runtime-ocr.txt',b'connected'),(self.fixed/'iperf2-client.log',b'1 bits/sec'),(self.fixed/'iperf2-server.log',(self.fixed/'iperf2-server.log').read_bytes().replace(b'262144',b'131072')),(self.fixed/'iperf3-server.log',json.dumps(dict(json.loads((self.fixed/'iperf3-server.log').read_text()),end=dict(json.loads((self.fixed/'iperf3-server.log').read_text())['end'],sum_received=dict(json.loads((self.fixed/'iperf3-server.log').read_text())['end']['sum_received'],bytes=131072)))).encode()),(self.fixed/'iperf3-server.log',b'{'),(self.fixed/'iperf3-client.log',b''),(self.hist/'clean-unix-1.31.0.log',b'')]
  for path,data in mutants:
   with self.subTest(name=path.name,data=data[:35]):
    original=path.read_bytes()
    try:
     path.write_bytes(data)
     with self.assertRaises((ValueError,KeyError,TypeError)):gate.semantics(self.hist,self.fixed)
    finally:path.write_bytes(original)
 def test_integrity_rejects_edited_file_and_omitted_index(self):
  original=(self.fixed/'iperf2-server.log').read_bytes()
  (self.fixed/'iperf2-server.log').write_bytes(b'partial')
  try:
   with self.assertRaises(ValueError):gate.integrity(gate.APP,self.fixed,S.parent/'evidence-index.json')
  finally:(self.fixed/'iperf2-server.log').write_bytes(original)
  index=json.loads((S.parent/'evidence-index.json').read_text());index['files'].pop('iperf2-server.log');indexpath=Path(self.tmp.name)/'omitted-index.json';indexpath.write_text(json.dumps(index))
  with self.assertRaises(ValueError):gate.integrity(gate.APP,self.fixed,indexpath)
if __name__=='__main__':unittest.main(verbosity=2)
