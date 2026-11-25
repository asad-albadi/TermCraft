from pathlib import Path
import subprocess,json,hashlib,time,traceback,sys
r=Path.cwd();q=r/'target/agent-work/tigervnc-fix-debian-upgrade';e=q/'evidence';e.mkdir(exist_ok=True);history=[];source=json.loads((q/'source-identity.json').read_text())['source_sha256']
image='debian:bookworm';name='termcraft-tigervnc-fix-debian-runtime';label='org.termcraft.validation=tigervnc-fix-debian-upgrade'
def run(key,cmd,check=True):
 start=time.time();p=subprocess.run(cmd,stdout=subprocess.PIPE,stderr=subprocess.STDOUT);(e/(key+'.log')).write_bytes(p.stdout);history.append({'label':key,'command':list(map(str,cmd)),'exit_code':p.returncode,'seconds':round(time.time()-start,3)});(e/'commands.json').write_text(json.dumps(history,indent=2)+'\n');print(key,p.returncode,p.stdout.decode(errors='replace')[-600:],flush=True)
 if check and p.returncode:raise RuntimeError(key)
 return p
try:
 image=run('runtime-image',['docker','image','inspect',image,'--format','{{.Id}}']).stdout.decode().strip()
 old=r/'target/agent-work/post-xdmcp-fix-debian-upgrade/build/out'
 run('create',['docker','run','-d','--name',name,'--label',label,'--cpuset-cpus','0','--pids-limit','256','--memory','3g','--security-opt','no-new-privileges','--mount',f'type=bind,src={q}/input,dst=/input,readonly','--mount',f'type=bind,src={q}/build,dst=/current,readonly','--mount',f'type=bind,src={old},dst=/old,readonly','--mount',f'type=bind,src={q}/scripts,dst=/driver,readonly','--mount',f'type=bind,src={e},dst=/evidence',image,'sleep','infinity'])
 def ex(key,args,check=True):return run(key,['docker','exec',name,*args],check)
 def shell(key,cmd):return ex(key,['sh','-ec',cmd])
 shell('baseline','cat /etc/os-release; dpkg-query -W; test ! -e /opt/termcraft; test ! -e /usr/bin/termcraft')
 ex('apt-update',['apt-get','update'])
 ex('clean-install',['apt-get','install','-y','--no-install-recommends','/current/out/termcraft_0.1.0-3_amd64.deb'])
 audit='dpkg-query -s termcraft; dpkg --audit; dpkg -V termcraft; ldd /usr/bin/termcraft; ldd /opt/termcraft/termcraft-rdp-engine; sha256sum /opt/termcraft/harbor-workspace /opt/termcraft/PACKAGE-MANIFEST.json'
 shell('clean-dependency-audit',audit)
 ex('package-info',['dpkg-deb','--info','/current/out/termcraft_0.1.0-3_amd64.deb'])
 ex('package-contents',['dpkg-deb','--contents','/current/out/termcraft_0.1.0-3_amd64.deb'])
 ex('test-tools',['apt-get','install','-y','--no-install-recommends','python3','python3-pil','xvfb','xauth','fonts-dejavu-core','fontconfig'])
 shell('native-runtime-versions','python3 --version; ldd --version; dpkg-query -W libc6 libegl1 libgl1 libx11-6 libxkbcommon0 python3 xvfb; getconf GNU_LIBC_VERSION')
 run('disconnect',['docker','network','disconnect','bridge',name]);shell('network-state','cat /proc/net/route; cat /proc/net/dev')
 shell('isolation','mkdir -p /tmp/qualification-home /tmp/qualification-runtime; chmod 700 /tmp/qualification-runtime; chown -R 1000:1000 /tmp/qualification-home /tmp/qualification-runtime /evidence; setpriv --reuid=1000 --regid=1000 --clear-groups sh -c "Xvfb :99 -screen 0 1280x800x24 -nolisten tcp > /evidence/xvfb.log 2>&1 &"')
 common=['setpriv','--reuid=1000','--regid=1000','--clear-groups','env','HOME=/tmp/qualification-home','XDG_RUNTIME_DIR=/tmp/qualification-runtime','DISPLAY=:99','LIBGL_ALWAYS_SOFTWARE=1']
 ex('create-real-fixture',common+['/current/persistence-fixture','create','/tmp/qualification-home/workspace'])
 ex('seed-workspace',common+['python3','/driver/seed-workspace.py'])
 shell('corrupt-fixture','mkdir /tmp/qualification-home/corrupt; printf "owned deliberately corrupt workspace {" > /tmp/qualification-home/corrupt/workspace.json; printf "TCVAULT1-owned-corrupt-vault" > /tmp/qualification-home/corrupt/credentials.vault; chmod 600 /tmp/qualification-home/corrupt/*; chown -R 1000:1000 /tmp/qualification-home/corrupt')
 ex('check-corrupt',common+['/current/persistence-fixture','corrupt','/tmp/qualification-home/corrupt'])
 time.sleep(1)
 ex('clean-installed-startup',common+['python3','/driver/runtime-smoke.py','clean-installed',source])
 shell('userdata-initial','find /tmp/qualification-home/workspace /tmp/qualification-home/corrupt -type f ! -name "*.vault-lock" -exec sha256sum {} + | sort')
 # Remove clean-install current package, then install the immutable older real artifact as the upgrade baseline.
 ex('remove-clean',['apt-get','remove','-y','termcraft'])
 shell('clean-remove-preserved','test ! -e /usr/bin/termcraft; test ! -e /opt/termcraft; find /tmp/qualification-home/workspace /tmp/qualification-home/corrupt -type f ! -name "*.vault-lock" -exec sha256sum {} + | sort; dpkg --audit')
 ex('install-old-baseline',['apt-get','install','-y','--no-install-recommends','/old/termcraft_0.1.0-2_amd64.deb']);shell('old-audit',audit)
 ex('old-warmed-startup',common+['python3','/driver/runtime-smoke.py','old-warmed','22bbf26840638474044431d242b92efa076123e87f39849cf0c8d967b33ecb04'])
 shell('userdata-old-baseline','find /tmp/qualification-home/workspace /tmp/qualification-home/corrupt -type f ! -name "*.vault-lock" -exec sha256sum {} + | sort')
 ex('upgrade-current',['apt-get','install','-y','--no-install-recommends','/current/out/termcraft_0.1.0-3_amd64.deb']);shell('upgrade-audit',audit)
 ex('monotonic-version',['dpkg','--compare-versions','0.1.0-3','gt','0.1.0-2'])
 ex('upgrade-startup',common+['python3','/driver/runtime-smoke.py','upgraded-installed',source])
 ex('upgrade-vault',common+['/current/persistence-fixture','verify','/tmp/qualification-home/workspace'])
 ex('same-version-replacement',['apt-get','install','-y','--reinstall','--no-install-recommends','/current/out/termcraft_0.1.0-3_amd64.deb']);shell('same-version-audit',audit)
 ex('same-version-vault',common+['/current/persistence-fixture','verify','/tmp/qualification-home/workspace'])
 ex('remove-upgraded',['apt-get','remove','-y','termcraft'])
 shell('removed-preserved','test ! -e /usr/bin/termcraft; test ! -e /opt/termcraft; find /tmp/qualification-home/workspace /tmp/qualification-home/corrupt -type f ! -name "*.vault-lock" -exec sha256sum {} + | sort; dpkg --audit')
 ex('reinstall-current',['apt-get','install','-y','--no-install-recommends','/current/out/termcraft_0.1.0-3_amd64.deb']);shell('reinstalled-audit',audit)
 ex('reinstalled-startup',common+['python3','/driver/runtime-smoke.py','reinstalled',source])
 ex('reinstalled-vault',common+['/current/persistence-fixture','verify','/tmp/qualification-home/workspace'])
 ex('reinstalled-corrupt',common+['/current/persistence-fixture','corrupt','/tmp/qualification-home/corrupt'])
 final=shell('userdata-final','find /tmp/qualification-home/workspace /tmp/qualification-home/corrupt -type f ! -name "*.vault-lock" -exec sha256sum {} + | sort').stdout
 assert final==(e/'userdata-initial.log').read_bytes()
 for key in ['clean-remove-preserved','userdata-old-baseline','removed-preserved']:
  assert (e/(key+'.log')).read_bytes()==final
 (e/'result.json').write_text(json.dumps({'result':'pass','source_sha256':source,'runtime_image':image,'upgrade_type':'monotonic Debian packaging upgrade 0.1.0-2 -> 0.1.0-3; embedded app version unchanged','portable_sha256':hashlib.sha256((q/'build/out/TermCraft-0.1.0-linux-x86_64.tar.gz').read_bytes()).hexdigest(),'deb_sha256':hashlib.sha256((q/'build/out/termcraft_0.1.0-3_amd64.deb').read_bytes()).hexdigest(),'workspace_vault_corrupt_bytes_preserved':True},indent=2)+'\n')
except Exception:
 (e/'failure.txt').write_text(traceback.format_exc());raise
finally:
 run('cleanup',['docker','rm','-f',name],False)
 run('cleanup-inventory',['docker','ps','-a','--filter','label='+label,'--format','{{.ID}} {{.Names}}'],False)
