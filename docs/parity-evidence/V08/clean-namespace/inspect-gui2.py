import json,os
from pathlib import Path
rows=[]
for p in Path('/proc').iterdir():
 if not p.name.isdigit():continue
 try:
  cmd=(p/'cmdline').read_bytes().replace(b'\0',b' ').decode()
  if not ('chromium' in cmd or 'chrome' in cmd):continue
  rows.append({'pid':int(p.name),'cmdline':cmd,'exe':os.readlink(p/'exe'),'exe_sha256':__import__('hashlib').file_digest((p/'exe').open('rb'),'sha256').hexdigest(),'status':(p/'status').read_text(),'namespaces':{n.name:os.readlink(n) for n in (p/'ns').iterdir()},'uid_map':(p/'uid_map').read_text(),'gid_map':(p/'gid_map').read_text()})
 except (FileNotFoundError,PermissionError,ProcessLookupError) as e:rows.append({'pid':int(p.name),'error':str(e)})
Path('/evidence/gui2-browser-processes.json').write_text(json.dumps(rows,indent=2)+'\n')
print('browser process records',len(rows))
