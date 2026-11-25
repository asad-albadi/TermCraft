from pathlib import Path
import hashlib,json,subprocess,os
root=Path(os.environ.get('B06_ADVANCED_ROOT',Path(__file__).resolve().parent)).resolve()
source=root.parent/'b06-puzzles/source';gitdir=(root.parent/'b06-puzzles/upstream.git').resolve();rev='f306b9db55453f7d7fc824bbbf89ca6ac7d2d273'
ranges={'solo.c':[(117,131),(304,322),(640,710),(878,1392),(1393,1610),(1690,2525),(3370,3780)],'map.c':[(50,65),(116,132),(945,1368),(1520,1585)],'puzzles.but':[(1000,1045),(1878,1908)],'LICENCE':[(1,25)]}
manifest={'commit':rev,'upstream':'https://git.tartarus.org/simon/puzzles.git','evidence_class':'RECOVERED pinned historical primary source, no original runtime oracle','files':[]};chunks=[]
for file,spans in ranges.items():
 raw=(source/file).read_bytes();exact=subprocess.check_output(['git',f'--git-dir={gitdir}','show',f'{rev}:{file}']);assert raw==exact,file
 lines=raw.decode('latin1').splitlines(keepends=True);entry={'file':file,'sha256':hashlib.sha256(raw).hexdigest(),'bytes':len(raw),'slices':[]}
 for start,end in spans:
  content=''.join(lines[start-1:end]);entry['slices'].append({'start':start,'end':end,'utf8_sha256':hashlib.sha256(content.encode()).hexdigest()});chunks.append(f'===== {file}:{start}-{end} @ {rev} =====\n'+content+'\n')
 manifest['files'].append(entry)
prov=root/'work/src/games/puzzles/provenance';(prov/'advanced-source-excerpts.txt').write_text(''.join(chunks));(prov/'advanced-source-manifest.json').write_text(json.dumps(manifest,indent=2)+'\n')
print('Verified solo.c, map.c, manual and LICENCE byte-for-byte against exact pinned Git revision; preserved advanced solver, stock-table, generator and manual slices.')
