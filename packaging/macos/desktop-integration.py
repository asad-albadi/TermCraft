#!/usr/bin/env python3
"""Register this TermCraft bundle with LaunchServices without choosing user defaults."""
import argparse
import json
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
REGISTER='/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister'
OWNER='org.termcraft.TermCraft'
class DesktopIntegration:
    def __init__(self,bundle,receipt):
        self.bundle=Path(bundle).resolve();self.receipt=Path(receipt)
        info=plistlib.loads((self.bundle/'Contents/Info.plist').read_bytes())
        if info.get('CFBundleIdentifier')!=OWNER or info.get('CFBundleExecutable')!='termcraft-desktop-launcher': raise ValueError('Select a complete TermCraft application bundle.')
        if not (self.bundle/'Contents/MacOS/termcraft-desktop-launcher').is_file(): raise ValueError('Native desktop event launcher is missing.')
    def _owned(self):
        if self.receipt.is_symlink(): raise ValueError('Registration receipt is a symbolic link.')
        if not self.receipt.exists(): return []
        value=json.loads(self.receipt.read_text())
        if value.get('owner')!=OWNER or value.get('version')!=1 or not isinstance(value.get('bundles'),list): raise ValueError('Unknown registration receipt owner.')
        paths=value['bundles']
        if any(not isinstance(path,str) or not Path(path).is_absolute() or Path(path).suffix!='.app' for path in paths): raise ValueError('Invalid owned bundle path.')
        return paths
    def _run(self,*args):
        if sys.platform!='darwin': raise ValueError('Native macOS LaunchServices is unavailable on this host.')
        return subprocess.run([REGISTER,*args],check=True,text=True,capture_output=True).stdout
    def status(self):
        if str(self.bundle) not in self._owned(): return False
        dump=self._run('-dump')
        return any(line.partition(':')[0].strip()=='path' and line.partition(':')[2].strip()==str(self.bundle) for line in dump.splitlines())
    def install_user(self):
        owned=self._owned();self._run('-f',str(self.bundle));self.receipt.parent.mkdir(parents=True,exist_ok=True)
        if str(self.bundle) not in owned: owned.append(str(self.bundle))
        with tempfile.NamedTemporaryFile(mode='w',dir=self.receipt.parent,delete=False) as output:
            json.dump({'owner':OWNER,'bundles':owned,'version':1},output);temporary=output.name
        os.replace(temporary,self.receipt)
    def remove_user(self):
        owned=self._owned()
        # A moved/replaced release may have registered more than one owned path.
        # Refuse to unregister any path now occupied by an unrelated bundle.
        for name in owned:
            path=Path(name)
            if path.exists():
                info=plistlib.loads((path/'Contents/Info.plist').read_bytes())
                if (info.get('CFBundleIdentifier')!=OWNER or
                    info.get('CFBundleExecutable')!='termcraft-desktop-launcher'):
                    raise ValueError('An owned registration path now contains a different application.')
        for name in owned: self._run('-u',name)
        if owned:self.receipt.unlink()
def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('action',choices=['install','uninstall','status']);args=parser.parse_args()
    try:
        root=Path(__file__).resolve().parents[2]
        receipt=Path.home()/'Library/Application Support/TermCraft/desktop-registration.json'
        integration=DesktopIntegration(root,receipt)
        if args.action=='install':integration.install_user()
        elif args.action=='uninstall':integration.remove_user()
        else:print('installed' if integration.status() else 'not installed')
    except (OSError,ValueError,subprocess.CalledProcessError) as error:parser.exit(1,f'{error}\n')
if __name__=='__main__':main()
