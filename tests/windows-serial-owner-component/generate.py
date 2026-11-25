from pathlib import Path
import json,hashlib,sys
p=Path(__file__).resolve().parent
root=Path(sys.argv[1]).resolve() if len(sys.argv)>1 else p.parents[1]
src=root/'engines/rdp/src'
out=p/'src'
out.mkdir(exist_ok=True)
for name in ['serial_windows.rs','serial_stats_windows.rs']:
 s=(src/name).read_text().replace('use windows_sys::','use crate::windows_sys::')
 s=s.replace('    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},\n','')
 # Top-level module keeps its inner documentation first.
 pos=s.index('use ')
 s=s[:pos]+'use crate::native_io::{AsRawHandle, FromRawHandle, OwnedHandle};\n'+s[pos:]
 if name=='serial_windows.rs':
  s=s.replace('thread::Builder::new()', 'crate::spawn_boundary::Builder::new()')
  for child in ['serial_windows_control.rs','serial_stats.rs']:
   s=s.replace('"'+child+'"',json.dumps(str(src/child)))
 (out/name).write_text(s)
# Preserve actual completion wire encoder and the FIX1 completion module.
s=(src/'devices.rs').read_text()
def fn(name):
 a=s.index('fn '+name+'('); b=s.index('{',a); depth=1;c=b+1
 while depth:
  if s[c]=='{':depth+=1
  if s[c]=='}':depth-=1
  c+=1
 return s[a:c]+'\n'
(out/'helpers.rs').write_text('\n'.join(fn(n) for n in ['u32_at','word','header','length_data','complete','completion_header','failure']))
(out/'completion.rs').write_text((src/'serial_completion.rs').read_text())
(out/'inputs.json').write_text(json.dumps({str(q.relative_to(root)):hashlib.sha256(q.read_bytes()).hexdigest() for q in [src/n for n in ['devices.rs','serial_windows.rs','serial_stats_windows.rs','serial_stats.rs','serial_windows_control.rs','serial_completion.rs']]},indent=2)+'\n')
