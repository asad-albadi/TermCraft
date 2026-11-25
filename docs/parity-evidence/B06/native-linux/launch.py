import subprocess,json,hashlib,time
from pathlib import Path
base=Path(__file__).resolve().parents[4]; out=Path(__file__).resolve().parent
root=base/'target/validation/v08-clean-namespace/rootfs'; frozen=base/'target/qualification-native-games'
cmd=['bwrap','--unshare-all','--uid','1000','--gid','1000','--cap-drop','ALL','--die-with-parent','--new-session','--ro-bind',str(root),'/', '--proc','/proc','--dev','/dev','--tmpfs','/tmp','--tmpfs','/home','--dir','/home/validation','--ro-bind',str(frozen),'/opt/termcraft','--bind',str(out),'/evidence','--clearenv','--setenv','HOME','/home/validation','--setenv','PATH','/usr/bin','--setenv','LANG','C.UTF-8','--chdir','/tmp','/usr/bin/bash','/evidence/session.sh']
(out/'namespace-command.json').write_text(json.dumps(cmd,indent=2)+'\n')
p=subprocess.run(cmd); print('namespace exited',p.returncode)
