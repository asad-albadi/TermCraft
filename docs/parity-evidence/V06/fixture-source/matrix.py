import os,sys,pathlib,subprocess,time,json,signal,hashlib,ctypes
ctypes.CDLL(None).prctl(36,1,0,0,0)
from PIL import Image,ImageChops,ImageStat
BASE=pathlib.Path(__file__).resolve().parent
BIN=BASE.parents[1]/'debug'
APP=pathlib.Path(os.environ.get('V06_APP_BINARY',str(BIN/'termcraft-v06-native-matrix')))
owned=[]
def start(args,log,env=None):
 f=open(log,'wb');p=subprocess.Popen([str(x) for x in args],stdout=f,stderr=subprocess.STDOUT,env=env);owned.append(p);return p

def wait(fn,seconds=12):
 end=time.monotonic()+seconds
 while time.monotonic()<end:
  try:
   result=fn()
   if result:return result
  except (FileNotFoundError,json.JSONDecodeError,subprocess.CalledProcessError,IndexError):pass
  time.sleep(.05)
 raise AssertionError('timeout: '+repr(fn))

def read(p):
 for attempt in range(20):
  try:return json.loads(p.read_text())
  except json.JSONDecodeError:time.sleep(.005)
 raise AssertionError('persistently invalid observation '+str(p))
