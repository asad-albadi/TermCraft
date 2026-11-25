#!/usr/bin/env python3
"""Fail-closed partial-evidence gate. Diagnostic valid-partial=0; strict incomplete=2; invalid=1."""
from pathlib import Path
import argparse,csv,hashlib,json,sys
HERE=Path(__file__).resolve().parent
APP=HERE.parents[4]
HISTORICAL_INDEX_SHA256='3bd2371c13d7593cd1f81f85ee07d23ff707f7c32d85897af46dc2a0c3b1c0f4'
SCENARIOS=('fresh-install-save-restart-pty','legacy-save','upgrade-current','corrupt-save-restart-final','absent-installed-mosh','absent-bundled-mosh-dependency','direct-iperf-byte-closure','direct-unix-byte-closure-exec-tmp','per-user-install-startup-uninstall','clean-bundled-browser','input-image-inventory','clean-declared-browser-runtime','runtime-image-inventory')
FIXED=('iperf-actual-bytes','mosh-negative-semantic','browser-minimal','browser-runtime','browser-ocr','cleanup-guard')
VERSION='Chromium 158.0.8127.88'
def need(value,message):
 if not value: raise ValueError(message)
def load(path):return json.loads(path.read_text())
def digest(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def integer(value,expected,label):need(type(value) is int and value==expected,label)
def exact_rows(rows,field,expected,label):
 need(type(rows) is list and len(rows)==len(expected),label+' count')
 need(all(type(x) is dict and field in x for x in rows),label+' schema')
 need({x[field] for x in rows}==set(expected) and len({x[field] for x in rows})==len(rows),label+' versions')
 return {x[field]:x for x in rows}
def records(directory,names):
 for name in names:
  r=load(directory/(name+'.json'));integer(r.get('exit_code'),0,name+' scenario exit');need(r.get('owned_container_removed') is True,name+' cleanup')
def iperf2_bytes(raw):
 rows=list(csv.reader(raw.splitlines()));need(len(rows)==1,'iperf2 CSV row count')
 r=rows[0];need(len(r)==9,'iperf2 CSV field count');need(r[1]==r[3]=='127.0.0.1','iperf2 endpoints')
 need(r[2].isdigit() and r[4].isdigit(),'iperf2 ports');need(len(r[6].split('-'))==2,'iperf2 interval')
 start,end=map(float,r[6].split('-'));need(start==0 and end>=0,'iperf2 complete interval')
 need(r[7].isdigit(),'iperf2 actual bytes');return int(r[7])
def iperf3_bytes(client,server):
 c=json.loads(client);s=json.loads(server)
 need('error' not in c and 'error' not in s,'iperf3 error')
 need(type(c.get('intervals')) is list and len(c['intervals'])>0,'iperf3 intervals')
 sent=c['end']['sum_sent']['bytes'];received=s['end']['sum_received']['bytes']
 integer(c['end']['sum_received']['bytes'],received,'iperf3 client/server receipt')
 integer(sent,262144,'iperf3 sender');integer(received,262144,'iperf3 receiver')
 return sent,received
def semantics(historical,fixed):
 records(historical,SCENARIOS);records(fixed,FIXED)
 old_iperf=exact_rows(load(historical/'clean-iperf-results.json'),'version',[2,3],'historical Iperf')
 for v,r in old_iperf.items():integer(r.get('client_exit'),0,'historical Iperf exit');integer(r.get('bytes_requested'),262144,'historical requested bytes')
 unix=exact_rows(load(historical/'clean-unix-results.json'),'version',['1.31.0','1.35.0'],'Unix')
 for version,r in unix.items():
  integer(r.get('exit'),0,'Unix exit')
  text=(historical/f'clean-unix-{version}.log').read_text();need('UNIX_REAL_PTY_'+version in text and 'ALPHABETA' in text,'Unix real PTY semantic output')
 need((historical/'clean-runtime-browser-version.log').read_text().strip()==VERSION,'historical browser version')
 iperf=exact_rows(load(fixed/'iperf-results.json'),'version',[2,3],'Iperf')
 for version,r in iperf.items():
  integer(r.get('client_exit'),0,'Iperf exit');integer(r.get('requested_bytes'),262144,'Iperf requested')
  need(r.get('client_report')==f'iperf{version}-client.log' and r.get('server_report')==f'iperf{version}-server.log','Iperf report provenance')
  c=(fixed/r['client_report']).read_text();s=(fixed/r['server_report']).read_text()
  sent,received=(iperf2_bytes(c),iperf2_bytes(s)) if version==2 else iperf3_bytes(c,s)
  integer(sent,262144,'actual sender bytes');integer(received,262144,'actual receiver bytes')
  integer(r.get('actual_sender_bytes'),sent,'recorded actual sender');integer(r.get('actual_receiver_bytes'),received,'recorded actual receiver')
 mosh=exact_rows(load(fixed/'mosh-negative-results.json'),'provider',['system','bundled'],'Mosh negative')
 messages={'system':'The selected installed Mosh provider is unavailable. Install the required system program.','bundled':'Bundled Mosh requires system Perl at /usr/bin/perl'}
 for provider,r in mosh.items():
  need(r.get('status')=='EXPECTED_ERROR_OBSERVED' and r.get('required_error')==messages[provider],'Mosh error status')
  need(r.get('installed_mosh_absent') is True and r.get('perl_absent') is True,'Mosh absent preconditions')
  need(r.get('ocr_file')==f'mosh-{provider}-ocr.txt' and r.get('screenshot')==f'mosh-{provider}.png','Mosh OCR provenance')
  text=' '.join((fixed/r['ocr_file']).read_text().split()).lower();need(messages[provider].lower() in text,'Mosh actual UI error text')
 ui_rows=exact_rows(load(fixed/'browser-ui-results.json'),'mode',['minimal','runtime'],'browser UI')
 for mode in ['minimal','runtime']:
  ui_row=ui_rows[mode];need(ui_row.get('actual_ui_error_observed') is True and ui_row.get('screenshot')==f'browser-{mode}.png' and ui_row.get('ocr')==f'browser-{mode}-ocr.txt','browser UI record')
  r=load(fixed/f'browser-{mode}-outcome.json');need(r.get('mode')==mode,'browser mode');v=r['version'];need(v.get('timed_out') is False,'browser version timeout')
  need(v.get('log')==f'browser-{mode}-version.log','browser version provenance');text=(fixed/v['log']).read_text()
  ui=' '.join((fixed/f'browser-{mode}-ocr.txt').read_text().split()).lower()
  need('the chromium process exited. check the installed browser and sandbox configuration.' in ui,'browser actual UI failure')
  if mode=='minimal':
   integer(v.get('exit_code'),127,'minimal loader exit');need('libopenh264.so.8' in text and 'cannot open shared object file' in text,'minimal loader error')
   need(r.get('status')=='EXPECTED_DEPENDENCY_ERROR' and r.get('qualification')=='NEGATIVE_ONLY','minimal browser status')
  else:
   integer(v.get('exit_code'),0,'runtime browser version exit');need(text.strip()==VERSION,'runtime browser version')
   n=r['native'];integer(n.get('exit_code'),-6,'native sandbox exit');need(n.get('timed_out') is False,'native sandbox timeout')
   need(n.get('log')=='browser-runtime-native.log','sandbox log provenance');raw=(fixed/n['log']).read_text()
   need('SUID sandbox helper binary was found, but is not configured correctly' in raw and 'mode 4755' in raw,'actual sandbox error')
   need(r.get('status')=='SANDBOX_BLOCKED' and r.get('qualification')=='UNAVAILABLE','sandbox unavailable classification')
   ns=r['user_namespace'];integer(ns.get('return_code'),-1,'namespace return');integer(ns.get('errno'),1,'namespace denied')
 return {'validation_status':'VALID_PARTIAL_EVIDENCE','qualification_complete':False,'iperf_actual_bytes':{str(v):{'sender':r['actual_sender_bytes'],'receiver':r['actual_receiver_bytes']} for v,r in iperf.items()},'browser_native':'UNAVAILABLE: SANDBOX_BLOCKED','remaining_gaps':['successful native browser sandbox','credential vault upgrade contents','native Windows/macOS','physical supported Linux distro/install/upgrade matrix','complete managed API/runtime and original parity','signing/notarization']}
def integrity(app,fixed,index_path):
 historical=HERE.parent/'historical-index.json';need(digest(historical)==HISTORICAL_INDEX_SHA256,'historical index identity')
 need(digest(app/'docs/parity-evidence/V08/clean-observations.json')==HISTORICAL_INDEX_SHA256,'original historical inventory changed')
 index=load(historical);need(len(index['evidence_hashes'])==134,'historical134 count')
 for name,sha in index['evidence_hashes'].items():need(digest(app/name)==sha,'historical evidence changed: '+name)
 for p in index['packages']:
  need(digest(app/p['archive'])==p['archive_sha256'],'immutable archive')
  package=app/p['extraction'];need(digest(package/'PACKAGE-MANIFEST.json')==p['manifest_sha256'],'immutable manifest')
  manifest=load(package/'PACKAGE-MANIFEST.json');actual={x.relative_to(package).as_posix():digest(x) for x in package.rglob('*') if x.is_file() and x.name!='PACKAGE-MANIFEST.json'}
  need(actual==manifest['files'],'immutable extraction')
 golden=load(index_path);need(golden.get('schema_version')==1 and type(golden.get('files')) is dict and len(golden['files'])>0,'FIX1 sealed evidence index')
 need(set(golden['files'])=={p.name for p in fixed.iterdir() if p.is_file()},'FIX1 index omissions or additions')
 for name,sha in golden['files'].items():
  need(Path(name).name==name,'FIX1 index path');need(digest(fixed/name)==sha,'FIX1 evidence changed: '+name)
def main():
 p=argparse.ArgumentParser(description=__doc__);p.add_argument('--app-root',type=Path,default=APP);p.add_argument('--evidence',type=Path,default=HERE.parent/'evidence');p.add_argument('--index',type=Path,default=HERE.parent/'evidence-index.json');p.add_argument('--output',type=Path);p.add_argument('--require-complete',action='store_true');a=p.parse_args()
 try:
  integrity(a.app_root,a.evidence,a.index)
  result=semantics(a.app_root/'docs/parity-evidence/V08/clean-run',a.evidence)
  result['historical_index_sha256']=HISTORICAL_INDEX_SHA256;result['fixed_index_sha256']=digest(a.index)
  if a.output:a.output.write_text(json.dumps(result,indent=2)+'\n')
  print(json.dumps(result,indent=2));return 2 if a.require_complete and not result['qualification_complete'] else 0
 except (ValueError,OSError,KeyError,TypeError,IndexError) as error:
  print(json.dumps({'validation_status':'INVALID_EVIDENCE','error':str(error)}));return 1
if __name__=='__main__':sys.exit(main())
