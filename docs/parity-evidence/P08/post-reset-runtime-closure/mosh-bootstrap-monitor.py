from pathlib import Path
import os,json,hashlib,time
seen={};deadline=time.monotonic()+12
while time.monotonic()<deadline:
 for p in Path('/proc').iterdir():
  if not p.name.isdigit():continue
  try:
   exe=os.readlink(p/'exe')
   if 'termcraft-mosh-' not in exe:continue
   cmd=(p/'cmdline').read_bytes().replace(b'\0',b' ').decode(errors='replace');key=p.name+':'+exe
   if key not in seen:seen[key]={'pid':p.name,'exe':exe,'sha256':hashlib.file_digest((p/'exe').open('rb'),'sha256').hexdigest(),'cmdline':cmd,'status':(p/'status').read_text(),'maps':(p/'maps').read_text(),'environment':(p/'environ').read_bytes().replace(b'\0',b'\n').decode(errors='replace')}
  except OSError:pass
 time.sleep(.025)
Path('/evidence/mosh-bootstrap-processes.json').write_text(json.dumps(list(seen.values()),indent=2)+'\n')
