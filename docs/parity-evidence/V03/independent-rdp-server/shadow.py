import pathlib,subprocess,os,socket,threading,select,time
B=pathlib.Path('/evidence');os.environ['DISPLAY']=':99'
subprocess.Popen(['Xvfb',':99','-screen','0','800x600x24','-nolisten','tcp'],stdout=open(B/'shadow-xvfb.log','wb'),stderr=subprocess.STDOUT)
time.sleep(1)
subprocess.Popen(['xterm','-geometry','95x30+0+0','-bg','#173b56','-fg','white','-title','Owned independent FreeRDP desktop','-e','sh','-c','echo OWNED_INDEPENDENT_RDP_DESKTOP; exec sh'],stdout=open(B/'shadow-xterm.log','wb'),stderr=subprocess.STDOUT)
pw=(B/'synthetic-password.private').read_text().strip()
# NT hash generated inside container; neither password nor SAM hash written to evidence logs.
nt=subprocess.check_output(['openssl','dgst','-provider','legacy','-md4'],input=pw.encode('utf-16le')).decode().split('= ')[1].strip()
sam=pathlib.Path('/tmp/sam.private');sam.write_text('rdpowned:::'+nt+':::\n');sam.chmod(0o600)
subprocess.Popen(['freerdp-shadow-cli','/port:3389','/bind-address:127.0.0.1','/sec:nla','+auth','/sam-file:'+str(sam)],stdout=open(B/'shadow.log','wb'),stderr=subprocess.STDOUT)
(B/'shadow-versions.txt').write_bytes(subprocess.check_output(['dpkg-query','-W','freerdp2-shadow-x11','libfreerdp2-2','xvfb']))
sock=socket.socket(socket.AF_UNIX);sock.bind('/evidence/shadow.sock');os.chmod('/evidence/shadow.sock',0o666);sock.listen()
def bridge(a):
 b=socket.create_connection(('127.0.0.1',3389))
 try:
  while True:
   for x in select.select([a,b],[],[],60)[0]:
    data=x.recv(65536)
    if not data:return
    (b if x is a else a).sendall(data)
 finally:a.close();b.close()
while True:
 a,_=sock.accept();threading.Thread(target=bridge,args=(a,),daemon=True).start()
