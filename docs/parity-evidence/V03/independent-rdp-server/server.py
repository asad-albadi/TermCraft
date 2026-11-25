import os,pathlib,subprocess,socket,threading,select,time,json
B=pathlib.Path('/evidence'); subprocess.run(['useradd','-m','-s','/bin/sh','rdpowned'],check=True)
# Generated password remains inside the container; no authentication can proceed without NLA.
import secrets
subprocess.run(['chpasswd'],input=('rdpowned:'+secrets.token_urlsafe(24)+'\n').encode(),check=True)
subprocess.run(['openssl','req','-x509','-nodes','-newkey','rsa:2048','-days','1','-subj','/CN=owned-xrdp','-keyout','/etc/xrdp/key.pem','-out','/etc/xrdp/cert.pem'],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,check=True)
p=pathlib.Path('/etc/xrdp/xrdp.ini');s=p.read_text().replace('security_layer=negotiate','security_layer=tls');p.write_text(s)
subprocess.run(['mkdir','-p','/run/xrdp']);subprocess.Popen(['xrdp-sesman','--nodaemon'],stdout=open(B/'sesman.log','wb'),stderr=subprocess.STDOUT); 

subprocess.Popen(['xrdp','--nodaemon'],stdout=open(B/'xrdp-stdout.log','wb'),stderr=subprocess.STDOUT)
(B/'versions.txt').write_bytes(subprocess.check_output(['dpkg-query','-W','xrdp','xorgxrdp','xserver-xorg-core']))
(B/'server-net.txt').write_text(os.readlink('/proc/self/ns/net'))
def bridge(a):
 b=socket.create_connection(('127.0.0.1',3389));a.settimeout(10);b.settimeout(10)
 try:
  while True:
   for x in select.select([a,b],[],[],10)[0]:
    data=x.recv(65536)
    if not data:return
    (b if x is a else a).sendall(data)
 finally:a.close();b.close()
sock=socket.socket(socket.AF_UNIX);sock.bind('/evidence/rdp.sock');os.chmod('/evidence/rdp.sock',0o666);sock.listen()
while True:
 a,_=sock.accept();threading.Thread(target=bridge,args=(a,),daemon=True).start()
