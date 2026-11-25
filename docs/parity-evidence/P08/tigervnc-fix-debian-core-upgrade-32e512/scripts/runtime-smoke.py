from pathlib import Path
import importlib.util,json,os,sys,tempfile
from PIL import Image,ImageStat
spec=importlib.util.spec_from_file_location('smoke','/input/scripts/package-check-startup.py');smoke=importlib.util.module_from_spec(spec);spec.loader.exec_module(smoke)
e=Path('/evidence')/sys.argv[1];e.mkdir()
root=Path('/opt/termcraft');host,m=smoke.verify(root,root/'PACKAGE-MANIFEST.json');assert m['profile']=='release';assert m['build']['snapshot_sha256']==sys.argv[2]
assert os.readlink('/usr/bin/termcraft')=='/opt/termcraft/harbor-workspace'
env={**os.environ,'PATH':'','HOME':'/tmp/qualification-home','XDG_CONFIG_HOME':'/tmp/qualification-home/config','XDG_DATA_HOME':'/tmp/qualification-home/data','DISPLAY':':99','LIBGL_ALWAYS_SOFTWARE':'1'}
for k in list(env):
 if k.startswith(('LD_','DYLD_','CARGO_','TERMCRAFT_')):env.pop(k)
with tempfile.TemporaryDirectory(prefix='installed-native-') as d:
 isolated=Path(d);exe=Path('/usr/bin/termcraft');helper=root/'termcraft-rdp-engine'
 assert b'TermCraft remote workspace' in smoke.run([exe,'--help'],isolated,env,e/'help.log')
 out=smoke.run([helper],isolated,env,e/'rdp-helper.log');assert out[0]==1 and json.loads(out[5:5+int.from_bytes(out[1:5],'big')])=={'Ready':{'version':1}}
 for name in ['workspace','corrupt']:
  directory=Path('/tmp/qualification-home')/name
  for p in directory.iterdir():
   if p.name in ('workspace.json','credentials.vault'):(e/(name+'.before.'+p.name)).write_bytes(p.read_bytes())
  before={p.name:smoke.sha256(p) for p in directory.iterdir() if p.name in ('workspace.json','credentials.vault')}
  smoke.run([exe,'--data-dir',directory,'--screenshot',e/(name+'.png')],isolated,env,e/(name+'.log'))
  after={p.name:smoke.sha256(p) for p in directory.iterdir() if p.name in ('workspace.json','credentials.vault')}
  for p in directory.iterdir():
   if p.name in ('workspace.json','credentials.vault'):(e/(name+'.after.'+p.name)).write_bytes(p.read_bytes())
  assert before==after,(name,before,after)
  with Image.open(e/(name+'.png')) as image:assert image.width>=640 and image.height>=400 and max(ImageStat.Stat(image.convert('RGB')).stddev)>2
 smoke.rdp_session_probe(exe,m['files']['termcraft-rdp-engine'],isolated,env,e)
 (e/'verification.json').write_text(json.dumps({'profile':m['profile'],'source_sha256':m['build']['snapshot_sha256'],'manifest_files_verified':len(m['files']),'workspace_and_corrupt_bytes_preserved':True,'native_gui_screenshots':True,'helper_ready_and_sealed_child':True,'uid':os.getuid()},indent=2)+'\n')
print('Installed native manifest, GUI, helper, workspace/vault and corrupt-store preservation passed')
