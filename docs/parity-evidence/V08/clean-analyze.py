#!/usr/bin/env python3
"""Recheck exact immutable bytes and clean evidence without modifying product files."""
from pathlib import Path
import json,hashlib,importlib.util,subprocess
root=Path(__file__).resolve().parents[3];out=root/'docs/parity-evidence/V08/clean-run'
sha=lambda p:hashlib.sha256(p.read_bytes()).hexdigest()
packages=[]
for folder,digest,archive in [('package-games-v03-qualified','68f356f63c92a9a6aced6abe3d845ab9d064e5da0d37b52ff0c4c99f0fc4b117',root/'target/package-games-v03-reviewed/TermCraft-0.1.0-linux-x86_64-arch-xwayland-xdmcp-mosh-unix-chromium-wayland-sharing.tar.gz'),('package-interop-xdmcp-devices-qualified','b0d1f602e02a06050afa4196b3f4e6f6c5e3b1b370ddb741b01295a1d09c1a98',next((root/'dist/snapshots/b0d1f602e02a06050afa4196b3f4e6f6c5e3b1b370ddb741b01295a1d09c1a98').glob('*.tar.gz')))]:
 assert sha(archive)==digest
 package=next((root/'target'/folder/digest).iterdir());manifest=json.loads((package/'PACKAGE-MANIFEST.json').read_text())
 actual={p.relative_to(package).as_posix():sha(p) for p in package.rglob('*') if p.is_file() and p.name!='PACKAGE-MANIFEST.json'}
 assert actual==manifest['files']
 packages.append({'archive':str(archive.relative_to(root)),'archive_sha256':digest,'extraction':str(package.relative_to(root)),'manifest_sha256':sha(package/'PACKAGE-MANIFEST.json'),'verified_file_count':len(actual),'build':manifest['build']})
fresh=json.loads((out/'fresh-final-restarted-store.json').read_text());assert fresh['preferences']['shell']=='/usr/bin/bash'
assert (out/'fresh-final-saved-store.json').read_bytes()==(out/'fresh-final-restarted-store.json').read_bytes()
assert (out/'pty-marker.txt').read_text()=='CLEAN_NATIVE_PTY_OK\n'
old=json.loads((out/'upgrade-old-saved-store.json').read_text());assert old['sessions'][0]['id']
for name in ['upgrade-current-before-save-store.json','upgrade-current-saved-store.json','upgrade-current-restarted-store.json','clean-installed-store.json']:
 assert (out/name).read_bytes()==(out/'upgrade-old-saved-store.json').read_bytes(),name
assert old['preferences']['appearance']=='Dark' and old['preferences']['font_size']==18
assert old['sessions'][0]['folder']=='Preserved' and old['sessions'][0]['name']=='Legacy clean fixture'
assert old['macros']==[{'name':'Legacy clean macro','text':'printf LEGACY_MACRO_PRESERVED','append_enter':False}]
assert (out/'corrupt-input.bin').read_bytes()==(out/'corrupt-after-save.bin').read_bytes()==(out/'corrupt/workspace.json').read_bytes()
assert (out/'clean-install-status.log').read_text().strip()=='installed'
assert (out/'clean-uninstall-status.log').read_text().strip()=='not installed'
for name in ['fresh-install-save-restart-pty','legacy-save','upgrade-current','corrupt-save-restart-final','absent-installed-mosh','absent-bundled-mosh-dependency','direct-iperf-byte-closure','direct-unix-byte-closure-exec-tmp','per-user-install-startup-uninstall','clean-declared-browser-runtime']:
 record=json.loads((out/f'{name}.json').read_text());assert record['exit_code']==0 and record['owned_container_removed'],name
s=importlib.util.spec_from_file_location('debian',root/'scripts/package-debian.py');mod=importlib.util.module_from_spec(s);s.loader.exec_module(mod)
manifest=json.loads((root/packages[0]['extraction']/'PACKAGE-MANIFEST.json').read_text())
try:debian={'depends':mod.dependencies(manifest),'status':'generated'}
except ValueError as error:debian={'status':'rejected safely before producing package','error':str(error)}
result={'qualification':'Bounded clean Arch Linux container observations; not a full V08 or cross-platform parity claim','packages':packages,'fresh_native_gui_save_restart_pty':'passed','legacy_real_save_current_upgrade_restart_exact_bytes':'passed','legacy_saved_workspace_sha256':sha(out/'upgrade-old-saved-store.json'),'corrupt_actual_save_restart_original_bytes':'passed','corrupt_original_sha256':sha(out/'corrupt-input.bin'),'per_user_install_installed_gui_uninstall_preserves_workspace_defaults_unowned':'passed','absent_installed_mosh':'explicit provider unavailable; no fallback','absent_bundled_mosh_dependency':'explicit system Perl missing; no fallback','direct_iperf_2_3_loopback':json.loads((out/'clean-iperf-results.json').read_text()),'direct_unix_ash_pty':json.loads((out/'clean-unix-results.json').read_text()),'debian_wrapper_current_full_arch_archive':debian,'known_setup_failures':['Initial xdotool title search matched no window; corrected to owned process ID.','Initial PTY typing without terminal-widget focus produced no marker; explicit terminal click fixed setup.','Initial corruption check used unavailable cmp; replaced with Python byte comparison.','Initial Unix fixture expected busybox archive member; actual member bin/ash corrected.','Initial direct Unix execution hit Docker tmpfs default noexec; explicit exec on private tmpfs fixed setup.'],'remaining_gaps':['Native Windows/macOS and physical clean Linux machines','Full original runtime behavior/signing/notarization','Native distro installation/upgrade matrix and oldest Debian ABI build','Credential vault contents across upgrade: fixture covers session/macro/settings only','Managed app API success paths for all Mosh/Unix/Iperf/browser capabilities; direct payload checks remain distinct']}

result['images']={'base':{'registry_digest':'sha256:375a401bb3c6376b37f40b6dfa25067db37ec78d0b7af67ae6315f43a0184a9d','gui_image':'sha256:c326d40de0700f7086d34670a6d21b0af6d8a6da71bc442fceed67bcd0648fe5'},'input_image':'sha256:bf83b781e2d467fe7cbafb6c4235c3263260fd56cec7cfb9b22dafde0a914a12','declared_runtime_image':'sha256:1ab61a2bb16febd0a86829a43c26849b06ca82aca31be8385880cc7a3570c79d'}
result['browser']={'minimal_gui_image':'negative absent-library observation: libopenh264.so.8 unavailable','declared_runtime_image_version':(out/'clean-runtime-browser-version.log').read_text().strip(),'sandboxed_native_launch':'unresolved: container lacks usable root-owned setuid helper/unprivileged namespace sandbox; exact SIGABRT log retained','no_sandbox_bypass':True,'system_clients_absent':json.loads((out/'clean-runtime-system-clients-absent.json').read_text())}
result['highest_declared_glibc_symbol_version']=max((x['highest_required_glibc_symbol_version'] for x in manifest['runtime']['linked_libraries'].values() if x.get('highest_required_glibc_symbol_version')),key=lambda x:tuple(map(int,x.split('.'))))
result['debian_dependency_script_sha256']=sha(root/'scripts/package-debian.py')
result['evidence_hashes']={str(p.relative_to(root)):sha(p) for p in sorted(out.iterdir()) if p.is_file() and p.name!='clean-observations.json'}
(root/'docs/parity-evidence/V08/clean-observations.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k not in ['packages','evidence_hashes']},indent=2))
