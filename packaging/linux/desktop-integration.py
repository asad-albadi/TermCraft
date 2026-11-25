#!/usr/bin/env python3
"""Install, remove or inspect owned per-user TermCraft launch integration; never changes defaults."""
import argparse
from contextlib import contextmanager
import fcntl
import stat
import hashlib
import json
import os
from pathlib import Path
import shutil
import tempfile
import subprocess

APP = 'org.termcraft.TermCraft'

def digest(content):
    return hashlib.sha256(content).hexdigest()

def desktop_value(value):
    if any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise ValueError('Desktop paths cannot contain control characters.')
    return value.replace('\\', '\\\\')

def argument(value):
    # Desktop entry string escaping happens before Exec tokenization. No shell executes.
    quoted = '"' + ''.join('\\' + c if c in '\\"$`' else '%%' if c == '%' else c for c in value) + '"'
    return desktop_value(quoted)

def entry_for(root):
    launcher = argument(str(root / 'launch.py'))
    return (
        '[Desktop Entry]\nType=Application\nVersion=1.0\nName=TermCraft\n'
        'Comment=Remote workspace and terminal\n'
        f'Exec=/usr/bin/python3 {launcher} --open-url %u\nIcon={APP}\n'
        'Terminal=false\nCategories=System;TerminalEmulator;Network;\n'
        'MimeType=x-scheme-handler/termcraft;application/x-termcraft-link;inode/directory;\n'
        'Actions=OpenHere;\n\n[Desktop Action OpenHere]\nName=Open terminal here\n'
        f'Exec=/usr/bin/python3 {launcher} --cwd %f\n'
    ).encode()

