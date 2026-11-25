#!/usr/bin/env python3
"""FIX2 required pinned-report structure and byte receipts overlay; valid partial=0, strict incomplete=2."""
from pathlib import Path
import importlib.util,sys,json,hashlib
sys.dont_write_bytecode=True
from pinned_iperf3 import iperf3_bytes
HERE=Path(__file__).resolve().parent
# Compare a separately retained predecessor seal; never rewrite predecessor evidence/index/source.
seal=json.loads((HERE.parent/'predecessor-index.json').read_text())
for name,expected in seal['files'].items():
 actual=hashlib.sha256((HERE.parents[1]/name).read_bytes()).hexdigest()
 if actual!=expected:raise SystemExit('FIX1 predecessor changed: '+name)
spec=importlib.util.spec_from_file_location('fix1_gate',HERE.parents[1]/'clean-FIX1/source/validate.py')
gate=importlib.util.module_from_spec(spec);spec.loader.exec_module(gate)
gate.iperf3_bytes=iperf3_bytes
original_semantics=gate.semantics
def scoped_semantics(historical,fixed):
 result=original_semantics(historical,fixed)
 result['iperf3_semantic_scope']='Required pinned iperf3 3.19.1 single forward TCP client/server report structure and byte receipts; not exhaustive Iperf JSON schema'
 return result
gate.semantics=scoped_semantics
if __name__=='__main__':sys.exit(gate.main())
