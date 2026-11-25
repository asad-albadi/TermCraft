from pathlib import Path
import hashlib,json,subprocess
r=Path(__file__).resolve().parent;raw=(r.parents[1]/'b06-puzzles/source/map.c').read_bytes();gitdir=(r.parents[1]/'b06-puzzles/upstream.git').resolve();rev='f306b9db55453f7d7fc824bbbf89ca6ac7d2d273'
assert raw==subprocess.check_output(['git',f'--git-dir={gitdir}','show',f'{rev}:map.c'])
ranges=[(684,812),(1418,1480),(1510,1588)];lines=raw.decode('latin1').splitlines(keepends=True);slices=[];chunks=[]
for a,b in ranges:
 text=''.join(lines[a-1:b]);slices.append({'start':a,'end':b,'utf8_sha256':hashlib.sha256(text.encode()).hexdigest()});chunks.append(f'===== map.c:{a}-{b} @ {rev} =====\n'+text+'\n')
p=r/'work/src/games/puzzles/provenance';(p/'map-generation-fix2-source.txt').write_text(''.join(chunks));(p/'map-generation-fix2-source.json').write_text(json.dumps({'commit':rev,'upstream':'https://git.tartarus.org/simon/puzzles.git','raw_map_c_sha256':hashlib.sha256(raw).hexdigest(),'slices':slices,'scope':'source randomized MRV colourer, random clue removal and final lower-grade rejection; degree priority is an independent heuristic','licence':'UPSTREAM-LICENCE retained'},indent=2)+'\n')
print('Pinned map.c equals exact Git object; retained colourer/removal/final-grade slices. No upstream runtime executed.')
