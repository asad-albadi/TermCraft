import pathlib,json,hashlib,sys,os
base=pathlib.Path(__file__).resolve().parent
prerequisites=len(sys.argv)>1 and sys.argv[1]=='--prerequisites'
reports=pathlib.Path(sys.argv[1]) if len(sys.argv)>1 and not prerequisites else base/'package-reports'
pins=json.loads((base/'pins.json').read_text())
for name,expected in pins.items():
 p=base/name;assert p.is_file() and os.access(p,os.X_OK),f'missing executable: {p}'
 assert hashlib.sha256(p.read_bytes()).hexdigest()==expected,f'changed executable: {p}'
if prerequisites:
 print('PASS native prerequisites and pinned executable hashes');sys.exit(0)
counts={}
for backend in ['x11','wayland']:
 for scale in [1,1.5,2]:
  path=reports/f'{backend}-{scale:g}';v=json.loads((path/'observations.json').read_text())
  assert 'package-extracted/TermCraft-0.1.706-linux-x86_64-arch-xwayland/harbor-workspace' in v['app_executable']
  assert v['status']=='PASS' and v['backend']==backend and v['scale']==scale
  assert len(v['observations'])==14,'zero/partial native cases are not a pass'
  assert v['binaries']['termcraft-v06-native-matrix']==pins['../../debug/termcraft-v06-native-matrix']
  assert v['binaries']['v06-clipboard-peer']==pins['../../debug/v06-clipboard-peer']
  if backend=='wayland':assert v['env']['DISPLAY'] is None and v['env']['WAYLAND_DISPLAY']
  else:assert v['env']['DISPLAY']==':71' and v['env']['WAYLAND_DISPLAY'] is None
  cases={}
  for observation in v['observations']:cases[observation['case']]=cases.get(observation['case'],0)+1
  assert cases=={'system_theme_changes_while_idle':6,'visible_theme_pixels':1,'scaled_root_input_coordinates':1,'scaled_detached_views_keep_correct_input_coordinates':2,'clipboard_roundtrip_between_real_windows':2,'scaled_detached_desktop_coordinates':2},cases
  for o in v['observations']:
   if o['case'] in ['scaled_root_input_coordinates','scaled_detached_views_keep_correct_input_coordinates']:
    assert o['pixels_per_point']==scale
    assert o['expected_hex'] in o['observed_hex']
    if backend=='wayland':assert o['native_window']['shell']=='xdg_shell' and o['native_window']['window'] is None
   if o['case']=='scaled_detached_desktop_coordinates':assert o['canvas']['ppp']==scale
  for name in ['theme-System-1','theme-System-2','theme-Light-1','theme-Light-2','theme-Dark-1','theme-Dark-2','multiline-confirmation','detached-input','clipboard-peer','detached-desktop']:
   assert (path/(name+'.png')).stat().st_size>1000,name
  counts[path.name]=14
engine=json.loads((reports/'wayland-1-engine/observations.json').read_text())
assert engine['status']=='PASS' and engine['env']['DISPLAY'] is None
assert engine['engine_executed_sha256']==pins['engine-package/x11/linux-x86_64/Xwayland']
assert engine['app_sha256']==pins['../../debug/termcraft-v06-native-matrix']
summary={'status':'PASS','sessions':6,'native_observations':sum(counts.values()),'packaged_engine_cases':1,'ignored':0,'counts':counts,'scope':'Linux x86_64 private headless native X11/Wayland with software rendering; instrumented production Workspace; original/native-all-platform parity remains unproven'}
(reports/'matrix-summary.json').write_text(json.dumps(summary,indent=2))
print(json.dumps(summary,indent=2))
