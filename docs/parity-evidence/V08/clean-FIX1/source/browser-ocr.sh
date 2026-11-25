for mode in minimal runtime; do
 tesseract "/evidence/browser-$mode.png" "/evidence/browser-$mode-ocr" -l eng --psm 11
done
python - <<'PY'
from pathlib import Path
import json
rows=[]
for mode in ['minimal','runtime']:
 text=' '.join(Path(f'/evidence/browser-{mode}-ocr.txt').read_text().split()).lower()
 assert 'the chromium process exited. check the installed browser and sandbox configuration.' in text,text
 rows.append({'mode':mode,'actual_ui_error_observed':True,'screenshot':f'browser-{mode}.png','ocr':f'browser-{mode}-ocr.txt'})
Path('/evidence/browser-ui-results.json').write_text(json.dumps(rows,indent=2)+'\n');print(json.dumps(rows))
PY
