from pathlib import Path
import json,hashlib,sys
from PIL import Image,ImageChops,ImageStat
base=Path(__file__).resolve().parent
reports=base/'package-reports';app=base/'package-extracted/TermCraft-0.1.708-linux-x86_64-arch-xwayland/harbor-workspace'
appsha=hashlib.sha256(app.read_bytes()).hexdigest();source=json.loads((base/'instrumented-source-sha256.json').read_text())
for name,expected in source.items():assert hashlib.sha256((base/'source'/name).read_bytes()).hexdigest()==expected,name
result=[]
for backend in ['x11','wayland']:
 folder=reports/f'{backend}-1.5';report=json.loads((folder/'observations.json').read_text());obs=report['observations']
 assert report['status']=='PASS' and report['backend']==backend and report['scale']==1.5 and len(obs)==15
 assert report['binaries']['termcraft-v06-current708']==appsha and report['app_executable']==str(app)
 env=report['env'];assert (env['DISPLAY'] is None and env['WAYLAND_DISPLAY']) if backend=='wayland' else (env['DISPLAY']==':71' and env['WAYLAND_DISPLAY'] is None)
 isolation=json.loads((folder/'isolation.json').read_text());assert isolation['host_net']!=isolation['private_net'] and isolation['host_socket_identity']!=isolation['private_socket_identity']
 assert isolation['display_before_backend'] is None and isolation['wayland_before_backend'] is None
 differences={}
 for a,b in [('theme-System-2','theme-System-1'),('theme-Light-1','theme-Light-2'),('theme-Dark-1','theme-Dark-2')]:differences[a+' vs '+b]=sum(ImageStat.Stat(ImageChops.difference(Image.open(folder/(a+'.png')).convert('RGB'),Image.open(folder/(b+'.png')).convert('RGB'))).mean)
 assert differences['theme-System-2 vs theme-System-1']>100 and differences['theme-Light-1 vs theme-Light-2']<2 and differences['theme-Dark-1 vs theme-Dark-2']<2
 for o in obs:
  if o['case']=='system_theme_changes_while_idle':assert o['observed']['theme']==o['expected'] and not o['observed']['games_open'] and not o['observed']['game_session_present'] and not o['observed']['idle_active']
 pty=(folder/'pty.bin').read_bytes();payload=(folder/'payload').read_bytes();assert payload in pty and (folder/'peer-text').read_bytes()==payload
 pointers=[json.loads(line) for line in (folder/'vnc-input.jsonl').read_text().splitlines()]
 for o in obs:
  if o['case'] in ['scaled_root_input_coordinates','scaled_detached_views_keep_correct_input_coordinates']:
   assert bytes.fromhex(o['expected_hex']) in pty and o['pixels_per_point']==1.5
   if backend=='wayland':assert o['native_window']['shell']=='xdg_shell' and o['native_window']['window'] is None
  if o['case']=='scaled_detached_desktop_coordinates':
   assert o['observed'] in pointers and o['observed']['mask']==1 and o['canvas']['ppp']==1.5
   assert all(abs(o['observed'][axis]-value)<=1 for axis,value in zip(['x','y'],o['expected']))
 guards=[json.loads(line) for line in (folder/'game-guards.jsonl').read_text().splitlines()];assert len({g['viewport'] for g in guards})>=2
 assert all(g['return'] is False and g['state']=={'launcher':False,'suspended':False,'session_present':False,'close_owner_present':False} for g in guards)
 cleanup=json.loads((folder/'cleanup.json').read_text());assert not cleanup['survivors_after_reap']
 result.append({'backend':backend,'scale':1.5,'status':'PASS','native_observations':14,'additional_default_guard_observation':1,'guard_callbacks':len(guards),'viewports':sorted({g['viewport'] for g in guards}),'recomputed_pixel_differences':differences,'payload_sha256':hashlib.sha256(payload).hexdigest(),'survivors':0})
summary={'status':'PASS','scope':'current-parent integrated default inactive-game normal terminal/clipboard/desktop paths; focused1.5x replay only','sessions':2,'native_observations':28,'additional_default_guard_observations':2,'ignored':0,'app_sha256':appsha,'sessions_detail':result,'active_games_native_qualified':False,'historical706_evidence_unchanged':True,'strict_original_native_all_platform_complete':0,'verification_semantics':'read-only audit recomputes retained pixels/raw clipboard/SGR/VNC/guard outcomes and verifies source/executable bytes; fresh native execution is separately recorded by matrix.py runs'}
(base/'reports/current708-summary.json').write_text(json.dumps(summary,indent=2));print(json.dumps(summary,indent=2))
