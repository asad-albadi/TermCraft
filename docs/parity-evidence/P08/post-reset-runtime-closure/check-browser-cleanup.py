from pathlib import Path
import json
rows=[]
for p in Path('/proc').iterdir():
 try:
  c=(p/'cmdline').read_bytes().replace(b'\0',b' ').decode()
  if 'termcraft-browser-runtime-' in c or '/opt/termcraft/harbor-workspace' in c: rows.append({'pid':p.name,'cmdline':c})
 except (OSError,UnicodeError): pass
Path('/evidence/browser-current-cleanup.json').write_text(json.dumps({'remaining':rows},indent=2))
assert not rows, rows
