"""Required pinned-report structure and byte receipt tests using retained report copies."""
from pathlib import Path
import copy,json,unittest
from pinned_iperf3 import iperf3_bytes
RAW=Path(__file__).resolve().parents[2]/'clean-FIX1/evidence'
class PinnedReports(unittest.TestCase):
 def setUp(self):
  self.client=json.loads((RAW/'iperf3-client.log').read_text());self.server=json.loads((RAW/'iperf3-server.log').read_text())
 def check_bad(self,c,s):
  with self.assertRaises((ValueError,KeyError,TypeError)):iperf3_bytes(json.dumps(c),json.dumps(s))
 def test_retained_full_reports_accept_real_byte_receipts(self):self.assertEqual(iperf3_bytes(json.dumps(self.client),json.dumps(self.server)),(262144,262144))
 def test_review_reduction_rejected(self):self.check_bad({'intervals':[{}],'end':{'sum_sent':{'bytes':262144},'sum_received':{'bytes':262144}}},{'end':{'sum_received':{'bytes':262144}}})
 def test_required_objects_arrays_and_fields_missing(self):
  paths=[('start',),('start','connected'),('start','version'),('start','cookie'),('start','timestamp'),('start','test_start'),('intervals',),('end',),('end','streams'),('end','sum_sent'),('end','sum_received')]
  for role in ['client','server']:
   for path in paths:
    with self.subTest(role=role,path=path):
     c,s=copy.deepcopy(self.client),copy.deepcopy(self.server);node=c if role=='client' else s
     for key in path[:-1]:node=node[key]
     del node[path[-1]];self.check_bad(c,s)
  for role in ['client','server']:
   for path in [('start','connected'),('intervals',),('intervals',0,'streams'),('end','streams')]:
    with self.subTest(role=role,empty=path):
     c,s=copy.deepcopy(self.client),copy.deepcopy(self.server);node=c if role=='client' else s
     for key in path[:-1]:node=node[key]
     node[path[-1]]=[];self.check_bad(c,s)
 def test_partial_or_mismatched_required_structure(self):
  changes=[('start.connected.0',{}),('start.connected.0.socket',True),('start.connected.0.local_host','192.0.2.1'),('start.connected.0.remote_port',12),('start.cookie','wrong-session'),('start.version','iperf 3.0'),('start.test_start.protocol','UDP'),('start.test_start.num_streams',2),('start.test_start.reverse',1),('start.test_start.bytes',131072),('intervals.0.streams.0',{}),('intervals.0.sum',{}),('intervals.0.streams.0.socket',999),('intervals.0.streams.0.bytes',131072),('intervals.0.streams.0.sender',None),('intervals.0.streams.0.omitted',True),('intervals.0.streams.0.seconds',0),('intervals.0.streams.0.bits_per_second',float('nan')),('intervals.0.streams.0.start',.5),('end.streams.0.sender',{}),('end.streams.0.receiver',{}),('end.streams.0.receiver.socket',999),('end.sum_received.bytes',131072),('end.sum_received.sender',None),('end.sum_received.seconds',False),('end.sum_received.bits_per_second',-1)]
  for role in ['client','server']:
   for path,value in changes:
    with self.subTest(role=role,path=path):
     c,s=copy.deepcopy(self.client),copy.deepcopy(self.server);node=c if role=='client' else s;keys=path.split('.')
     for key in keys[:-1]:node=node[int(key) if isinstance(node,list) else key]
     key=keys[-1];node[int(key) if isinstance(node,list) else key]=value;self.check_bad(c,s)
 def test_typed_report_summary_and_missing_interval_details(self):
  for role in ['client','server']:
   for path in [('intervals',0,'streams',0,'bytes'),('intervals',0,'sum','sender'),('end','streams',0,'receiver','seconds'),('end','sum_received','bits_per_second')]:
    with self.subTest(role=role,path=path):
     c,s=copy.deepcopy(self.client),copy.deepcopy(self.server);node=c if role=='client' else s
     for key in path[:-1]:node=node[key]
     del node[path[-1]];self.check_bad(c,s)
if __name__=='__main__':unittest.main(verbosity=2)
