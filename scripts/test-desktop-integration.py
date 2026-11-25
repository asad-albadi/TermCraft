import importlib.util, tempfile, unittest, pathlib, os, subprocess, json, shutil
SOURCE=pathlib.Path(__file__).resolve().parents[1]/'packaging/linux/desktop-integration.py'
spec=importlib.util.spec_from_file_location('desktop',SOURCE); desktop=importlib.util.module_from_spec(spec);spec.loader.exec_module(desktop)
class DesktopTests(unittest.TestCase):
 def test_integration_removal_keeps_user_defaults(self):
  with tempfile.TemporaryDirectory() as t:
   base=pathlib.Path(t); package=base/'package';package.mkdir();(package/'harbor-workspace').write_text('binary');(package/'assets').mkdir();(package/'assets/termcraft-logo.png').write_bytes(b'png')
   data=base/'data';data.mkdir(); defaults=data/'mimeapps.list';defaults.write_text('[Default Applications]\nx-scheme-handler/termcraft=mine.desktop;\n'); original=defaults.read_bytes()
   integration=desktop.DesktopIntegration(data,package); integration.install_user();self.assertTrue(integration.status())
   (package/'harbor-workspace').write_text('upgraded binary');integration.install_user()
   self.assertEqual((data/'termcraft/desktop/payload/harbor-workspace').read_text(),'upgraded binary')
   extra=data/'termcraft/desktop/user-note';extra.write_text('keep')
   integration.remove_user();self.assertFalse(integration.status()); self.assertEqual(defaults.read_bytes(),original);self.assertEqual(extra.read_text(),'keep')
 def test_external_launcher_survives_relocation_and_preserves_literal_argv(self):
  if not shutil.which('gio'): self.skipTest('gio unavailable')
  with tempfile.TemporaryDirectory() as t:
   base=pathlib.Path(t);package=base/'package with % $(literal)';package.mkdir();(package/'assets').mkdir();(package/'assets/termcraft-logo.png').write_bytes(b'png')
   binary=package/'harbor-workspace';binary.write_text('#!/usr/bin/python3\nimport os,sys,json\nopen(os.environ["ARGUMENT_CAPTURE"],"w").write(json.dumps(sys.argv[1:]))\n');binary.chmod(0o755)
   data=base/'data with % spaces';integration=desktop.DesktopIntegration(data,package);integration.install_user();shutil.rmtree(package)
   capture=base/'capture'; value='termcraft://session/a%20b%3B%24%28touch%20pwned%29'
   env=dict(os.environ,ARGUMENT_CAPTURE=str(capture),XDG_DATA_HOME=str(data))
   subprocess.run(['gio','launch',str(data/'applications/org.termcraft.TermCraft.desktop'),value],env=env,check=True,capture_output=True)
   import time
   for _ in range(100):
    if capture.exists():break
    time.sleep(.02)
   self.assertEqual(json.loads(capture.read_text()),['--open-url',value]);self.assertTrue((data/'icons/hicolor/256x256/apps/org.termcraft.TermCraft.png').is_file())
   for flag, selected in [('--cwd',base/'folder % $(literal)'),('--open-file',base/'a % bookmark.termcraft-link')]:
    if flag=='--cwd': selected.mkdir()
    else: selected.write_text('{"version":1,"session":"bookmark"}')
    capture.unlink()
    subprocess.run(['gio','launch',str(data/'applications/org.termcraft.TermCraft.desktop'),selected.as_uri()],env=env,check=True,capture_output=True)
    for _ in range(100):
     if capture.exists():break
     time.sleep(.02)
    self.assertEqual(json.loads(capture.read_text()),[flag,str(selected)])
 def test_modified_owned_file_blocks_removal(self):
  with tempfile.TemporaryDirectory() as t:
   base=pathlib.Path(t);package=base/'package';package.mkdir();(package/'harbor-workspace').write_text('binary');(package/'assets').mkdir();(package/'assets/termcraft-logo.png').write_bytes(b'png');data=base/'data'
   integration=desktop.DesktopIntegration(data,package);integration.install_user();menu=data/'applications/org.termcraft.TermCraft.desktop';menu.write_text('user replacement')
   with self.assertRaises(ValueError):integration.remove_user()
   self.assertEqual(menu.read_text(),'user replacement')
