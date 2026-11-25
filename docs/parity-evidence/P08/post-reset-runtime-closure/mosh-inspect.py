from pathlib import Path
import json,os,hashlib
rows=[]
for p in Path('/proc').iterdir():
 if not p.name.isdigit():continue
 try:
  cmd=(p/'cmdline').read_bytes().replace(b'\0',b' ').decode(); exe=os.readlink(p/'exe')
  if 'termcraft-mosh-' not in exe and '/opt/termcraft/harbor-workspace' not in exe and 'mosh' not in cmd:continue
  rows.append({'pid':p.name,'cmdline':cmd,'exe':exe,'sha256':hashlib.file_digest((p/'exe').open('rb'),'sha256').hexdigest(),'status':(p/'status').read_text(),'maps':(p/'maps').read_text(),'environment':(p/'environ').read_bytes().replace(b'\0',b'\n').decode(errors='replace')})
 except (OSError,UnicodeError):pass
Path('/evidence/mosh-processes.json').write_text(json.dumps(rows,indent=2)+'\n')
