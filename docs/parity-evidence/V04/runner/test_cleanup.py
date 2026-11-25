#!/usr/bin/env python3
"""Focused FIX1 fault probes: no Mosh session, hardware, Cargo or host network changes."""
import ast
import ctypes
import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time
import types
from owned_process import OwnedChildren, run_owned

HERE=Path(__file__).resolve().parent
ctypes.CDLL(None).prctl(36,1,0,0,0)
records=[]
probe_ownership=OwnedChildren()
try:
    # A pre-existing unrelated owned child must remain outside every scope.
    sentinel=subprocess.Popen([sys.executable,'-c','import time;time.sleep(60)'],start_new_session=True)


    def absent(pids):
        assert all(not Path('/proc',str(pid)).exists() for pid in pids), pids
        for pid in pids:
            try: os.waitpid(pid,os.WNOHANG)
            except ChildProcessError: continue
            raise AssertionError(('child was not reaped',pid))
        assert sentinel.poll() is None, 'cleanup touched a pre-existing child'


    with tempfile.TemporaryDirectory(prefix='v04-fix1-') as tmp:
        work=Path(tmp)
        tree=ast.parse((HERE/'run_mosh_roaming.py').read_text()); function=next(n for n in tree.body if isinstance(n,ast.FunctionDef) and n.name=='run_case')
        for mode in ['launcher-timeout-before-owner','launcher-exits-before-owner-parse','popen-startup-failure']:
            launched=[]; pidfile=work/(mode+'.pid')
            def popen(*args,**kwargs):
                if mode=='popen-startup-failure': raise OSError('injected Popen failure')
                if mode=='launcher-timeout-before-owner':
                    p=subprocess.Popen([sys.executable,'-c','import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(60)'],start_new_session=True);launched.append(p.pid)
                    def wait(timeout=None): raise subprocess.TimeoutExpired('injected launcher wait',.02)
                    p.wait=wait;return p
                script='import subprocess,sys; p=subprocess.Popen([sys.executable,"-c","import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(60)"],start_new_session=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);open(sys.argv[1],"w").write(str(p.pid))'
                p=subprocess.Popen([sys.executable,'-c',script,str(pidfile)],start_new_session=True);launched.append(p.pid);return p
            class Sock:
                def close(self): pass
            ns=dict(work=work,OUT=work,child=work/'dummy',cmd=lambda *args:'[]',sock=lambda *args:Sock(),subprocess=types.SimpleNamespace(Popen=popen),sys=sys,os=os,time=time,re=re,signal=signal,Path=Path,OwnedChildren=OwnedChildren)
            exec(compile(ast.Module(body=[function],type_ignores=[]),'exact-candidate-run_case','exec'),ns)
            start=time.monotonic()
            try: ns['run_case'](mode)
            except (subprocess.TimeoutExpired,AssertionError,OSError) as e: error=type(e).__name__
            else: raise AssertionError('fault falsely returned success')
            if pidfile.exists():launched.append(int(pidfile.read_text()))
            absent(launched);elapsed=time.monotonic()-start;assert elapsed<4
            records.append({'case':mode,'status':'passed','original_failure_preserved':error,'elapsed_seconds':elapsed,'pids_absent_and_reaped':launched})

        for mode in ['outer-timeout-namespace-detached-descendant','outer-nonzero-detached-descendant']:
            pidfile=work/(mode+'.pid'); rootfile=work/(mode+'.root')
            script='import subprocess,sys,time,os;open(sys.argv[2],"w").write(str(os.getpid()));p=subprocess.Popen([sys.executable,"-c","import signal,time;signal.signal(signal.SIGTERM,signal.SIG_IGN);time.sleep(60)"],start_new_session=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);open(sys.argv[1],"w").write(str(p.pid));'+('time.sleep(60)' if 'timeout' in mode else 'sys.exit(7)')
            args=[sys.executable,'-c',script,str(pidfile),str(rootfile)]
            if 'namespace' in mode: args=['unshare','--user','--map-root-user','--net']+args
            start=time.monotonic()
            try:
                result=run_owned(args,timeout=.3,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
                assert mode=='outer-nonzero-detached-descendant' and result.returncode==7
                error='exit 7'
            except subprocess.TimeoutExpired:
                assert 'timeout' in mode;error='TimeoutExpired'
            pids=[int(rootfile.read_text()),int(pidfile.read_text())];absent(pids);elapsed=time.monotonic()-start;assert elapsed<4
            records.append({'case':mode,'status':'passed','original_failure_preserved':error,'elapsed_seconds':elapsed,'pids_absent_and_reaped':pids})

        # Interrupt the actual outer ownership helper while a namespace child tree runs.
        pidfile=work/'interrupt.pid';rootfile=work/'interrupt.root'
        worker=work/'interrupt.py';worker.write_text('import sys,threading,os,signal,subprocess\nsys.path.insert(0,sys.argv[1])\nfrom owned_process import run_owned,install_interrupt_cleanup\ninstall_interrupt_cleanup()\nthreading.Timer(.4,lambda:os.kill(os.getpid(),signal.SIGTERM)).start()\nrun_owned(sys.argv[2:],timeout=60,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)\n')
        script='import subprocess,sys,time,os;open(sys.argv[2],"w").write(str(os.getpid()));p=subprocess.Popen([sys.executable,"-c","import time;time.sleep(60)"],start_new_session=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL);open(sys.argv[1],"w").write(str(p.pid));time.sleep(60)'
        result=run_owned([sys.executable,str(worker),str(HERE),'unshare','--user','--map-root-user','--net',sys.executable,'-c',script,str(pidfile),str(rootfile)],timeout=5,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True)
        assert result.returncode==143,(result.returncode,result.stdout);pids=[int(rootfile.read_text()),int(pidfile.read_text())];absent(pids)
        records.append({'case':'outer-SIGTERM-stack-unwind','status':'passed','preserved_exit':143,'pids_absent_and_reaped':pids})

    result=run_owned([sys.executable,'-c','import sys;sys.stdout.buffer.write(sys.stdin.buffer.read())'],input=b'v04-input-roundtrip',timeout=2,stdout=subprocess.PIPE)
    assert result.returncode==0 and result.stdout==b'v04-input-roundtrip'
    records.append({'case':'owned-helper-stdin-success','status':'passed','payload_exact':True})
    os.kill(sentinel.pid,signal.SIGTERM);sentinel.wait(timeout=2)
    assert len(records)==7
    print(json.dumps({'status':'GREEN','cases':records,'preexisting_unrelated_child_preserved':True,'probe_processes_reaped':True,'scope':'FIX1 cleanup-only fault injection; original Mosh/serial observation records unchanged'},indent=2))
finally:
    probe_ownership.cleanup()
