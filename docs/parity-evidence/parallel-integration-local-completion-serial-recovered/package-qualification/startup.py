from pathlib import Path
import hashlib,importlib.util,json,os,secrets,select,struct,subprocess,time
root=Path('/opt/termcraft');e=Path('/evidence');home=Path('/home/validation');home.mkdir(exist_ok=True)
auth=Path('/tmp/xauthority');cookie=secrets.token_bytes(16)
def authority(number):
 parts=[b'',number,b'MIT-MAGIC-COOKIE-1',cookie];auth.write_bytes(struct.pack('>H',65535)+b''.join(struct.pack('>H',len(x))+x for x in parts));auth.chmod(0o600)
authority(b'');readfd,writefd=os.pipe();server=None;record={'scope':'Owned isolated Xvfb and native Linux declared-runtime image; no hardware/original claims'}
try:
 with (e/'xvfb.log').open('wb') as log:
  server=subprocess.Popen(['/usr/bin/Xvfb','-displayfd',str(writefd),'-screen','0','1280x800x24','-nolisten','tcp','-auth',str(auth)],pass_fds=(writefd,),stdout=log,stderr=log)
 os.close(writefd);assert select.select([readfd],[],[],10)[0],'Xvfb readiness timeout';number=os.read(readfd,64).strip();assert number.isdigit(),number;os.close(readfd);authority(number)
 env={'HOME':str(home),'PATH':'','DISPLAY':':'+number.decode(),'XAUTHORITY':str(auth),'LANG':'C.UTF-8','LIBGL_ALWAYS_SOFTWARE':'1','XDG_CONFIG_HOME':'/tmp/config','XDG_DATA_HOME':'/tmp/data'}
 spec=importlib.util.spec_from_file_location('startup','/tmp/test-tools/package-check-startup.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
 manifests=list(root.rglob('PACKAGE-MANIFEST.json'));assert len(manifests)==1
 _,manifest=m.verify(root,manifests[0]);assert manifest['profile']=='release';exe=root/'harbor-workspace';helper=root/'termcraft-rdp-engine'
 helpout=m.run([exe,'--help'],Path('/tmp'),env,e/'help.log');assert b'TermCraft remote workspace' in helpout
 packet=m.run([helper],Path('/tmp'),env,e/'helper.log');assert packet[0]==1;end=5+int.from_bytes(packet[1:5],'big');assert json.loads(packet[5:end])=={'Ready':{'version':1}}
 m.run([exe,'--data-dir','/tmp/workspace','--screenshot',e/'startup.png'],Path('/tmp'),env,e/'startup.log',timeout=60)
 m.rdp_session_probe(exe,manifest['files']['termcraft-rdp-engine'],Path('/tmp'),env,e)
 record.update({'status':'passed','display_backend':'X11','files_verified':len(manifest['files']),'help':True,'helper_ready':True,'rdp_initial_negotiation_and_actual_sealed_child':True,'workspace_preserved':True,'private_xauthority':True,'tcp_display_disabled':True})
finally:
 if server is not None:
  if server.poll() is None:server.terminate()
  try:server.wait(5)
  except subprocess.TimeoutExpired:server.kill();server.wait(5)
  record['xvfb_reaped']=server.poll() is not None
 (e/'startup-result.json').write_text(json.dumps(record,indent=2)+'\n')
