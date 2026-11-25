from pathlib import Path
import json, subprocess
root=Path.cwd()
def attempt(name,artifact,expect):
 out=root/'evidence'/('negative-'+name)
 cmd=['python3','scripts/check-protocols.py','--output',str(out),'--build-artifacts',str(artifact),'--build-inputs','evidence/pre-build-final-sha256.json','--case','ftp-ascii']
 r=subprocess.run(cmd,capture_output=True,text=True)
 assert r.returncode==1,(name,r.returncode,r.stdout,r.stderr)
 record=json.loads((out/'evidence.json').read_text())
 assert record['records'][0]['result']==expect and record['controlled_endpoint_checkpoint'] is False
 print(name+': rejected; exit 1; '+expect)
attempt('missing-artifact',Path('/dev/null'),'unavailable')
fake=root/'evidence/zero-test-executable'
fake.write_text('#!/bin/sh\nif [ "$1" = "--list" ]; then echo "ascii_mode_converts_only_selected_text: test"; else echo "test result: ok. 0 passed; 0 failed; 0 ignored;"; fi\n');fake.chmod(0o700)
artifact=root/'evidence/zero-test-artifact.jsonl'
artifact.write_text(json.dumps(dict(reason='compiler-artifact',manifest_path=str(root/'Cargo.toml'),executable=str(fake),profile=dict(test=True),target=dict(kind=['test'],name='ftp_ftps')))+'\n')
attempt('zero-test',artifact,'failed')
fake.write_text('#!/bin/sh\nif [ "$1" = "--list" ]; then echo "ascii_mode_converts_only_selected_text: test"; else echo "test ascii_mode_converts_only_selected_text ... fixture unavailable"; echo ok; echo "test result: ok. 1 passed; 0 failed; 0 ignored;"; fi\n')
attempt('false-success-unavailable',artifact,'unavailable')
