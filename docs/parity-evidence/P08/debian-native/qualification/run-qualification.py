from pathlib import Path
import subprocess,json,hashlib,time,traceback
r=Path.cwd(); q=r/'target/validation/p08-debian-native/qualification'; e=r/'docs/parity-evidence/P08/debian-native/qualification'; a=r/'target/validation/p08-debian-native/build/verified-out';name='termcraft-p08-native-clean-install'; history=[]
def run(label,cmd,check=True):
    started=time.time();p=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(e/(label+'.log')).write_bytes(p.stdout);history.append({'label':label,'command':cmd,'exit_code':p.returncode,'seconds':round(time.time()-started,3)});(e/'qualification-commands.json').write_text(json.dumps(history,indent=2)+'\n');print(label,p.returncode,p.stdout.decode(errors='replace')[-700:],flush=True)
    if check and p.returncode:raise RuntimeError(label)
    return p
try:
    image=run('clean-image-id',['docker','image','inspect','debian:bookworm','--format','{{.Id}}']).stdout.decode().strip()
    run('clean-image-inspect',['docker','image','inspect',image])
    run('container-create',['docker','run','-d','--name',name,'--label','org.termcraft.validation=p08-native-qualification','--pids-limit','256','--memory','3g','--cpus','2','--security-opt','no-new-privileges','--mount',f'type=bind,src={q}/input,dst=/input,readonly','--mount',f'type=bind,src={a},dst=/archive,readonly','--mount',f'type=bind,src={q}/out,dst=/packages,readonly','--mount',f'type=bind,src={q}/installed-smoke.py,dst=/installed-smoke.py,readonly','--mount',f'type=bind,src={e},dst=/evidence',image,'sleep','infinity'])
    def ex(label,args,check=True):return run(label,['docker','exec',name,*args],check)
    ex('clean-baseline',['sh','-c','cat /etc/os-release; dpkg-query -W; test ! -e /opt/termcraft; test ! -e /usr/bin/termcraft'])
    ex('apt-update',['apt-get','update'])
    ex('package-install',['apt-get','install','-y','--no-install-recommends','/packages/termcraft_0.1.0_amd64.deb'])
    ex('installed-dependency-audit',['sh','-c','dpkg-query -s termcraft; dpkg --audit; dpkg -V termcraft; ldd /usr/bin/termcraft; ldd /opt/termcraft/termcraft-rdp-engine'])
    ex('test-tools-install',['apt-get','install','-y','--no-install-recommends','python3','python3-pil','xvfb','xauth','fonts-dejavu-core','fontconfig'])
    ex('installed-packages',['dpkg-query','-W'])
    run('network-isolation',['docker','network','disconnect','bridge',name])
    ex('network-state',['sh','-c','cat /proc/net/route; cat /proc/net/dev'])
    ex('prepare-isolation',['sh','-c','mkdir -p /tmp/qualification-home /tmp/qualification-runtime /evidence/portable; rmdir /evidence/portable; chmod 700 /tmp/qualification-runtime; printf "retained user workspace\\n" > /tmp/qualification-home/user-sentinel.txt; chown -R 1000:1000 /tmp/qualification-home /tmp/qualification-runtime /evidence; setpriv --reuid=1000 --regid=1000 --clear-groups sh -c "Xvfb :99 -screen 0 1280x800x24 -nolisten tcp > /evidence/xvfb.log 2>&1 &"'])
    time.sleep(1)
    common=['setpriv','--reuid=1000','--regid=1000','--clear-groups','env','HOME=/tmp/qualification-home','XDG_RUNTIME_DIR=/tmp/qualification-runtime','DISPLAY=:99','LIBGL_ALWAYS_SOFTWARE=1']
    ex('portable-startup',common+['python3','/input/scripts/package-check-startup.py','/archive/TermCraft-0.1.0-linux-x86_64.tar.gz','--evidence','/evidence/portable','--display',':99'])
    ex('installed-startup',common+['python3','/installed-smoke.py'])
    ex('userdata-before-remove',['sh','-c','find /tmp/qualification-home -type f -exec sha256sum {} +'])
    ex('uninstall',['apt-get','remove','-y','termcraft'])
    ex('uninstall-preservation',['sh','-c','test ! -e /usr/bin/termcraft; test ! -e /opt/termcraft; test "$(cat /tmp/qualification-home/user-sentinel.txt)" = "retained user workspace"; find /tmp/qualification-home -type f -exec sha256sum {} +; dpkg --audit'])
    (e/'qualification-result.json').write_text(json.dumps({'result':'pass','archive_sha256':hashlib.sha256((a/'TermCraft-0.1.0-linux-x86_64.tar.gz').read_bytes()).hexdigest(),'deb_sha256':hashlib.sha256((q/'out/termcraft_0.1.0_amd64.deb').read_bytes()).hexdigest(),'runtime_image':image,'scope':'genuine native release core archive; older default 13-games snapshot; clean Debian 12 install, network-disconnected nonroot Xvfb portable and installed startup, verified helper child, uninstall preservation'},indent=2)+'\n')
except Exception:
    (e/'qualification-failure.txt').write_text(traceback.format_exc());raise
finally:
    run('cleanup-container',['docker','rm','-f',name],False)
    run('cleanup-inventory',['docker','ps','-a','--filter','label=org.termcraft.validation=p08-native-qualification','--format','{{.ID}} {{.Names}}'],False)
