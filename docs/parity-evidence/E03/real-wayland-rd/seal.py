from pathlib import Path
import hashlib,json,subprocess
B=Path(__file__).resolve().parent;APP=B.parents[2]
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
sourcefiles=['src/services/vnc/wayland.rs','src/services/vnc/wayland/portal.rs','src/services/vnc/wayland/helper.rs','src/services/vnc/keyboard.rs','src/runtime/manifest.rs','packaging/runtime-manifest.json']
(B/'source-hashes.json').write_text(json.dumps({p:sha(B/'source'/p) for p in sourcefiles},indent=2))
meta={'git_head':subprocess.check_output(['git','rev-parse','HEAD'],cwd=APP,text=True).strip(),'private_component':'real-wayland-rd-provider-private 0.1.0-real-rd.1 normal src/main.rs','rustc':subprocess.check_output(['rustc','--version'],text=True).strip(),'build_command':'CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR=<app>/target cargo build --offline --manifest-path target/agent-work/real-wayland-rd/source/Cargo.toml --bin real-wayland-rd-provider-private','native_packages':subprocess.check_output(['pacman','-Q','xdg-desktop-portal','pipewire','wireplumber','mesa','gtk4','libei','python-gobject'],text=True),'signed_official_arch_packages':{p.name:{'sha256':sha(p),'signature_sha256':sha(Path(str(p)+'.sig')),'url':'https://fastly.mirror.pkgbuild.com/extra/os/x86_64/'+p.name} for p in (B/'inputs').glob('*.pkg.tar.zst')},'verification_keyring_sha256':sha(APP/'target/wayland-real-portal-inputs/archlinux-keyring.gpg'),'primary_references':['https://github.com/GNOME/mutter/blob/main/doc/building-and-running.md','https://github.com/GNOME/xdg-desktop-portal-gnome/blob/main/src/remotedesktop.c','https://github.com/GNOME/xdg-desktop-portal-gnome/blob/main/src/remotedesktopdialog.ui','https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.RemoteDesktop.html'],'limitations':'Configured native private headless GNOME/Mutter platform, source harness, not application release GUI or physical GPU desktop; no Windows/macOS claim.'}
(B/'inputs.json').write_text(json.dumps(meta,indent=2))
# Hash all final inputs/results plus retained setup/unprimed evidence and extracted native tools.
files=[p for p in B.rglob('*') if p.is_file() and p.name != 'EVIDENCE-SHA256.json']
(B/'EVIDENCE-SHA256.json').write_text(json.dumps({str(p.relative_to(B)):sha(p) for p in sorted(files)},indent=2))
print('Sealed',len(files),'evidence files')
