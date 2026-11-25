from pathlib import Path
import hashlib,json,subprocess
root=Path('app/target/agent-work/b06-puzzle-options'); source=Path('app/target/agent-work/b06-puzzles/source');gitdir=Path('app/target/agent-work/b06-puzzles/upstream.git').resolve();rev='f306b9db55453f7d7fc824bbbf89ca6ac7d2d273'
ranges={'net.c':[(160,208),(273,367),(1510,1558),(1746,1825)],'mines.c':[(94,139),(248,275),(2306,2316),(2600,2661)],'solo.c':[(117,131),(273,331),(1800,1830),(2062,2179),(3066,3124),(3568,3579),(3595,3613)],'guess.c':[(49,101),(214,229),(266,285),(369,410),(574,607),(791,800)],'map.c':[(50,65),(99,156)],'untangle.c':[(119,155),(208,213)],'puzzles.but':[(130,168)]}
manifest={'commit':rev,'upstream':'https://git.tartarus.org/simon/puzzles.git','evidence_class':'RECOVERED historical primary source; no original runtime oracle','files':[]};chunks=[]
for file,spans in ranges.items():
 raw=(source/file).read_bytes();exact=subprocess.check_output(['git',f'--git-dir={gitdir}','show',f'{rev}:{file}']);assert raw==exact,file
 lines=raw.decode('latin1').splitlines(keepends=True);entry={'file':file,'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),'slices':[]}
 for start,end in spans:
  content=''.join(lines[start-1:end]);entry['slices'].append({'start':start,'end':end,'utf8_sha256':hashlib.sha256(content.encode()).hexdigest()});chunks.append(f'===== {file}:{start}-{end} @ {rev} =====\n'+content+'\n')
 manifest['files'].append(entry)
prov=root/'src/games/puzzles/provenance';(prov/'options-source-excerpts.txt').write_text(''.join(chunks));(prov/'options-source-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Verified all selected raw files against exact Git commit; retained source slices')