def call(args,**kw):return subprocess.check_output([str(x) for x in args],text=True,**kw)
def main():
 mode=sys.argv[1] if len(sys.argv)>1 else 'wayland';scale=float(sys.argv[2]) if len(sys.argv)>2 else 1.
 engine_only=len(sys.argv)>3 and sys.argv[3]=='engine'
 report=pathlib.Path(os.environ.get('V06_REPORT_ROOT',str(BASE/'reports')))/f'{mode}-{scale:g}{"-engine" if engine_only else ""}';report.mkdir(parents=True,exist_ok=False)
 private_net=os.readlink('/proc/self/ns/net');st=pathlib.Path('/tmp/.X11-unix').stat();private_sockets=f'{st.st_dev}:{st.st_ino}'
 assert private_net!=os.environ['V06_HOST_NET'] and private_sockets!=os.environ['V06_HOST_SOCKETS']
 (report/'isolation.json').write_text(json.dumps({'host_net':os.environ['V06_HOST_NET'],'private_net':private_net,'host_socket_identity':os.environ['V06_HOST_SOCKETS'],'private_socket_identity':private_sockets,'display_before_backend':os.environ.get('DISPLAY'),'wayland_before_backend':os.environ.get('WAYLAND_DISPLAY'),'pid':os.getpid()}))
 os.environ['V06_REPORT']=str(report)
 os.environ['V06_ENGINE_PACKAGE']=os.environ.get('V06_BUNDLE',str(BASE/'engine-package'))
 os.environ['V06_PROBE_EXE']=str(BASE/'probe-wrapper.sh')
 env=os.environ.copy();env.pop('DISPLAY',None);env.pop('WAYLAND_DISPLAY',None)
 cfg=report/'sway.conf';cfg.write_text(('for_window [app_id="org.freedesktop.Xwayland"] floating enable\nfor_window [title="^Xwayland"] floating enable\n' if engine_only else '')+f'output HEADLESS-1 mode 1920x1080 scale {scale}\nseat seat0 fallback true\nxwayland disable\nfocus_follows_mouse no\ndefault_border none\n')
 if mode=='wayland':
  compositor=start(['sway','--unsupported-gpu','-c',cfg],report/'compositor.log',env)
  socket=wait(lambda:next(pathlib.Path('/tmp/v06-runtime').glob('wayland-*'),None))
  os.environ['WAYLAND_DISPLAY']=socket.name
  os.environ['SWAYSOCK']=str(wait(lambda:next(pathlib.Path('/tmp/v06-runtime').glob('sway-ipc*sock'),None)))
  keyboard=start(['wtype','-s','300000'],report/'keyboard.log')
 else:
  xvfb=BASE.parent/'original-runtime/xvfb/usr/bin/Xvfb'
  compositor=start([xvfb,':71','-screen','0','1920x1080x24','-nolisten','tcp','-ac'],report/'compositor.log',env)
  wait(lambda:pathlib.Path('/tmp/.X11-unix/X71').exists());os.environ['DISPLAY']=':71';os.environ['WINIT_X11_SCALE_FACTOR']=str(scale)
 (report/'portal-value').write_text('2')
 portal=start([BIN/'v06-appearance-portal',report/'portal-value'],report/'portal.log')
 wait(lambda:'ready' in (report/'portal.log').read_text())
 vnc=start([sys.executable,'-I',BASE/'vnc_peer.py',report],report/'vnc.log');wait(lambda:(report/'vnc-port').exists());os.environ['V06_VNC_PORT']=(report/'vnc-port').read_text()
 app=start([APP,'--data-dir',report/'data'],report/'app.log')
 wait(lambda:(report/'workspace.json').exists())
 def command(value):
  (report/'command').write_text(value);time.sleep(.25)
 def capture(label):
  path=report/(label+'.png')
  if mode=='wayland':subprocess.run(['grim',str(path)],check=True)
  else:subprocess.run(['import','-window','root',str(path)],check=True)
  assert path.stat().st_size>1000
 def tree():return json.loads(call(['swaymsg','-t','get_tree']))
 def nodes(node):
  yield node
  for child in node.get('nodes',[])+node.get('floating_nodes',[]):yield from nodes(child)
 def focus(title):
  if mode=='wayland':
   node=wait(lambda:next((n for n in nodes(tree()) if n.get('name')==title),None))
   call(['swaymsg',f'[con_id={node["id"]}] focus']);return node
  wid=wait(lambda:call(['xdotool','search','--onlyvisible','--name','^'+('TermCraft' if title.startswith('TermCraft') else title)+('' if title.startswith('TermCraft') else '$')]).strip().splitlines()[-1])
  call(['xdotool','windowraise',wid]);wait(lambda:call(['xdotool','windowfocus','--sync',wid]) is not None);return wid
 def click_terminal(col,row):
  metrics=read(report/'terminal.json');r=next(r for r in metrics['rows'] if r['row']==row)
  localx=r['min'][0]+(col+.5)*metrics['cell_width'];localy=(r['min'][1]+r['max'][1])/2
  if mode=='wayland':
   n=focus('V06 Native Terminal' if read(report/'workspace.json')['detached'] else 'TermCraft — Remote workspace');rect=n['rect'];x=rect['x']+localx;y=rect['y']+localy
   call(['v06-pointer',str(round(x)),str(round(y)),str(round(1920/scale)),str(round(1080/scale))])
  else:
   wid=focus('V06 Native Terminal' if read(report/'workspace.json')['detached'] else 'TermCraft — Remote workspace')
   x=round(localx*metrics['pixels_per_point']);y=round(localy*metrics['pixels_per_point'])
   call(['xdotool','mousemove','--window',wid,str(x),str(y),'click','1'])
  expected=f'\x1b[<0;{col+1};{row+1}M'.encode()
  wait(lambda:expected in (report/'pty.bin').read_bytes())
  return {'col':col,'row':row,'native_window':n if mode=='wayland' else wid,'local':[localx,localy],'injected':[x,y],'pixels_per_point':metrics['pixels_per_point'],'expected_hex':expected.hex(),'observed_hex':(report/'pty.bin').read_bytes().hex()}
 def paste_keys():
  if mode=='wayland':call(['wtype','-M','ctrl','-M','shift','-k','v','-m','shift','-m','ctrl'])
  else:call(['xdotool','key','ctrl+shift+v'])
 if engine_only:
  assert mode=='wayland' and 'DISPLAY' not in os.environ
  command('xserver');state=wait(lambda:(lambda v:v if v['display'] else None)(read(report/'xserver.json')),25)
  cap=state['capabilities']['Ok'];assert not cap['errors'],cap
  assert cap['opengl']['matched_pixels']==cap['opengl']['sampled_pixels'] and cap['opengl']['sampled_pixels']>0
  assert cap['composite_pixels'] and cap['randr_change'] and cap['initial_mode_restored']
  engines=[]
  for proc in pathlib.Path('/proc').iterdir():
   if not proc.name.isdigit():continue
   try:
    data=(proc/'cmdline').read_bytes()
    if b'/x11/linux-x86_64/Xwayland' in data and (proc/'exe').readlink().name=='memfd:termcraft-xwayland (deleted)':engines.append(proc)
   except (FileNotFoundError,PermissionError):pass
  assert len(engines)==1,engines
  actual_hash=hashlib.sha256((engines[0]/'exe').read_bytes()).hexdigest()
  assert actual_hash=='cbedd90fbc960d4cc799792cedf89b0afa1f1f74a93ba9fb05bfd4da2f27b2b8'
  client_env=os.environ.copy();client_env.pop('WAYLAND_DISPLAY',None);client_env['DISPLAY']=state['display'];client_env['XAUTHORITY']=state['authority']
  gears=start(['glxgears','-geometry','400x300'],report/'glxgears.log',client_env);time.sleep(1)
  engine_tree=tree();assert any(n.get('app_id')=='org.freedesktop.Xwayland' and n.get('visible') for n in nodes(engine_tree)),engine_tree
  capture('packaged-xserver-glx-client');(report/'engine-tree.json').write_text(json.dumps(engine_tree,indent=2))
  gears.terminate();gears.wait(timeout=5);command('stop-xserver');wait(lambda:not read(report/'xserver.json')['running'])
  assert not engines[0].exists() and not pathlib.Path(state['authority']).exists()
  (report/'observations.json').write_text(json.dumps({'status':'PASS','backend':'wayland','scale':scale,'case':'packaged_xserver_actual_workspace_start_render_stop','engine_executed_sha256':actual_hash,'state':state,'app_sha256':hashlib.sha256(APP.read_bytes()).hexdigest(),'env':{k:os.environ.get(k) for k in ['DISPLAY','WAYLAND_DISPLAY','DBUS_SESSION_BUS_ADDRESS']}},indent=2))
  print('PASS engine native Workspace startup/GLX/Composite/RandR/visible client/owned stop',flush=True);return
 observations=[]
 binaries={name:hashlib.sha256((APP if name=='termcraft-v06-native-matrix' else BIN/name).read_bytes()).hexdigest() for name in ['termcraft-v06-native-matrix','v06-clipboard-peer','v06-appearance-portal']}
 (report/'app-environ.txt').write_bytes(pathlib.Path(f'/proc/{app.pid}/environ').read_bytes().replace(b'\x00',b'\n'))
 assert mode!='wayland' or 'DISPLAY' not in os.environ
 # Portal transition alone must wake an otherwise idle root window.
 for appearance,portal_value,expected in [('System',2,'Light'),('System',1,'Dark'),('Light',1,'Light'),('Light',2,'Light'),('Dark',2,'Dark'),('Dark',1,'Dark')]:
  command('appearance '+appearance)
  before=read(report/'workspace.json')['frame'];(report/'portal-value').write_text(str(portal_value))
  wait(lambda:read(report/'workspace.json')['theme']==expected)
  time.sleep(2.3)
  state=read(report/'workspace.json');assert state['theme']==expected
  label=f'theme-{appearance}-{portal_value}';capture(label)
  observations.append({'case':'system_theme_changes_while_idle','appearance':appearance,'portal':portal_value,'expected':expected,'observed':state,'before_frame':before,'capture':label+'.png'})
 images={k:Image.open(report/(k+'.png')).convert('RGB') for k in ['theme-System-2','theme-System-1','theme-Light-1','theme-Light-2','theme-Dark-1','theme-Dark-2']}
 differences={}
 for a,b in [('theme-System-2','theme-System-1'),('theme-Light-1','theme-Light-2'),('theme-Dark-1','theme-Dark-2')]:
  differences[a+' vs '+b]=sum(ImageStat.Stat(ImageChops.difference(images[a],images[b])).mean)
 assert differences['theme-System-2 vs theme-System-1']>100,differences
 assert differences['theme-Light-1 vs theme-Light-2']<2,differences
 assert differences['theme-Dark-1 vs theme-Dark-2']<2,differences
 observations.append({'case':'visible_theme_pixels','differences':differences})
 command('terminal');wait(lambda:(report/'terminal.json').exists() and (report/'pty.bin').exists());time.sleep(.5)
 # Root terminal mapping, then an actual native detached viewport.
 observations.append({'case':'scaled_root_input_coordinates',**click_terminal(8,0)})
 # Detached title is controlled by production WorkspaceWindows.
 command('detach');wait(lambda:read(report/'workspace.json')['detached']);time.sleep(.6)
 focus('V06 Native Terminal');time.sleep(.2)
 for col,row in [(4,2),(20,5)]:observations.append({'case':'scaled_detached_views_keep_correct_input_coordinates',**click_terminal(col,row)})
 capture('detached-input')
 payload='V06 Unicode: 日本語 café Ελληνικά 🦀\nsecond line\nthird line'
 (report/'payload').write_text(payload)
 peer=start([BIN/'v06-clipboard-peer'],report/'peer.log');focus('V06 Clipboard Peer');time.sleep(.5)
 (report/'peer-command').write_text('copy');time.sleep(.5)
 focus('V06 Native Terminal');command('paste')
 if wait(lambda:(report/'confirmation.json').exists(),5):
  c=read(report/'confirmation.json');x,y=c['center'];root=focus('TermCraft — Remote workspace');capture('multiline-confirmation')
  if mode=='wayland':call(['v06-pointer',str(round(root['rect']['x']+x)),str(round(root['rect']['y']+y)),str(round(1920/scale)),str(round(1080/scale))])
  else:call(['xdotool','mousemove','--window',root,str(round(x*c['ppp'])),str(round(y*c['ppp'])),'click','1'])
 expected=payload.encode();wait(lambda:expected in (report/'pty.bin').read_bytes())
 observations.append({'case':'clipboard_roundtrip_between_real_windows','direction':'native peer window -> production read_os_text -> detached PTY','expected_sha256':hashlib.sha256(expected).hexdigest(),'pty_hex':(report/'pty.bin').read_bytes().hex()})
 # Production egui copy goes through the backend's native selection owner.
 command('copy');time.sleep(.5);focus('V06 Clipboard Peer');time.sleep(.3)
 if mode=='wayland':call(['wtype','-M','ctrl','-k','v','-m','ctrl'])
 else:(report/'peer-command').write_text('paste')
 capture('clipboard-peer-requested')
 wait(lambda:(report/'peer-text').read_text()==payload)
 observations.append({'case':'clipboard_roundtrip_between_real_windows','direction':'production egui copy -> native peer TextEdit','expected_sha256':hashlib.sha256(expected).hexdigest(),'actual_sha256':hashlib.sha256((report/'peer-text').read_bytes()).hexdigest()})
 capture('clipboard-peer')
 command('vnc');wait(lambda:(report/'desktop.json').exists());command('detach');time.sleep(.8)
 n=focus('V06 Native Desktop')
 if mode=='wayland':
  call(['swaymsg',f'[con_id={n["id"]}] floating enable, resize set 600 400, move position 300 50']);time.sleep(.5);n=focus('V06 Native Desktop')
 time.sleep(.3)
 canvas=read(report/'desktop.json');left,top,right,bottom=canvas['rect']
 for nx,ny in [(0.25,0.25),(0.75,0.75)]:
  localx=left+(right-left)*nx;localy=top+(bottom-top)*ny
  if mode=='wayland':call(['v06-pointer',str(round(n['rect']['x']+localx)),str(round(n['rect']['y']+localy)),str(round(1920/scale)),str(round(1080/scale))])
  else:call(['xdotool','mousemove','--window',n,str(round(localx*canvas['ppp'])),str(round(localy*canvas['ppp'])),'click','1'])
  ex,ey=int(nx*canvas['width']),int(ny*canvas['height'])
  def actual():
   values=[json.loads(line) for line in (report/'vnc-input.jsonl').read_text().splitlines()]
   return next((v for v in values if v['type']=='pointer' and v['mask']==1 and abs(v['x']-ex)<=1 and abs(v['y']-ey)<=1),None)
  got=wait(actual)
  observations.append({'case':'scaled_detached_desktop_coordinates','canvas':canvas,'expected':[ex,ey],'observed':got,'native_window':n})
 capture('detached-desktop')

 (report/'observations.json').write_text(json.dumps({'status':'PASS','backend':mode,'app_executable':str(APP),'binaries':binaries,'scale':scale,'env':{k:os.environ.get(k) for k in ['DISPLAY','WAYLAND_DISPLAY','SWAYSOCK','XDG_RUNTIME_DIR','DBUS_SESSION_BUS_ADDRESS','WINIT_X11_SCALE_FACTOR']},'observations':observations},indent=2))
 print('PASS',mode,scale,len(observations),flush=True)

