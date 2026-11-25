from pathlib import Path
import json,subprocess,os,signal
out=Path(__file__).resolve().parent;b=out.parents[2];pkg=json.loads((out/'payload-equivalence.json').read_text())['package'];name='termcraft-post-reset-mosh-398d';cmd=['docker','run','--rm','--name',name,'--label','termcraft.runtime-closure=398d','--user','0','--network','none','--mount','type=bind,src='+pkg+',dst=/opt/termcraft,readonly','--mount','type=bind,src='+str(out)+',dst=/evidence','--mount','type=bind,src='+str(b/'target/agent-work/p01-mosh-closure/clean-root/fixture')+',dst=/fixture,readonly','sha256:1ab61a2bb16febd0a86829a43c26849b06ca82aca31be8385880cc7a3570c79d','/usr/bin/bash','/evidence/mosh-docker.sh'];(out/'mosh-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
try:
 p=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=65);code=p.returncode;data=p.stdout
except subprocess.TimeoutExpired as e:
 subprocess.run(['docker','stop','-t','2',name],stdout=subprocess.PIPE);code='timeout';data=e.stdout or b''
(out/'mosh-run.log').write_bytes(data);(out/'mosh-result.json').write_text(json.dumps({'exit_code':code})+'\n');print(code);print(data.decode(errors='replace')[-4000:])
