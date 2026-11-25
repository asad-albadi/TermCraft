from pathlib import Path
from PIL import Image,ImageChops,ImageStat
import json,hashlib
B=Path(__file__).resolve().parent;R=B/'final-reports' if (B/'final-reports').is_dir() else B;checks=[]
def check(v,n):
 assert v,n
 checks.append(n)
def obj(p):return json.loads(p.read_text())
commands={}
for p in sorted((R/'commands').glob('*.json'),key=lambda p:int(p.stem)):
 c=obj(p)
 if c['kind']=='finish':continue
 a=obj(p.with_suffix('.ack'));check(a.get('ok') is True,'native GUI/RFB operation '+p.stem)
 if 'name' in c:commands[c['name']]=(c,a)
def ack(name):return commands[name][1]
inputs=obj(R/'genuine-inputs.json');check(inputs['archive_sha256']=='8a9c97e5dc5791ee278fb485a2082f5fa7c75220801f798e46e1c21a0dd72bc8','genuine corrected archive identity');check(inputs['source_snapshot_sha256']=='d1a54c2ec020d0bc2a4e731184065bc33c28362f9b56f9bd95401f8a14947b9f','genuine source snapshot identity');check(inputs['files_verified']==3372,'strict package inventory verified');check(not inputs['source_changes_during_build'],'source frozen throughout real native build')
helper=inputs['wayland_helper_sha256'];check(helper=='fa7147099cda83486a4bdc3a59510cecc0e560e10200f4c1bac4bf6ff5b19f59','genuine fixed native helper identity')
check(ack('rfb-authenticated')['authenticated'],'normal VNC challenge authentication')
live=ack('running-state');check(live['listener'] and len(live['product_helpers'])==1,'GUI Start creates real listener and owned helper');check(live['product_helpers'][0]['sha256']==helper and 'memfd:' in live['product_helpers'][0]['exe'],'executed sealed child matches genuine packaged helper')
idle=ack('static-idle-eight-seconds');check(idle['idle_seconds']>=8 and idle['listener_alive'] and idle['native_observer_alive'] and idle['product_helpers'][0]['sha256']==helper,'real GUI server survives eight seconds static desktop')
for name in ['rfb-before-idle','rfb-after-idle','viewonly-rfb-capture']:
 a=Image.open(R/(name+'.rfb.png')).convert('RGB');b=Image.open(R/(name+'.png')).convert('RGB');check(a.size==(1600,1000),'RFB framebuffer dimensions '+name);check(a.tobytes()==b.tobytes(),'RFB pixels match separate native monitor capture '+name)
check(Image.open(R/'rfb-before-idle.rfb.png').tobytes()==Image.open(R/'rfb-after-idle.rfb.png').tobytes(),'static pixels unchanged across idle');check(Image.open(R/'rfb-after-idle.rfb.png').convert('RGB').getpixel((650,450))==(38,209,145),'owned target recognizable green content')
events=[json.loads(l) for l in (R/'target/events.jsonl').read_text().splitlines()]
check(any(e['kind']=='key-down' and e.get('keyval')==97 for e in events) and any(e['kind']=='key-up' and e.get('keyval')==97 for e in events),'actual RFB ordinary key reaches native target')
check(any(e['kind']=='button-down' and e.get('button')==1 and e.get('x')==230 and e.get('y')==181 for e in events),'actual RFB absolute pointer/button reaches owned client coordinates')
disconnect=ack('client-disconnect-release')['time']
for kind,sym in [('key-up',65505),('button-up',1)]:check(any(e['kind']==kind and e.get('keyval' if kind=='key-up' else 'button')==sym and disconnect<=e['time']<disconnect+1 for e in events),'client disconnect releases held native '+kind)
for name in ['actual-gui-stop','viewonly-actual-gui-stop']:
 a=ack(name);check(a['pre_listener'] and len(a['pre_helpers'])==1 and not a['post_listener'] and not a['post_helpers'],'actual GUI Stop removes live listener/helper '+name);check(a['post_closed_sessions']==a['pre_closed_sessions']+1 and a['cleanup_seconds']<2,'actual GUI Stop closes portal within two seconds '+name)
check(ack('client-eof-after-gui-stop')['eof'],'controlling client EOF after real GUI Stop')
post=ack('post-stop-unmodified-key')['time'];check(any(e['kind']=='key-down' and e.get('keyval')==98 and e.get('state')==0 and post<=e['time']<post+1 for e in events),'GUI Stop cancellation clears held remote Shift before later target input')
a=ack('actual-native-session-revoked');check(a['pre_listener'] and len(a['pre_helpers'])==1 and not a['post_listener'] and not a['post_helpers'] and a['cleanup_seconds']<2,'genuine backend session revocation closes real GUI service');check(ack('client-eof-after-revocation')['eof'],'client EOF after native revocation')
check(ack('viewonly-client-authenticated')['authenticated'] and obj(R/'viewonly-input-proof.json')['unchanged'],'actual GUI view-only ignores RFB key/pointer without target events')
check(not ack('after-gui-denial')['listener'] and not ack('after-gui-denial')['product_helpers'] and ack('after-gui-denial')['closed_sessions']==4,'actual native permission denial fails closed')
a=ack('actual-gui-stop-pending-permission');check(not a['pre_listener'] and not a['pre_helpers'] and a['post_closed_sessions']==a['pre_closed_sessions']+1 and a['cleanup_seconds']<2,'actual GUI Stop cancels pending native permission request')
for name in ['after-gui-stop-state','after-native-revocation','after-gui-denial','after-pending-gui-stop']:
 a=ack(name);check(a['app_alive'] and a['target_alive'] and a['compositor_alive'],'sibling survival '+name)
for name,(c,a) in commands.items():
 if 'private_persistence_scan' in a:check(not a['private_persistence_scan']['plaintext_password_hits'],'volatile password absent from private workspace/config/data '+name)
# Focused password field is visibly empty after Ctrl+Z; exclude leftmost caret and border.
im=Image.open(R/'password-undo-remains-empty.png').convert('RGB');dark=sum(max(v)<100 for v in im.crop((466,505,651,517)).getdata());check(dark==0,'actual cleared password field remains empty after Undo')
iso=obj(R/'isolation.json');check(iso['host_net']!=iso['private_net'] and iso['display'] is None and iso['wayland_before'] is None,'private network and no host display inherited');check(not {'dri','input','snd'}&set(iso['dev']),'no real GPU/input/audio devices exposed')
check(all(p['returncode'] is not None for p in obj(R/'cleanup.json')),'all explicitly owned private processes reaped')
report={'status':'PASS','check_count':len(checks),'checks':checks,'scope':'actual genuine packaged GUI Start/Stop + real GNOME/Mutter native Linux VNC workflow in configured private headless environment; not global all-clear or physical desktop/other OS qualification'}
(B/'FINAL-VERIFICATION.json').write_text(json.dumps(report,indent=2));print(json.dumps({'status':'PASS','check_count':len(checks)}))
