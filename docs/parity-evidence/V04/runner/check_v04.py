#!/usr/bin/env python3
"""Reproduce scoped V04 fixtures; --require-complete rejects remaining unavailable cells."""
import argparse,hashlib,json,os,re,shutil,subprocess,sys
from pathlib import Path
from owned_process import run_owned, install_interrupt_cleanup

def validate_mosh(report):
    required={'udp-rebind-loss-reordering','private-interface-migration'}
    cases=report['cases']; passed=[c for c in cases if c['status']=='passed']; unavailable=[c for c in cases if c['status']=='unavailable']
    assert {c['case'] for c in passed}==required and len(passed)==2
    assert len(cases)==5 and len(unavailable)==3 and report['status']=='partial'
    assert report['strict_native_original']==0 and report['executed_passed']==2 and report['failed']==0 and report['unavailable']==3
    for c in passed:
        assert c['owned_process_cleanup']=='passed' and c['cancel_seconds']<3
        assert c['counters']['drop']>0 and c['counters']['reordered']>0 and c['counters']['return']>0
        assert [r['value'] for r in c['journal']]==['before','during_loss','reorder_one','reorder_two','after']
        assert [r['n'] for r in c['journal']]==[1,2,3,4,5] and len({r['pid'] for r in c['journal']})==1
    return 2,len(unavailable)

def validate_serial(output):
    matches=re.findall(r'test result: ok\. (\d+) passed; (\d+) failed; (\d+) ignored; (\d+) measured; (\d+) filtered out',output)
    assert matches==[('3','0','0','0','0')], 'requires exactly three executed tests and zero ignored/filtered cases'

if __name__=='__main__':
    install_interrupt_cleanup()
    p=argparse.ArgumentParser(description=__doc__); p.add_argument('app',type=Path);p.add_argument('output',type=Path);p.add_argument('--require-complete',action='store_true');a=p.parse_args()
    app=a.app.resolve();out=a.output.resolve();out.mkdir(parents=True,exist_ok=False);here=Path(__file__).resolve().parent
    def run(args,log,env=None):
        result=run_owned(args,cwd=app,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,text=True,env=env,timeout=180)
        (out/log).write_text(result.stdout);assert result.returncode==0, (args,result.returncode,result.stdout[-2000:]); return result.stdout
    expected=json.loads((here/'serial-base.json').read_text())
    for name,h in expected.items(): assert hashlib.sha256((app/name).read_bytes()).hexdigest()==h, f'source drift: {name}'
    serial=out/'serial-source'; (serial/'src/serial').mkdir(parents=True);(serial/'src/process').mkdir();(serial/'tests/support').mkdir(parents=True)
    for f in ['src/serial.rs','src/serial/settings.rs','src/process_input.rs','tests/support/serial.rs']: shutil.copy2(app/f,serial/f)
    (serial/'Cargo.toml').write_text('[package]\nname="termcraft-v04-private"\nversion="0.1.904"\nedition="2024"\n[lib]\nname="harbor_workspace"\n[dependencies]\nlibc="0.2"\nserde={version="1.0",features=["derive"]}\n')
    (serial/'src/lib.rs').write_text('pub mod serial;\npub mod process { #[path="../process_input.rs"] mod input; pub use input::InputQueue; }\n')
    source=(app/'tests/serial_settings.rs').read_text();start=source.index('#[cfg(unix)]\n#[path = "support/serial.rs"]');(serial/'tests/serial_pty.rs').write_text('use harbor_workspace::serial::SerialSettings;\n'+source[start:])
    os.utime(serial/'src/lib.rs',None)
    env={**os.environ,'CARGO_BUILD_JOBS':'2','CARGO_TARGET_DIR':str(app/'target')}
    output=run(['cargo','test','--offline','--manifest-path',str(serial/'Cargo.toml'),'--test','serial_pty','--','--test-threads=1'],'serial.log',env);validate_serial(output)
    # Pin exact emitted test executable and selected input sources, rather than attributing results to a guessed executable.
    exe=re.search(r'Running tests/serial_pty.rs \(([^)]+)\)',output);assert exe
    binary=Path(exe[1]);binary=binary if binary.is_absolute() else app/binary
    provenance={'source_base':expected,'serial_test_executable':str(binary),'serial_test_executable_sha256':hashlib.sha256(binary.read_bytes()).hexdigest(),'cargo_lock_sha256':hashlib.sha256((serial/'Cargo.lock').read_bytes()).hexdigest(),'rustc':run(['rustc','-Vv'],'rustc.log')}
    (out/'serial-provenance.json').write_text(json.dumps(provenance,indent=2))
    env={**os.environ,'V04_HOST_NETNS':os.readlink('/proc/self/ns/net')}
    run(['unshare','--user','--map-root-user','--net',sys.executable,str(here/'run_mosh_roaming.py'),str(app),str(out/'mosh')],'mosh.log',env)
    report=json.loads((out/'mosh/results.json').read_text());passed,unavailable=validate_mosh(report)
    # Validator negative controls: unavailable masquerading as passed and empty/ignored serial runs must fail.
    bad=json.loads(json.dumps(report)); bad['cases'][-1]['status']='passed'
    try: validate_mosh(bad)
    except AssertionError: pass
    else: raise AssertionError('validator accepted unavailable-as-pass')
    for bad in ['test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 3 filtered out','test result: ok. 3 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out']:
        try: validate_serial(bad)
        except AssertionError: pass
        else: raise AssertionError('validator accepted empty/ignored run')
    summary={'status':'partial','scoped_mosh_passed':passed,'serial_pty_passed':3,'unavailable':unavailable,'strict_native_original':0,'validator_negative_controls':3,'whole_v04_complete':False}
    (out/'summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary))
    if a.require_complete: sys.exit(2)
