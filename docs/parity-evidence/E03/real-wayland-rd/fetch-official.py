from pathlib import Path
import subprocess,urllib.request,json,sys,hashlib,concurrent.futures
B=Path(__file__).resolve().parent;APP=B.parents[2]
def fetch(name):
 d=json.load(urllib.request.urlopen('https://archlinux.org/packages/extra/x86_64/'+name+'/json/',timeout=20));fn=d['filename']; (B/'inputs'/(name+'-package.json')).write_text(json.dumps(d,indent=2))
 for suffix in ['', '.sig']:
  dest=B/'inputs'/(fn+suffix)
  if not dest.exists():dest.write_bytes(urllib.request.urlopen('https://fastly.mirror.pkgbuild.com/extra/os/x86_64/'+fn+suffix,timeout=20).read())
 p=B/'inputs'/fn
 subprocess.run(['gpgv','--homedir',str(APP/'target/wayland-real-portal-inputs/gnupg'),'--keyring',str(APP/'target/wayland-real-portal-inputs/archlinux-keyring.gpg'),str(p)+'.sig',str(p)],check=True,stdout=open(B/(name+'-signature.log'),'w'),stderr=subprocess.STDOUT)
 subprocess.run(['tar','--zstd','-xf',str(p),'-C',str(B/'tools')],check=True)
 print(name,fn,hashlib.sha256(p.read_bytes()).hexdigest(),flush=True)
for name in sys.argv[1:]:fetch(name)
