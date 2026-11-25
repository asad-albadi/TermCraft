from pathlib import Path
import shutil,json,hashlib
r=Path.cwd();q=r/'target/agent-work/tigervnc-fix-debian-upgrade';dest=r/'docs/parity-evidence/P08/tigervnc-fix-debian-core-upgrade-32e512';assert not dest.exists();dest.mkdir(parents=True)
root_files=['artifact-hashes.json','build-command.json','build-result.json','native-build.log','portable-manifest.json','source-identity.json','source-during-build.json','source-delta-from-22bb.json','cache-provenance.json','early-core-integrity.json','deb-payload-verification.json','byte-preservation-proof.json','final-verification.json','visual-inspection.json','final-owned-container-inventory.log','runtime-runner.log','evidence-summary.md']
for n in root_files:shutil.copy2(q/n,dest/n)
excluded=[]
for directory in ['preflight','scripts','evidence']:
 for p in sorted((q/directory).rglob('*')):
  if not p.is_file() or '__pycache__' in p.parts:continue
  rel=p.relative_to(q)
  if '.before.' in p.name or '.after.' in p.name:excluded.append(str(rel));continue
  out=dest/rel;out.parent.mkdir(parents=True,exist_ok=True);shutil.copy2(p,out)
(dest/'private-evidence-index.json').write_text(json.dumps({'private_workspace':str(q),'raw_snapshots_not_promoted':excluded,'exact_raw_byte_pair_hash_proof':'byte-preservation-proof.json','reason':'Raw workspace/vault/corrupt before-after files retained privately; fixture is wholly synthetic.'},indent=2)+'\n')
seal={str(p.relative_to(dest)):hashlib.sha256(p.read_bytes()).hexdigest() for p in sorted(dest.rglob('*')) if p.is_file()};(dest/'evidence-sha256.json').write_text(json.dumps(seal,indent=2)+'\n');print('Promoted',len(seal),'safe sealed files; retained',len(excluded),'raw private snapshots. Seal:',hashlib.sha256((dest/'evidence-sha256.json').read_bytes()).hexdigest())
