from pathlib import Path
import json,os
rows=[]
for p in Path('/proc').iterdir():
 try:
  e=os.readlink(p/'exe');c=(p/'cmdline').read_bytes().replace(b'\0',b' ').decode()
  if 'termcraft-mosh-' in e or e=='/opt/termcraft/harbor-workspace':rows.append({'pid':p.name,'exe':e,'cmd':c})
 except (OSError,UnicodeError):pass
Path('/evidence/mosh-cleanup.json').write_text(json.dumps({'remaining_owned_client_processes':rows},indent=2)+'\n');assert not rows,rows
