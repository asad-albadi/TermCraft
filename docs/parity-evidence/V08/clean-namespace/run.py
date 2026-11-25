#!/usr/bin/env python3
import hashlib,json,os,subprocess,time,sys
from pathlib import Path
base=Path(__file__).resolve().parents[4]; out=Path(__file__).resolve().parent
scratch=base/'target/validation/v08-clean-namespace'; root=scratch/'rootfs'
pkg=next((base/'target/package-games-v03-qualified').glob('68f356*/*'))
stem=sys.argv[1] if len(sys.argv)>1 else 'probe'
if stem not in ('probe','gui','gui2','gui3'): raise ValueError(stem)
script=out/(stem+'.sh')
cmd=['bwrap','--unshare-all','--uid','1000','--gid','1000','--cap-drop','ALL','--die-with-parent','--new-session','--ro-bind',str(root),'/','--proc','/proc','--dev','/dev','--tmpfs','/tmp','--tmpfs','/home','--dir','/home/validation','--ro-bind',str(pkg),'/opt/termcraft','--bind',str(out),'/evidence','--clearenv','--setenv','HOME','/home/validation','--setenv','PATH','/usr/bin','--setenv','LANG','C.UTF-8','--setenv','LD_LIBRARY_PATH','/opt/termcraft/browser/linux-x86_64/libraries','--chdir','/tmp','/usr/bin/bash','/evidence/'+stem+'.sh']
record={'command':cmd,'package':str(pkg),'package_manifest_sha256':hashlib.sha256((pkg/'PACKAGE-MANIFEST.json').read_bytes()).hexdigest(),'script_sha256':hashlib.sha256(script.read_bytes()).hexdigest(),'image':'sha256:1ab61a2bb16febd0a86829a43c26849b06ca82aca31be8385880cc7a3570c79d'}
t=time.monotonic()
try:
 p=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT,timeout=95,start_new_session=True);record['exit_code']=p.returncode;data=p.stdout
except subprocess.TimeoutExpired as e:
 record['exit_code']='timeout';data=e.stdout or b''
record['seconds']=round(time.monotonic()-t,2);(out/(stem+'.log')).write_bytes(data);(out/(stem+'-run.json')).write_text(json.dumps(record,indent=2)+'\n');print(data.decode(errors='replace'));print(record['exit_code'])
