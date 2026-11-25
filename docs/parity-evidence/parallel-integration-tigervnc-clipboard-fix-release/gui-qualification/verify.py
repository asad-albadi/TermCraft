from pathlib import Path
from PIL import Image
import json,hashlib
B=Path(__file__).resolve().parent;R=B/'gui-reports' if (B/'gui-reports').is_dir() else B;checks=[]
def obj(p):return json.loads(p.read_text())
def check(v,n):
 assert v,n
 checks.append(n)
x=obj(R/'genuine-inputs.json');check(x['source_snapshot_sha256']=='32e512ae85fe16dd44d9b775060a0bd1ac70de345ddcf862a221e0f861db74ad','accepted current source');check(x['archive_sha256']=='d8cec853b37018e55c76a837340c86d8cdb406ef4bff9c6008d7ddebd92098f0','genuine current archive');check(x['application_sha256']=='920f59d2a1486c2c20ccac4aea60df381b79ac2414fb11cf42af6d7d84631e2c','genuine current app');check(x['helper_exit_code']==0 and x['package_exit_code']==0,'both genuine native producer stages exited zero');check(x['files_verified']==3372 and not x['source_changes_during_build'],'strict package inventory/source immutability');check(x['wayland_helper_sha256']=='fa7147099cda83486a4bdc3a59510cecc0e560e10200f4c1bac4bf6ff5b19f59','unchanged corrected optional helper')
probe=obj(R/'normal-startup/result.json');check(probe['status']=='PASS' and probe['normal_app_help'],'normal current app startup/help');check(probe['actual_normal_app_RDP_session_TPKT'] and probe['actual_child_memfd_seals_hash'] and probe['preserved_workspace'],'normal app executes sealed verified RDP helper and preserves profile')
for p in sorted((R/'commands').glob('*.json'),key=lambda p:int(p.stem)):
 c=obj(p)
 if c['kind']=='finish':continue
 a=obj(p.with_suffix('.ack'));check(a.get('ok') is True,'actual native GUI operation '+p.stem)
 if 'password_persistence_scan' in a:check(not a['password_persistence_scan']['hits'],'password absent from private persistence '+p.stem)
check(obj(R/'peer/read-result.json')['text']=='SERVER_CLIPBOARD_OWNED_GUI','real server→normal GUI Copy remote→independent native clipboard peer')
check(obj(R/'peer/set-result.json')['marker_set'],'independent private native client clipboard marker set')
check((R/'independent-server-client-clipboard.txt').read_text()=='CLIENT_CLIPBOARD_OWNED_GUI','normal GUI paste→independent TigerVNC clipboard query')
im=Image.open(R/'actual-vnc-connected.png').convert('RGB');check(im.size==(1600,1000),'actual normal GUI framebuffer rendered');check(im.getpixel((1100,800))==(221,34,34),'known independent TigerVNC red desktop pixel visible')
check(all(p['returncode'] is not None for p in obj(R/'cleanup.json')),'owned normal GUI fixture processes reaped')
clean=obj(B/'owned-fixture-cleanup.json');check(clean['gui_container_exit']==0 and not clean['remaining_matching_containers'],'owned client/daemon containers gone');check(all(p['absent_after'] for p in clean['generated_private_inputs']),'four generated private credential/key files deleted')
iso=obj(R/'isolation.json');check(iso['private_net']!=iso['host_net'] and iso['display'] is None and iso['wayland_before'] is None,'isolated owned network and no host display');check(not {'input','dri','snd'}&set(iso['dev']),'no host input/GPU/audio devices')
transfer=obj(B/'SERVER-COVERAGE-TRANSFER.json');check(transfer['all_13_server_inputs_identical'] and transfer['helper_identical'] and transfer['source_delta']==['src/desktop/vnc_clipboard.rs'],'explicit unchanged125 server/portal evidence transfer')
result={'status':'PASS','check_count':len(checks),'checks':checks,'scope':'current genuine native archive + normal app startup/sealed helper + actual GUI TigerVNC bidirectional clipboard in configured isolated Linux runtime; no physical-desktop or other-OS claim'};(B/'VERIFICATION.json').write_text(json.dumps(result,indent=2));print(json.dumps({'status':'PASS','check_count':len(checks)}))
