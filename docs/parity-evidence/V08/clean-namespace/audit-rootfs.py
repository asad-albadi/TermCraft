#!/usr/bin/env python3
"""Supplement export extraction with full resolved-link and file-content audit."""
import hashlib,json,posixpath,tarfile
from pathlib import Path
base=Path(__file__).resolve().parents[4];out=Path(__file__).resolve().parent
scratch=base/'target/validation/v08-clean-namespace';root=(scratch/'rootfs').resolve();checked=0;links=0
with tarfile.open(scratch/'owned-rootfs.tar') as tf:
 members=tf.getmembers();linknames={posixpath.normpath(m.name) for m in members if m.issym() or m.islnk()}
 for m in members:
  p=Path(m.name)
  if any(str(a) in linknames for a in p.parents if str(a)!='.'):raise RuntimeError('archive member below link '+m.name)
  dest=root/m.name
  if not dest.resolve().is_relative_to(root):raise RuntimeError('resolved escape '+m.name)
  if m.isfile():
   if hashlib.file_digest(tf.extractfile(m),'sha256').digest()!=hashlib.file_digest(dest.open('rb'),'sha256').digest():raise RuntimeError('file mismatch '+m.name)
   checked+=1
  if m.issym() or m.islnk():links+=1
for p in root.rglob('*'):
 if p.is_symlink() and not p.resolve().is_relative_to(root):raise RuntimeError('filesystem link escape '+str(p))
result={'export_regular_files_content_verified':checked,'archive_links_resolved_within_root':links,'archive_entries_beneath_link_ancestors':0,'escaping_resolved_filesystem_links':0,'added_empty_mountpoint_directories':['/opt/termcraft','/evidence']}
(out/'rootfs-audit.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
