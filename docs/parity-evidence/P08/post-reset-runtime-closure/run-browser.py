from pathlib import Path
import subprocess,json,time,os,signal
out=Path(__file__).resolve().parent;b=out.parents[2]
record=json.loads((b/'docs/parity-evidence/parallel-integration-local-completion-serial-release/package-qualification/command.json').read_text());cmd=record['command'];idx=cmd.index('--ro-bind',cmd.index('/opt/termcraft'));del cmd[idx:idx+3];idx=cmd.index('--bind');cmd[idx+1]=str(out);cmd[-2:] = ['/usr/bin/bash','/evidence/browser-current.sh'];cmd[cmd.index('/opt/termcraft')-1]=json.loads((out/'payload-equivalence.json').read_text())['package'];record['command']=cmd
(out/'browser-current-command.json').write_text(json.dumps(record,indent=2)+'\n')
p=subprocess.Popen(cmd,stdout=(out/'browser-current.log').open('wb'),stderr=subprocess.STDOUT,start_new_session=True)
try: result=p.wait(timeout=70)
except subprocess.TimeoutExpired: os.killpg(p.pid,signal.SIGTERM);result=p.wait(timeout=10)
(out/'browser-current-result.json').write_text(json.dumps({'exit_code':result})+'\n');print(result);print((out/'browser-current.log').read_text()[-2500:])
