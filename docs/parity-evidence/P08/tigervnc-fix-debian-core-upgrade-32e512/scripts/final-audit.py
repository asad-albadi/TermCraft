from pathlib import Path
import hashlib,json,tarfile,subprocess,re
r=Path.cwd();q=r/'target/agent-work/tigervnc-fix-debian-upgrade';e=q/'evidence';sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
source=json.loads((q/'source-identity.json').read_text());assert all(sha(r/k)==v for k,v in source['source_inputs'].items())
archive=q/'build/out/TermCraft-0.1.0-linux-x86_64.tar.gz';manifest=json.loads((q/'portable-manifest.json').read_text())
with tarfile.open(archive) as t:
 files={m.name.split('/',1)[1]:m for m in t.getmembers() if m.isfile()}
 for name,digest in manifest['files'].items():assert hashlib.sha256(t.extractfile(files[name]).read()).hexdigest()==digest,name
 assert set(files)==set(manifest['files'])|{'PACKAGE-MANIFEST.json'}
 assert not any(any(x in n.lower() for x in ['xephyr','xdmcp','xwayland']) for n in files)
 versions={}
 for name in ['harbor-workspace','termcraft-rdp-engine']:
  p=q/'preflight'/name;p.write_bytes(t.extractfile(files[name]).read());out=subprocess.run(['readelf','--version-info',str(p)],check=True,capture_output=True,text=True).stdout;(q/'preflight'/(name+'.readelf.log')).write_text(out)
  versions[name]=max(set(re.findall(r'GLIBC_(\d+\.\d+)',out)),key=lambda s:tuple(map(int,s.split('.'))));p.unlink()
(q/'early-core-integrity.json').write_text(json.dumps({'archive_path':str(archive),'archive_sha256':sha(archive),'archive_bytes':archive.stat().st_size,'source_sha256':source['source_sha256'],'manifest_files_verified':len(manifest['files']),'app_sha256':manifest['files']['harbor-workspace'],'helper_sha256':manifest['files']['termcraft-rdp-engine'],'elf_glibc_requirements':versions,'managed_x_runtime_payload_entries':[]},indent=2)+'\n')
commands=json.loads((e/'commands.json').read_text());assert all(c['exit_code']==0 for c in commands)
pairs=[]
for before in sorted(e.glob('*/*.before.*')):
 after=before.with_name(before.name.replace('.before.','.after.'));assert before.read_bytes()==after.read_bytes();pairs.append({'before':str(before.relative_to(q)),'after':str(after.relative_to(q)),'sha256':sha(before)})
assert len(pairs)==16,len(pairs)
(q/'byte-preservation-proof.json').write_text(json.dumps(pairs,indent=2)+'\n')
inventories=['userdata-initial','clean-remove-preserved','userdata-old-baseline','removed-preserved','userdata-final'];assert len({(e/(n+'.log')).read_bytes() for n in inventories})==1
for p in e.glob('*/verification.json'):
 m=json.loads(p.read_text());assert m['manifest_files_verified']==1074 and m['uid']==1000
assert 'Unpacking termcraft (0.1.0-3) over (0.1.0-2)' in (e/'upgrade-current.log').read_text()
assert not (e/'cleanup-inventory.log').read_text().strip()
old=r/'target/agent-work/post-xdmcp-fix-debian-upgrade';prior=r/'docs/parity-evidence/P08/post-xdmcp-fix-debian-core-upgrade-22bbf268';seal=json.loads((prior/'evidence-sha256.json').read_text());assert all(sha(prior/k)==v for k,v in seal.items())
artifacts=json.loads((old/'artifact-hashes.json').read_text());assert all(sha(old/'build/out'/k)==v for k,v in artifacts.items())
(q/'final-verification.json').write_text(json.dumps({'result':'pass','source_sha256':source['source_sha256'],'runtime_commands_exit_zero':len(commands),'before_after_byte_comparisons':len(pairs),'lifecycle_hash_list_equality':inventories,'owned_container_inventory_empty':True,'prior_22bb_seal_sha256':sha(prior/'evidence-sha256.json'),'prior_22bb_sealed_files_preserved':len(seal),'prior_22bb_artifacts_preserved':artifacts,'app_glibc_requirement':versions['harbor-workspace'],'helper_glibc_requirement':versions['termcraft-rdp-engine'],'managed_x_runtime_payload_entries':[],'saved_workspace_screenshot_identical_to_prior':(e/'reinstalled/workspace.png').read_bytes()==(old/'evidence/reinstalled/workspace.png').read_bytes()},indent=2)+'\n')
print('Independent final audit passed:',len(commands),'commands;',len(pairs),'byte pairs;',len(manifest['files']),'payload files')
