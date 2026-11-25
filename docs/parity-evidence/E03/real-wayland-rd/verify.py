from pathlib import Path
from PIL import Image,ImageChops,ImageStat
import json,hashlib,re
B=Path(__file__).resolve().parent;APP=B.parents[2];R=B/'reports';checks=[]
def check(v,n):
 assert v,n
 checks.append(n)
def obj(p):return json.loads(p.read_text())
sourcefiles=['src/services/vnc/wayland.rs','src/services/vnc/wayland/portal.rs','src/services/vnc/wayland/helper.rs','src/services/vnc/keyboard.rs','src/runtime/manifest.rs','packaging/runtime-manifest.json']
for p in sourcefiles:check((APP/p).read_bytes()==(B/'source'/p).read_bytes(),'exact current production source '+p)
for package in ['mutter','xdg-desktop-portal-gnome','gnome-desktop-4','startup-notification']:check('Good signature' in (B/(package+'-signature.log')).read_text(),'official package signature '+package)
check('Created surfaceless renderer without GPU' in (R/'mutter.log').read_text(),'native renderer without GPU')
check('org.freedesktop.portal.RemoteDesktop' in (R/'portal-introspection.xml').read_text(),'genuine frontend RemoteDesktop exposed')
for mode in ['control','viewonly']:
 p=R/mode;check((p/'opened').exists(),'native portal permission granted '+mode)
 im=Image.open(p/'provider-capture-0.png').convert('RGBA');ref=Image.open(R/(mode+'-after-share.png')).convert('RGBA')
 check(im.size==(800,600),'native virtual monitor dimensions '+mode)
 check(im.tobytes()==ref.tobytes(),'pixel match separate native compositor capture '+mode)
 check(im.getpixel((400,300))==(38,209,145,255),'owned target green center rendered '+mode)
 check(im.getpixel((10,300))==(20,48,79,255),'owned target blue edge rendered '+mode)
 (p/'pixel-comparison.json').write_text(json.dumps({'dimensions':im.size,'mean_absolute_difference':ImageStat.Stat(ImageChops.difference(im,ref)).mean,'capture_sha256':hashlib.sha256(im.tobytes()).hexdigest(),'independent_session_reference':str(ref.filename) if hasattr(ref,'filename') else mode+'-after-share.png'}))
 check(obj(p/'lifecycle.json')['returncode']==0,'provider exited normally '+mode)
for n in range(12):check('error' not in obj(R/'control'/f'ack-{n}.json'),'production control action succeeded '+str(n))
events=[json.loads(x) for x in (R/'target/events.jsonl').read_text().splitlines()]
def key(kind,sym):return [e for e in events if e['kind']==kind and e.get('keyval')==sym]
for sym in [97,65505,98]:
 down=key('key-down',sym);up=key('key-up',sym)
 check(bool(down) and bool(up) and down[0]['time']<up[0]['time'],'native target key press/release '+str(sym))
primer=obj(R/'fixture-keyboard-primer.json');ready=obj(R/'keyboard-ready-before-provider.json')
check(primer['before_target_map'] and primer['production_provider_not_started'],'owned fixture keyboard established before target/provider')
wire='\n'.join(ready['wire_lines'])
check('capabilities(2)' in wire and 'get_keyboard' in wire and '.keymap(' in wire and re.search(r'wl_keyboard#\d+\.enter\(',wire),'target keyboard capability/binding/keymap/focus before provider')
first=obj(R/'control/timing-4.json');check(ready['time']<first['start'],'keyboard readiness precedes first production key')
check(re.search(r'wl_keyboard#\d+\.key\([^\n]*, 30, 1\)',(R/'owned-target.log').read_text()) is not None,'first ordinary production a-down observed on native Wayland wire')
check(re.search(r'wl_keyboard#\d+\.key\([^\n]*, 30, 0\)',(R/'owned-target.log').read_text()) is not None,'ordinary production a-up observed on native Wayland wire')
comparison=B/'direct-native-comparison';cmpwire=(comparison/'owned-target.log').read_text()
check('capabilities(0)' in cmpwire and 'capabilities(2)' in cmpwire,'independent unprimed native compositor comparison shows seat capability transition')
cmpevents=[json.loads(l) for l in (comparison/'target/events.jsonl').read_text().splitlines() if 'key-' in l]
check([e['kind'] for e in cmpevents]==['key-up','key-down','key-up'],'unprimed initial-key loss reproduced without production provider')
release=obj(R/'control/timing-8.json');stop=obj(R/'control/timing-11.json')
shift_up=key('key-up',65505)[0];b_up=key('key-up',98)[0]
check(release['start']<=shift_up['time']<=release['done']+.2,'Release releases held native Shift')
check(stop['start']<=b_up['time']<=stop['done']+.2,'Stop/drop releases held native b')
for i,label in [(8,'Release'),(11,'Stop/drop')]:
 timing=obj(R/'control'/f'timing-{i}.json')
 check(any(e['kind']=='button-up' and timing['start']<=e['time']<=timing['done']+.2 for e in events),label+' releases held native left button')
 check(timing['done']-timing['start']<1,label+' bounded under one second')
check(any(e['kind']=='motion' and abs(e['x']-350)<.01 and abs(e['y']-261)<.01 for e in events),'production absolute pointer maps correct owned client position')
check(all('view-only' in obj(R/'viewonly'/f'ack-{i}.json').get('error','') for i in [1,2]),'view-only rejects native keyboard and pointer requests')
check(obj(R/'viewonly/input-observation.json')['unchanged'],'view-only rejected requests produce no target events')
for mode,msg in [('control-denied','did not grant keyboard and pointer'),('deny','denied or cancelled'),('cancel','failed, cancelled or timed out')]:
 result=obj(R/mode/'result.json');check(msg in result.get('open_error',''),'native '+mode+' fail-closed response');check(result['elapsed_ms']<2500,'native '+mode+' bounded under 2.5 sec including fixture consent wait')
check(all(v is None for v in obj(R/'sibling-survival.json').values()),'owned compositor and target survive all provider lifecycles')
ps=(R/'processes-after-providers.txt').read_text();check('memfd:termcraft' not in ps and 'real-wayland-rd-provider-private' not in ps,'no product provider or sealed helper survives Stop')
# One explicitly named nonsealed helper remains solely for fixture native monitor screenshots until fixture teardown.
check(sum('termcraft-wayla' in line for line in ps.splitlines())==1,'only explicit owned screenshot fixture helper remains before teardown')
check(all(p['returncode'] is not None for p in obj(R/'cleanup.json')),'all explicitly owned fixture children reaped')
iso=obj(R/'isolation.json');check(iso['private_net']!=iso['host_net'],'separate network namespace');check(iso['display'] is None and iso['wayland_before'] is None,'host displays absent');check(not any(x in iso['dev'] for x in ['dri','input','snd']),'no GPU or input devices exposed')
report={'status':'PASS','check_count':len(checks),'checks':checks,'scope':'actual GNOME portal/Mutter native headless Linux RemoteDesktop input through exact production provider; source harness, not release GUI or physical GPU desktop'}
(B/'verification.json').write_text(json.dumps(report,indent=2));print(json.dumps(report,indent=2))
