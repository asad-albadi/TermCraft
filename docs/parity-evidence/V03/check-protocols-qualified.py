#!/usr/bin/env python3
"""V03 controlled endpoint matrix, schema 1. Nonzero on failed OR unavailable cells."""
import argparse, datetime, hashlib, json, os, platform, re, shlex, signal, subprocess, shutil
from pathlib import Path


def digest(path):
    h=hashlib.sha256()
    with Path(path).open('rb') as f:
        for block in iter(lambda:f.read(1024*1024),b''): h.update(block)
    return h.hexdigest()


def accepted(code, output, test):
    return (code == 0 and not re.search(r'\b(unavailable|skipped)\b', output, re.I)
        and len(re.findall(r'test result:', output)) == 1
        and re.search(r'test result: ok\. 1 passed; 0 failed; 0 ignored;',output) is not None
        and re.search(r'^test '+re.escape(test)+r' \.\.\. (?:ok$|[^\n]*\n(?:[\s\S]*?\n)?ok$)',output,re.M) is not None)


def run(command, root, env, timeout):
    try: proc=subprocess.Popen(command,cwd=root,env=env,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,start_new_session=True)
    except OSError as error: return 127, 'UNAVAILABLE: executable could not start: '+str(error)+'\n'
    try: text,_=proc.communicate(timeout=timeout);return proc.returncode,text
    except subprocess.TimeoutExpired:
        os.killpg(proc.pid,signal.SIGKILL)
        text,_=proc.communicate()
        return 124,text+'\nTIMEOUT: owned process group killed and reaped\n'


def inventory(root):
    files=[]
    for directory in ['src','tests','assets','vendor','packaging','inputs','scripts']:
        files.extend(p for p in (root/directory).rglob('*') if p.is_file() and '__pycache__' not in str(p))
    files += [root/'Cargo.toml',root/'Cargo.lock',root/'FEATURE_PARITY.md',root/'protocol-matrix.json']
    return {str(p.relative_to(root)):digest(p) for p in sorted(files)}


def main():
    p=argparse.ArgumentParser(__doc__)
    p.add_argument('--output',required=True,type=Path)
    p.add_argument('--build-artifacts',required=True,type=Path,help='cargo --message-format=json no-run output from this exact private source')
    p.add_argument('--build-inputs',required=True,type=Path,help='pre-build compiled-source SHA256 manifest')
    p.add_argument('--case',action='append',default=[])
    args=p.parse_args();root=Path(__file__).resolve().parent.parent
    matrix=json.loads((root/'protocol-matrix.json').read_text())
    cases=matrix['cases'];ids=[c['id'] for c in cases]
    if not cases or len(set(ids))!=len(ids):p.error('empty matrix or duplicate case ids')
    if set(args.case)-set(ids):p.error('unknown case')
    selected=[c for c in cases if not args.case or c['id'] in args.case]
    if not selected:p.error('zero cases selected')
    build_inputs=json.loads(args.build_inputs.read_text())
    if not build_inputs or any(not (root/f).is_file() or digest(root/f)!=sha for f,sha in build_inputs.items()):p.error('compiled source differs from pre-build manifest')
    artifacts={}
    for line in args.build_artifacts.read_text().splitlines():
        try:m=json.loads(line)
        except ValueError:continue
        if m.get('manifest_path') == str(root/'Cargo.toml') and m.get('reason')=='compiler-artifact' and m.get('executable') and 'test' in m.get('profile',{}) and m['profile']['test']:
            # Lib test and endpoint test executables only; never execute the GUI.
            if m['target']['kind']==['lib']:artifacts['lib']=m['executable']
            elif m['target']['kind']==['test']:artifacts[m['target']['name']]=m['executable']
    args.output.mkdir(parents=True,exist_ok=False)
    before=inventory(root)
    (args.output/'source-input-sha256.json').write_text(json.dumps(before,indent=2)+'\n')
    env=dict(os.environ,CARGO_BUILD_JOBS='2',TERMCRAFT_V03_RDP_ENGINE=str(root/'inputs/termcraft-rdp-engine'),TERMCRAFT_V03_RDP_FIXTURE=str(root/'inputs/termcraft-rdp-engine-evaluation'),TERMCRAFT_TEST_XDMCP_PACKAGE=str(root/'inputs/xdmcp-engine'),TERMCRAFT_TEST_XDMCP_FIXTURE=str(root/'inputs/xdmcp-fixture'))
    for key in ['DISPLAY','WAYLAND_DISPLAY','WAYLAND_SOCKET','XAUTHORITY','SSH_AUTH_SOCK']:env.pop(key,None)
    records=[]
    for case in selected:
        exe=Path(artifacts.get(case['target'],'/nonexistent-v03-executable'));command=[];code=None;exe_sha=None
        if not exe.is_file():status='unavailable';text='Missing compiled executable; zero tests are never a pass.\n'
        elif any(not (root/f).is_file() for f in case['inputs']):status='unavailable';text='Missing declared fixture artifact.\n'
        elif any(shutil.which(tool,path=env.get('PATH')) is None for tool in case.get('requires',[])):status='unavailable';text='Missing declared isolated fixture tool.\n'
        else:
            exe_sha=digest(exe)
            listing_code,listing=run([str(exe),'--list','--format=terse'],root,env,30)
            if listing_code or case['test']+': test' not in listing.splitlines():
                status='failed';text='Exact test name absent from executable.\n'+listing
            else:
                command=[str(exe),'--exact',case['test'],'--nocapture','--test-threads=1']
                if case.get('ignored'):command.append('--ignored')
                if case.get('reserved_namespace'):command=[str(root/'tests/support/run_protocol_reserved_isolated.sh')]+command
                if case.get('isolated_xdmcp'):command=[str(root/'tests/support/run_xdmcp_isolated.sh'),str(root/'inputs/compositor')]+command
                code,text=run(command,root,env,180)
                status='unavailable' if re.search(r'\b(unavailable|skipped)\b',text,re.I) else ('passed' if accepted(code,text,case['test']) else 'failed')
                if digest(exe)!=exe_sha:status='failed';text+='\nExecutable changed while executing.\n'
        log=args.output/(case['id']+'.log');log.write_text(shlex.join(command)+'\n'+text)
        record=dict(case=case,command=command,cwd=str(root),executable=str(exe),executable_sha256=exe_sha,exit_code=code,result=status,log=log.name,log_sha256=digest(log),source_input_inventory='source-input-sha256.json')
        records.append(record);print(case['id']+': '+status,flush=True)
    after=inventory(root);stable=before==after
    result=dict(schema=1,point='V03',capture_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),platform=platform.platform(),matrix_sha256=digest(root/'protocol-matrix.json'),build_artifacts_sha256=digest(args.build_artifacts),build_inputs_sha256=digest(args.build_inputs),source_inputs_stable=stable,source_input_inventory_sha256=digest(args.output/'source-input-sha256.json'),complete_matrix=len(selected)==len(cases),native_original_validated=0,records=records)
    result['controlled_endpoint_checkpoint']=stable and result['complete_matrix'] and all(c['result']=='passed' for c in records)
    (args.output/'evidence.json').write_text(json.dumps(result,indent=2)+'\n')
    return 0 if stable and all(c['result']=='passed' for c in records) else 1

if __name__=='__main__':raise SystemExit(main())
