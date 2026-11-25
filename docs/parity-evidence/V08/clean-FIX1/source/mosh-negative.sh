python - <<'PY'
from pathlib import Path
import json
for key,provider in [('system','system'),('bundled','bundled')]:
 p=Path('/evidence/fixture-mosh-'+key);p.mkdir(exist_ok=True)
 (p/'workspace.json').write_text(json.dumps({'sessions':[{'id':'0123456789abcdef0123456789abcdef','name':'FIX1 '+key+' Mosh','protocol':'Mosh','host':'127.0.0.1','user':'fixture','mosh_provider':provider}]}))
PY
for provider in system bundled; do
 launch "fixture-mosh-$provider"
 /opt/termcraft/harbor-workspace --data-dir "/evidence/fixture-mosh-$provider" --session 0123456789abcdef0123456789abcdef
 sleep 2
 shot "mosh-$provider"
 tesseract "/evidence/mosh-$provider.png" "/evidence/mosh-$provider-ocr" -l eng --psm 11
 stop
done
python - <<'PY'
from pathlib import Path
import json,shutil
assert shutil.which('mosh') is None
assert not Path('/usr/bin/perl').exists()
rows=[]
for provider,message in [('system','The selected installed Mosh provider is unavailable. Install the required system program.'),('bundled','Bundled Mosh requires system Perl at /usr/bin/perl')]:
 text=' '.join(Path(f'/evidence/mosh-{provider}-ocr.txt').read_text().split())
 assert message.lower() in text.lower(),text
 rows.append({'provider':provider,'status':'EXPECTED_ERROR_OBSERVED','required_error':message,'ocr_file':f'mosh-{provider}-ocr.txt','screenshot':f'mosh-{provider}.png','installed_mosh_absent':shutil.which('mosh') is None,'perl_absent':not Path('/usr/bin/perl').exists()})
Path('/evidence/mosh-negative-results.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows))
cat=Path('/opt/termcraft-validation-fix1-ocr-packages.txt');Path('/evidence/ocr-image-packages.txt').write_bytes(cat.read_bytes())
PY