class InstallFailureTests(unittest.TestCase):
 def package(self,root,version):
  package=root/'package';package.mkdir();(package/'assets').mkdir();(package/'assets/termcraft-logo.png').write_bytes(b'png')
  program=package/'harbor-workspace';program.write_text('#!/usr/bin/python3\nimport os\nopen(os.environ["ARGUMENT_CAPTURE"],"w").write('+repr(version)+')\n');program.chmod(0o755)
  return package
 def test_first_install_directory_conflict_does_not_strand_payload_and_can_retry(self):
  with tempfile.TemporaryDirectory() as t:
   root=pathlib.Path(t);package=self.package(root,'old');data=root/'data';data.mkdir()
   conflict=data/'applications';conflict.write_bytes(b'user-owned file');defaults=data/'mimeapps.list';defaults.write_bytes(b'user defaults')
   integration=desktop.DesktopIntegration(data,package)
   with self.assertRaises(OSError):integration.install_user()
   self.assertEqual(conflict.read_bytes(),b'user-owned file');self.assertEqual(defaults.read_bytes(),b'user defaults')
   self.assertFalse((data/'termcraft/desktop/payload/harbor-workspace').exists(),'failed first install stranded unowned application bytes')
   self.assertFalse(integration.manifest.exists());conflict.unlink()
   integration.install_user();self.assertTrue(integration.status());integration.remove_user();self.assertFalse(integration.status())
   self.assertEqual(defaults.read_bytes(),b'user defaults')
 def test_upgrade_receipt_failure_restores_old_release_then_can_retry_and_remove(self):
  from unittest.mock import patch
  import sys
  for recovery in ('retry','remove'):
   with self.subTest(recovery=recovery):
    with tempfile.TemporaryDirectory() as t:
     root=pathlib.Path(t);package=self.package(root,'old');(package/'obsolete-owned').write_bytes(b'old helper')
     data=root/'data';integration=desktop.DesktopIntegration(data,package);integration.install_user()
     original={name:(data/name).read_bytes() for name in integration._owned()};receipt=integration.manifest.read_bytes()
     note=data/'termcraft/desktop/user-note';note.write_bytes(b'keep note');defaults=data/'mimeapps.list';defaults.write_bytes(b'keep defaults')
     program=package/'harbor-workspace';program.write_text(program.read_text().replace("'old'","'new'"));(package/'obsolete-owned').unlink();(package/'new-helper').write_bytes(b'new helper')
     real_replace=desktop.os.replace
     def fail_receipt(source,destination):
      if pathlib.Path(destination)==integration.manifest:raise OSError('injected receipt publish failure')
      return real_replace(source,destination)
     with patch.object(desktop.os,'replace',side_effect=fail_receipt):
      with self.assertRaisesRegex(OSError,'injected receipt'):integration.install_user()
     self.assertEqual((data/'termcraft/desktop/payload/harbor-workspace').read_bytes(),original['termcraft/desktop/payload/harbor-workspace'],'failed upgrade left new executable behind')
     self.assertEqual(integration.manifest.read_bytes(),receipt)
     for name,content in original.items():self.assertEqual((data/name).read_bytes(),content,name)
     self.assertFalse((data/'termcraft/desktop/payload/new-helper').exists());self.assertTrue(integration.status())
     capture=root/'capture';subprocess.run([sys.executable,str(data/'termcraft/desktop/launch.py')],env=dict(os.environ,ARGUMENT_CAPTURE=str(capture)),check=True)
     self.assertEqual(capture.read_text(),'old');self.assertEqual(note.read_bytes(),b'keep note');self.assertEqual(defaults.read_bytes(),b'keep defaults')
     if recovery=='remove':
      integration.remove_user();self.assertFalse(integration.status());self.assertEqual(note.read_bytes(),b'keep note');self.assertEqual(defaults.read_bytes(),b'keep defaults')
     integration.install_user();self.assertTrue(integration.status());subprocess.run([sys.executable,str(data/'termcraft/desktop/launch.py')],env=dict(os.environ,ARGUMENT_CAPTURE=str(capture)),check=True);self.assertEqual(capture.read_text(),'new')
     integration.remove_user();self.assertFalse(integration.status());self.assertEqual(note.read_bytes(),b'keep note');self.assertEqual(defaults.read_bytes(),b'keep defaults')
 def test_simultaneous_installer_waits_for_failed_transaction_before_upgrading(self):
  import sys,time
  code = r"""
import importlib.util,sys,pathlib,time,os
source,data,package,mode,ready,release=sys.argv[1:]
spec=importlib.util.spec_from_file_location('integration',source);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
integration=module.DesktopIntegration(pathlib.Path(data),pathlib.Path(package))
if mode=='fail':
 real_replace=module.os.replace
 def replace(source,destination):
  if pathlib.Path(destination)==integration.manifest:
   pathlib.Path(ready).write_text('waiting at receipt')
   deadline=time.monotonic()+10
   while not pathlib.Path(release).exists():
    if time.monotonic()>deadline:raise RuntimeError('test release deadline exceeded')
    time.sleep(.01)
   raise OSError('injected receipt publish failure')
  return real_replace(source,destination)
 module.os.replace=replace
 try:integration.install_user()
 except OSError:sys.exit(0)
 raise RuntimeError('expected receipt failure')
integration.install_user()
"""
  with tempfile.TemporaryDirectory() as t:
   root=pathlib.Path(t);package=self.package(root,'old');data=root/'data';integration=desktop.DesktopIntegration(data,package);integration.install_user()
   program=package/'harbor-workspace';program.write_text(program.read_text().replace("'old'","'first'"))
   second_package=root/'second-package';shutil.copytree(package,second_package);program2=second_package/'harbor-workspace';program2.write_text(program2.read_text().replace("'first'","'second'"))
   ready=root/'ready';release=root/'release';first=None;second=None
   try:
    first=subprocess.Popen([sys.executable,'-c',code,str(SOURCE),str(data),str(package),'fail',str(ready),str(release)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    deadline=time.monotonic()+5
    while not ready.exists():
     if time.monotonic()>deadline:self.fail('first installer did not reach receipt publication')
     time.sleep(.01)
    second=subprocess.Popen([sys.executable,'-c',code,str(SOURCE),str(data),str(second_package),'normal',str(ready),str(release)],stdout=subprocess.PIPE,stderr=subprocess.PIPE)
    with self.assertRaises(subprocess.TimeoutExpired,msg='second installer did not wait for the first transaction'):second.wait(timeout=.3)
   finally:
    release.touch()
    if first: first_result=first.communicate(timeout=10)
    if second: second_result=second.communicate(timeout=10)
   self.assertEqual(first.returncode,0,first_result);self.assertEqual(second.returncode,0,second_result)
   self.assertTrue(integration.status());self.assertIn("'second'",(data/'termcraft/desktop/payload/harbor-workspace').read_text());integration.remove_user()
class MacSourceChecks(unittest.TestCase):
 def test_native_events_and_services_are_connected_to_literal_argv(self):
  import plistlib
  root=SOURCE.parents[2]; info=plistlib.loads((root/'packaging/macos/Info.plist').read_bytes()); source=(root/'packaging/macos/desktop-launcher.m').read_text()
  self.assertEqual(info['CFBundleExecutable'],'termcraft-desktop-launcher')
  self.assertEqual(info['CFBundleURLTypes'][0]['CFBundleURLSchemes'],['termcraft'])
  self.assertEqual(info['CFBundleDocumentTypes'][0]['LSHandlerRank'],'Alternate')
  self.assertIn('openURLs:',source);self.assertIn('task.arguments = arguments',source)
  self.assertIn('application.servicesProvider = delegate',source)
  self.assertEqual(info['NSServices'][0]['NSMessage'],'openTerminalHere')
  self.assertIn('openTerminalHere:(NSPasteboard *)pasteboard',source)
  self.assertIn('arrayByAddingObjectsFromArray',source)
  self.assertNotIn('system(',source)
 def test_native_registration_is_unavailable_on_linux(self):
  if os.uname().sysname=='Darwin':self.skipTest('Linux-only unavailable assertion')
  import plistlib
  script=SOURCE.parents[2]/'packaging/macos/desktop-integration.py'
  spec=importlib.util.spec_from_file_location('macdesktop',script);module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
  with tempfile.TemporaryDirectory() as t:
   root=pathlib.Path(t); bundle=root/'TermCraft.app'; executable=bundle/'Contents/MacOS/termcraft-desktop-launcher';executable.parent.mkdir(parents=True);executable.write_bytes(b'fixture')
   (bundle/'Contents/Info.plist').write_bytes(plistlib.dumps({'CFBundleIdentifier':'org.termcraft.TermCraft','CFBundleExecutable':'termcraft-desktop-launcher'}))
   integration=module.DesktopIntegration(bundle,root/'receipt')
   with self.assertRaisesRegex(ValueError,'unavailable'):integration.install_user()
   self.assertFalse((root/'receipt').exists())
if __name__=='__main__':unittest.main()
