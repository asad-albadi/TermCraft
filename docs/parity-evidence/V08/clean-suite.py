#!/usr/bin/env python3
"""Replay bounded V08 scenarios in the recorded owned images; only resets owned fixture directories."""
from pathlib import Path
import json,subprocess,sys,shutil
root=Path(__file__).resolve().parents[3];evidence=Path(__file__).parent;out=evidence/'clean-run'
input_image='sha256:bf83b781e2d467fe7cbafb6c4235c3263260fd56cec7cfb9b22dafde0a914a12'
runtime_image='sha256:1ab61a2bb16febd0a86829a43c26849b06ca82aca31be8385880cc7a3570c79d'
for image in [input_image,runtime_image]:subprocess.run(['docker','image','inspect',image],check=True,stdout=subprocess.DEVNULL)
for directory in ['fresh-final','upgrade','corrupt','absent','absent-bundled','browser']:
 p=out/directory
 if p.exists():shutil.rmtree(p)
 p.mkdir()
(out/'pty-marker.txt').unlink(missing_ok=True)
shutil.copyfile(out/'upgrade-input-store.json',out/'upgrade/workspace.json')
(out/'corrupt/workspace.json').write_bytes((out/'corrupt-input.bin').read_bytes())
for name,session in [('absent',{'name':'Absent installed Mosh','protocol':'Mosh','host':'127.0.0.1','user':'fixture','mosh_provider':'system'}),('absent-bundled',{'name':'Absent bundled Mosh dependency','protocol':'Mosh','host':'127.0.0.1','user':'fixture','mosh_provider':'bundled'}),('browser',{'name':'Clean bundled browser','protocol':'Browser','host':'http://127.0.0.1:25380','browser_options':{'embedded':True,'runtime':'BundledChromium'}})]:
 session['id']='0123456789abcdef0123456789abcdef';(out/name/'workspace.json').write_text(json.dumps({'sessions':[session]},indent=2)+'\n')
old=next((root/'target/package-interop-xdmcp-devices-qualified/b0d1f602e02a06050afa4196b3f4e6f6c5e3b1b370ddb741b01295a1d09c1a98').iterdir())
scenarios=[('fresh-install-save-restart-pty','fresh-final.sh'),('legacy-save','legacy.sh'),('upgrade-current','current-upgrade.sh'),('corrupt-save-restart-final','corrupt.sh'),('absent-installed-mosh','absent.sh'),('absent-bundled-mosh-dependency','absent-bundled.sh'),('direct-iperf-byte-closure','iperf.sh'),('direct-unix-byte-closure-exec-tmp','unix.sh'),('per-user-install-startup-uninstall','install.sh'),('clean-bundled-browser','browser.sh'),('input-image-inventory','inventory.sh'),('clean-declared-browser-runtime','browser-runtime.sh'),('runtime-image-inventory','runtime-inventory.sh')]
for name,script in scenarios:
 command=[sys.executable,str(evidence/'clean-runner.py'),name,str(evidence/'clean-scenarios'/script),'--image',runtime_image if name in ['clean-declared-browser-runtime','runtime-image-inventory'] else input_image]
 if name=='legacy-save':command+=['--package',str(old)]
 subprocess.run(command,cwd=root,check=True)
subprocess.run([sys.executable,str(evidence/'clean-analyze.py')],cwd=root,check=True)
