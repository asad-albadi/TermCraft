from pathlib import Path
import importlib.util,sys,json,hashlib,subprocess,time
r=Path.cwd();q=r/'target/agent-work/current-debian-upgrade';expected=sys.argv[1]
spec=importlib.util.spec_from_file_location('release',r/'scripts/package-release.py');release=importlib.util.module_from_spec(spec);spec.loader.exec_module(release)
source,inputs=release.freeze_sources(r,q/'sources');assert source.name==expected,(source.name,expected)
(q/'source-identity.json').write_text(json.dumps({'source_sha256':source.name,'source_inputs':inputs},indent=2)+'\n')
(q/'input').symlink_to(source.relative_to(q),target_is_directory=True)
image='sha256:1e7cd41f77c63d2093907d8a33cc65562eafd5cfbed60c829b9faedf652f24a0'
cmd=['docker','run','--rm','--name','termcraft-current-debian-build','--label','org.termcraft.validation=current-debian-upgrade','--network','none','--read-only','--cpuset-cpus','0','--user','1000:1000','--cap-drop','ALL','--security-opt','no-new-privileges','--pids-limit','256','--memory','6g','--tmpfs','/tmp:rw,nosuid,mode=1777','--tmpfs','/cargo:rw,nosuid,mode=1777','--env','HOME=/tmp/home','--env','CARGO_HOME=/cargo','--env','CARGO_BUILD_JOBS=2','--env','TERMCRAFT_OFFLINE=1','--mount',f'type=bind,src={source},dst=/src,readonly','--mount','type=bind,src=/home/asad/.cargo/registry,dst=/cargo/registry,readonly','--mount',f'type=bind,src={q}/build,dst=/build','--mount',f'type=bind,src={q}/scripts,dst=/driver,readonly','--workdir','/build','--entrypoint','/usr/bin/bash',image,'/driver/release.sh']
(q/'build-command.json').write_text(json.dumps({'command':cmd,'source_sha256':expected},indent=2)+'\n')
started=time.time()
with (q/'native-build.log').open('xb') as output:result=subprocess.run(cmd,stdout=output,stderr=subprocess.STDOUT)
(q/'build-result.json').write_text(json.dumps({'exit_code':result.returncode,'seconds':round(time.time()-started,3),'image':image,'source_sha256':expected},indent=2)+'\n')
if result.returncode:sys.exit(result.returncode)
import tarfile
with tarfile.open(q/'build/out/TermCraft-0.1.0-linux-x86_64.tar.gz') as bundle:
 names=[m for m in bundle.getmembers() if m.name.endswith('/PACKAGE-MANIFEST.json')];assert len(names)==1;m=json.load(bundle.extractfile(names[0]));assert m['profile']=='release' and m['build']['snapshot_sha256']==expected and m['build']['source_inputs']==inputs
(q/'portable-manifest.json').write_text(json.dumps(m,indent=2)+'\n')
(q/'artifact-hashes.json').write_text(json.dumps({p.name:hashlib.sha256(p.read_bytes()).hexdigest() for p in (q/'build/out').iterdir() if p.is_file()},indent=2)+'\n')
print('Native ordinary release and Debian packaging passed: '+expected)