class DesktopIntegration:
    def __init__(self, data, package):
        self.data = Path(data)
        self.package = Path(package).resolve()
        if not self.data.is_absolute():
            raise ValueError('XDG_DATA_HOME must be absolute.')
        desktop_value(str(self.data))
        self.root = self.data / 'termcraft/desktop'
        self.manifest = self.root / 'owned.json'

    def _owned(self):
        if self.manifest.is_symlink(): raise ValueError('Ownership manifest is a symbolic link.')
        if not self.manifest.exists(): return {}
        value = json.loads(self.manifest.read_text())
        if value.get('owner') != APP or value.get('version') != 1: raise ValueError('Unknown desktop integration owner.')
        result = value['files']
        for name, checksum in result.items():
            path = Path(name)
            allowed = name in {f'applications/{APP}.desktop', f'icons/hicolor/256x256/apps/{APP}.png', 'mime/packages/org.termcraft.TermCraft.xml', 'termcraft/desktop/launch.py'} or (name.startswith('termcraft/desktop/payload/') and len(path.parts)>3)
            if not allowed: raise ValueError('Ownership record names an unrelated file.')
            if path.is_absolute() or '..' in path.parts or not isinstance(checksum, str): raise ValueError('Invalid ownership record.')
        return result

    def _safe_path(self, name):
        path = self.data / name
        for parent in (path, *path.parents):
            if parent.is_symlink(): raise ValueError(f'Refusing symbolic link: {parent}')
            if parent == self.data: break
        return path

    def _verify(self, owned):
        for name, checksum in owned.items():
            path = self._safe_path(name)
            if path.exists() and (not path.is_file() or digest(path.read_bytes()) != checksum):
                raise ValueError(f'Owned integration file was changed: {path}')

    @contextmanager
    def _locked(self):
        # Keep the lock inode after uninstall: unlinking it would let two installers
        # hold different locks for the same data root. No user file bytes are changed.
        lock=self._safe_path('termcraft/desktop/.install.lock')
        self.root.mkdir(parents=True,exist_ok=True)
        descriptor=os.open(lock,os.O_CREAT | os.O_RDWR | os.O_NOFOLLOW | os.O_CLOEXEC,0o600)
        try:
            if not stat.S_ISREG(os.fstat(descriptor).st_mode):
                raise ValueError('Integration lock must be a regular file.')
            fcntl.flock(descriptor,fcntl.LOCK_EX)
            yield
        finally:
            os.close(descriptor)

    def status(self):
        with self._locked():
            return self._status()

    def _status(self):
        owned = self._owned()
        self._verify(owned)
        return bool(owned) and all(self._safe_path(name).is_file() for name in owned)

    def install_user(self):
        with self._locked():
            self._install_user()

    def _install_user(self):
        owned = self._owned()
        self._verify(owned)
        files = {}
        # Keep a private installed payload: unpacked releases can move or disappear safely.
        # Include runtime/helper manifests beside their original binaries and preserve layout.
        for source in sorted(self.package.rglob('*')):
            if source.is_symlink(): raise ValueError(f'Package contains a symbolic link: {source}')
            if source.is_file():
                relative = source.relative_to(self.package)
                files[str(Path('termcraft/desktop/payload') / relative)] = (source.read_bytes(), source.stat().st_mode & 0o777)
        if 'termcraft/desktop/payload/harbor-workspace' not in files: raise ValueError('Package has no application binary.')
        icon = self.package / 'assets/termcraft-logo.png'
        files[f'icons/hicolor/256x256/apps/{APP}.png'] = (icon.read_bytes(), 0o644)
        launch = b'''import os,sys\nfrom pathlib import Path\np=Path(__file__).resolve().parent/'payload/harbor-workspace'\na=sys.argv[1:]\nif a==['--open-url']: a=[]\nif a==['--cwd']: a=['--new-tab']\nif len(a)==2 and a[0]=='--open-url' and not a[1].startswith('termcraft:'):\n from urllib.parse import urlsplit,unquote\n u=urlsplit(a[1])\n if u.scheme=='file' and u.netloc in ('','localhost') and not u.query and not u.fragment: a=['--cwd' if Path(unquote(u.path,errors='strict')).is_dir() else '--open-file',unquote(u.path,errors='strict')]\n elif not u.scheme and Path(a[1]).is_absolute(): a=['--cwd' if Path(a[1]).is_dir() else '--open-file',a[1]]\n else: raise SystemExit('Unsupported desktop file URL')\nos.execv(str(p),[str(p),*a])\n'''
        files['termcraft/desktop/launch.py'] = (launch,0o644)
        files[f'applications/{APP}.desktop'] = (entry_for(self.root),0o644)
        files['mime/packages/org.termcraft.TermCraft.xml'] = (b'<?xml version="1.0"?><mime-info xmlns="http://www.freedesktop.org/standards/shared-mime-info"><mime-type type="application/x-termcraft-link"><comment>TermCraft bookmark</comment><glob pattern="*.termcraft-link"/></mime-type></mime-info>',0o644)
        # Preflight every target and ancestor before the first installed byte changes.
        for name in files.keys() | owned.keys():
            path = self._safe_path(name)
            for parent in path.parents:
                if parent.exists() and not parent.is_dir():
                    raise NotADirectoryError(f'Integration directory is a regular file: {parent}')
                if parent == self.data: break
            if path.exists() and name not in owned:
                raise ValueError(f'Existing file is not owned by TermCraft: {path}')
        self.root.mkdir(parents=True,exist_ok=True)
        stage = Path(tempfile.mkdtemp(prefix='.stage-',dir=self.root))
        changes = []
        retain_backups = False
        try:
            # Stage the receipt as well: preparation failures cannot alter the old release.
            for index,(name,(content,mode)) in enumerate(files.items()):
                staged=stage/str(index);staged.write_bytes(content);staged.chmod(mode)
            receipt=stage/'manifest'
            receipt.write_text(json.dumps({'owner':APP,'version':1,'files':{name:digest(value[0]) for name,value in files.items()}},indent=2))
            try:
                targets = list(files) + sorted(owned.keys()-files.keys())
                for index,name in enumerate(targets):
                    path=self._safe_path(name)
                    path.parent.mkdir(parents=True,exist_ok=True)
                    backup=None
                    if path.exists():
                        # Recheck ownership after preparation; never back up a user's replacement.
                        if name not in owned or not path.is_file() or digest(path.read_bytes()) != owned[name]:
                            raise ValueError(f'Integration target changed during installation: {path}')
                        backup=stage/f'backup-{index}'
                        os.replace(path,backup)
                    change={'path':path,'backup':backup,'installed':None}
                    changes.append(change)
                    if name in files:
                        os.replace(stage/str(index),path)
                        change['installed']=digest(files[name][0])
                # Publish ownership only after all new files and obsolete-file removals succeed.
                os.replace(receipt,self.manifest)
            except BaseException as error:
                rollback_errors=[]
                for change in reversed(changes):
                    path=change['path']
                    try:
                        self._safe_path(str(path.relative_to(self.data)))
                        if path.exists():
                            # Concurrent user edits must survive even a failed transaction.
                            if change['installed'] is None or not path.is_file() or digest(path.read_bytes()) != change['installed']:
                                raise ValueError(f'Refusing rollback over a changed file: {path}')
                            if change['backup'] is None:
                                path.unlink()
                        if change['backup'] is not None:
                            os.replace(change['backup'],path)
                    except BaseException as rollback_error:
                        rollback_errors.append(str(rollback_error))
                if rollback_errors:
                    # Do not erase the only surviving copies if the filesystem also refuses rollback.
                    retain_backups=True
                    raise RuntimeError(f'Installation failed and rollback needs recovery; preserved backups in {stage}: {rollback_errors}') from error
                raise
        finally:
            if not retain_backups:
                shutil.rmtree(stage)
        self._refresh()

    def _refresh(self):
        for tool, directory in (("update-desktop-database", "applications"), ("update-mime-database", "mime")):
            executable=shutil.which(tool)
            if executable: subprocess.run([executable,str(self.data/directory)],check=True,stdout=subprocess.DEVNULL,stderr=subprocess.PIPE)

    def remove_user(self):
        with self._locked():
            self._remove_user()

    def _remove_user(self):
        owned=self._owned();self._verify(owned)
        for name in owned: self._safe_path(name).unlink(missing_ok=True)
        self.manifest.unlink(missing_ok=True)
        if owned: self._refresh()
        # Leave directories and any unowned content in place. Never touch mimeapps.list.

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action',choices=('install','uninstall','status'))
    args=parser.parse_args()
    data=Path(os.environ.get('XDG_DATA_HOME') or Path.home()/'.local/share')
    try:
        integration=DesktopIntegration(data,Path(__file__).resolve().parent)
        if args.action=='install': integration.install_user()
        elif args.action=='uninstall': integration.remove_user()
        else: print('installed' if integration.status() else 'not installed')
    except (OSError,ValueError,KeyError,TypeError,subprocess.CalledProcessError,RuntimeError) as error: parser.exit(1,f'{error}\n')
if __name__=='__main__': main()
