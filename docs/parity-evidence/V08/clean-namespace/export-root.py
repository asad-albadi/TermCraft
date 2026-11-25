#!/usr/bin/env python3
"""Export only a newly owned container; safely extract regular files and contained links."""
import hashlib,json,posixpath,subprocess,tarfile,uuid,os
from pathlib import Path
base=Path(__file__).resolve().parents[4]; out=Path(__file__).resolve().parent
scratch=base/'target/validation/v08-clean-namespace'; root=scratch/'rootfs'; archive=scratch/'owned-rootfs.tar'
image='sha256:1ab61a2bb16febd0a86829a43c26849b06ca82aca31be8385880cc7a3570c79d'
name='termcraft-v08-clean-ns-'+uuid.uuid4().hex[:12]
record={'image':image,'owned_container':name}
try:
 record['container_id']=subprocess.check_output(['docker','create','--name',name,'--label','org.termcraft.validation=v08-clean-namespace','--network','none',image,'/usr/bin/true'],text=True).strip()
 subprocess.run(['docker','export','--output',str(archive),name],check=True)
finally:
 subprocess.run(['docker','rm','-f',name],check=True,stdout=subprocess.DEVNULL);record['owned_container_removed']=True
record['export_sha256']=hashlib.file_digest(archive.open('rb'),'sha256').hexdigest()
if root.exists(): raise RuntimeError('Refusing existing root')
root.mkdir(); skipped=[]; count=0
with tarfile.open(archive) as tf:
 members=tf.getmembers(); names=set()
 for m in members:
  n=posixpath.normpath(m.name)
  if m.name.startswith('/') or n=='..' or n.startswith('../'): raise RuntimeError('unsafe member '+m.name)
  if n in names: raise RuntimeError('duplicate member '+n)
  names.add(n)
  if m.issym() or m.islnk():
   target=posixpath.normpath(posixpath.join(posixpath.dirname(n),m.linkname) if m.issym() and not m.linkname.startswith('/') else m.linkname.lstrip('/'))
   if target=='..' or target.startswith('../'): raise RuntimeError('unsafe link '+n)
 # Files and directories first. No archive-created links can redirect extraction.
 for m in members:
  p=root/m.name
  if m.isdir(): p.mkdir(parents=True,exist_ok=True);p.chmod(m.mode&0o777)
  elif m.isfile():
   p.parent.mkdir(parents=True,exist_ok=True)
   with tf.extractfile(m) as src,p.open('xb') as dst:
    import shutil;shutil.copyfileobj(src,dst)
   p.chmod(m.mode&0o777);count+=1
  elif not (m.issym() or m.islnk()): skipped.append(m.name)
 for m in members:
  if m.islnk():
   p=root/m.name; target=root/posixpath.normpath(m.linkname.lstrip('/'))
   if not target.is_file() or target.is_symlink(): raise RuntimeError('invalid hardlink '+m.name)
   os.link(target,p)
 for m in members:
  if m.issym():
   p=root/m.name;p.parent.mkdir(parents=True,exist_ok=True)
   # Rewrite absolute rootfs links to equivalent contained relative paths.
   target=posixpath.relpath(m.linkname.lstrip('/'),posixpath.dirname(m.name) or '.') if m.linkname.startswith('/') else m.linkname
   p.symlink_to(target)
record.update(extracted_regular_files=count,skipped_special_files=skipped,rootfs=str(root),ownership='extractor UID/GID; setuid/setgid stripped; special files skipped; absolute symlinks rewritten within root')
(out/'export-provenance.json').write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
