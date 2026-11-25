from pathlib import Path
import importlib.util,json,os,tempfile,subprocess
from PIL import Image, ImageStat
spec=importlib.util.spec_from_file_location('smoke','/input/scripts/package-check-startup.py'); smoke=importlib.util.module_from_spec(spec); spec.loader.exec_module(smoke)
e=Path('/evidence/installed');e.mkdir()
root=Path('/opt/termcraft');host,manifest=smoke.verify(root,root/'PACKAGE-MANIFEST.json');assert manifest['profile']=='release'
assert os.readlink('/usr/bin/termcraft')=='/opt/termcraft/harbor-workspace'
with tempfile.TemporaryDirectory(prefix='installed-native-') as d:
    isolated=Path(d);env={**os.environ,'PATH':'','HOME':'/tmp/qualification-home','XDG_CONFIG_HOME':'/tmp/qualification-home/config','XDG_DATA_HOME':'/tmp/qualification-home/data','DISPLAY':':99','LIBGL_ALWAYS_SOFTWARE':'1'}
    for k in list(env):
        if k.startswith(('LD_','DYLD_','CARGO_','TERMCRAFT_')):env.pop(k)
    exe=Path('/usr/bin/termcraft');helper=root/'termcraft-rdp-engine'
    out=smoke.run([exe,'--help'],isolated,env,e/'help.log');assert b'TermCraft remote workspace' in out
    out=smoke.run([helper],isolated,env,e/'rdp-helper.log');assert out[0]==1 and json.loads(out[5:5+int.from_bytes(out[1:5],'big')])=={'Ready':{'version':1}}
    smoke.run([exe,'--data-dir','/tmp/qualification-home/workspace','--screenshot',e/'startup.png'],isolated,env,e/'startup.log')
    with Image.open(e/'startup.png') as im:
        size=im.size;std=ImageStat.Stat(im.convert('RGB')).stddev;assert size[0]>=640 and size[1]>=400 and max(std)>2
    smoke.rdp_session_probe(exe,manifest['files']['termcraft-rdp-engine'],isolated,env,e)
    (e/'verification.json').write_text(json.dumps({'installed_launcher':str(exe),'profile':manifest['profile'],'manifest_files_verified':len(manifest['files']),'screen_size':size,'screen_stddev':std,'rdp_helper_wire_version':1,'rdp_helper_child_location_verified':True,'rdp_loopback_negotiation':True,'workspace_preferences_preserved':True,'uid':os.getuid()},indent=2)+'\n')
print('installed native GUI, helper handshake and verified sealed child passed')
