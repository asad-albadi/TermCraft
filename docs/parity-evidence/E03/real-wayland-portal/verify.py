from pathlib import Path
from PIL import Image,ImageChops
import json,hashlib
B=Path(__file__).resolve().parent;APP=B.parents[2];R=B/'reports'; checks=[]
def check(v,n):
 assert v,n
 checks.append(n)
for p,h in json.loads((B/'source-hashes.json').read_text()).items():
 check(hashlib.sha256((APP/p).read_bytes()).hexdigest()==h,'production source unchanged '+p)
 check((APP/p).read_bytes()==(B/'source'/p).read_bytes(),'harness exact source '+p)
check('org.freedesktop.portal.ScreenCast' in (R/'introspection.xml').read_text(),'real frontend ScreenCast present')
check('org.freedesktop.portal.RemoteDesktop' not in (R/'introspection.xml').read_text(),'RemoteDesktop unavailable on actual frontend')
for mode in ['control','deny','cancel']:
 result=json.loads((R/mode/'result.json').read_text());check('open_error' in result,mode+' fails closed');check(result['elapsed_ms']<2500,mode+' bounded under 2.5 sec')
check('denied or cancelled' in json.loads((R/'deny/result.json').read_text())['open_error'],'actual chooser denial propagated')
check((R/'revoke/revoke-call.txt').read_text().strip()=='()','actual backend Session.Close completed')
for mode in ['revoke','normal-drop','capture']:
 result=json.loads((R/mode/'result.json').read_text());check('view-only' in result['view_only_input_error'],'view-only input rejected '+mode)
 check(result['elapsed_ms']<2500,'capture/drop lifecycle bounded '+mode)
 check(result['after_error'] is None if mode=='normal-drop' else result['after_error'] is not None,'native lifecycle result '+mode)
 im=Image.open(R/mode/'captured.png').convert('RGBA');ref=Image.open(R/'owned-target.png').convert('RGBA')
 check(im.size==(640,480),'native capture dimensions '+mode)
 check(im.tobytes()==ref.tobytes(),'exact independent compositor pixel match '+mode)
 check(len(im.getcolors(640*480))>200,'recognizable nonblank colors '+mode)
check('termcraft-wayland-engine' not in (R/'remaining-processes.json').read_text(),'no sealed video helpers left after provider drop')
check('memfd:termcraft' not in (R/'remaining-processes.json').read_text(),'no sealed executables left after provider drop')
check(all(v is None for v in json.loads((R/'sibling-survival.json').read_text()).values()),'owned compositor and target survive every provider lifecycle')
check(all(x['returncode'] is not None for x in json.loads((R/'cleanup.json').read_text())),'all explicitly owned child processes reaped')
iso=json.loads((R/'isolation.json').read_text());check(iso['private_net']!=iso['host_net'],'separate network namespace');check(iso['wayland_before'] is None and iso['display'] is None,'no host display inherited');check(not any(x in iso['dev'] for x in ['dri','input','snd']),'no hardware/input devices exposed')
summary={'status':'PASS','check_count':len(checks),'checks':checks,'scope':'exact production provider source harness, actual native ScreenCast, configured private headless compositor; not GUI release qualification or native remote control'}
(B/'verification.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))