try:main()
finally:
 def descendants(pid):
  try:children=[int(x) for x in pathlib.Path(f'/proc/{pid}/task/{pid}/children').read_text().split()]
  except FileNotFoundError:return []
  return children+[child for p in children for child in descendants(p)]
 test_children=descendants(os.getpid())
 for p in reversed(owned):
  if p.poll() is None:p.terminate()
 for p in reversed(owned):
  try:p.wait(timeout=5)
  except subprocess.TimeoutExpired:p.kill();p.wait()

 for child in test_children:
  try:os.kill(child,signal.SIGTERM)
  except ProcessLookupError:pass
 cleanup_deadline=time.monotonic()+3
 while time.monotonic()<cleanup_deadline:
  try:
   pid,status=os.waitpid(-1,os.WNOHANG)
   if pid==0:time.sleep(.02)
  except ChildProcessError:break
 survivors=[p for p in test_children if pathlib.Path(f'/proc/{p}').exists()]
 for child in survivors:
  try:os.kill(child,signal.SIGKILL)
  except ProcessLookupError:pass
 while True:
  try:os.waitpid(-1,0)
  except ChildProcessError:break
 cleanup={'owned':[{'pid':p.pid,'returncode':p.returncode} for p in owned],'descendants':test_children,'survivors_after_reap':[p for p in test_children if pathlib.Path(f'/proc/{p}').exists()]}
 report_path=pathlib.Path(os.environ.get('V06_REPORT',str(BASE/'reports')))
 (report_path/'cleanup.json').write_text(json.dumps(cleanup,indent=2))
 assert not cleanup['survivors_after_reap'],cleanup
