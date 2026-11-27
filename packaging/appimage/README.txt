TermCraft unsigned Linux x86_64 AppImage

Keep the AppImage and the matching AppImage-source.tar.gz from the same
release. Verify SHA256SUMS. To use a downloaded copy, chmod +x the AppImage,
then launch it. Extract-and-run uses the AppImage runtime and requires
writable, executable temporary space:
  ./TermCraft-<version>-linux-x86_64.AppImage --appimage-extract-and-run

The release qualification exercises extract-and-run on Ubuntu 24.04 x86_64
using private X11 and native Wayland displays. A FUSE launch needs a working
host FUSE setup and is outside this qualification. APPIMAGE-MANIFEST.json
records the maximum GLIBC symbol requirement measured from the executables
and private client libraries. Host requirements include a compatible glibc
and loader, POSIX /bin/sh, NSS, CA trust, X11 locale and keyboard layout data,
a running display server, and a coherent GLVND, EGL/GLX and GPU driver stack.
The client libraries and their notices are inside the image. The complete
portable application and adjacent RDP helper are under usr/lib/termcraft.

The source sidecar contains matching Ubuntu source package inputs including
downstream packaging, runtime dependency sources, notices, compiler versions,
a linker map and replacement-library relink inputs. The separate TermCraft
source archive covers the application. To relink, use the recorded Alpine
builder tools, replace the desired static archive in runtime/relink and run
sh relink.sh /absolute/output/runtime there. To rebuild from source, build
recipe/runtime.Dockerfile for linux/amd64, mount sources/runtime at /inputs,
recipe at /recipe and an empty writable directory at /out; run
sh /recipe/build-runtime.sh in /build. The measured runtime SHA is pinned. The local runtime patch supplies a positive
file-descriptor limit for recursive cleanup under musl, propagates removal
errors, bounds TMPDIR and reserves space for the argument-list terminator.
Each launch exclusively creates a private extraction directory, including when
multiple launches share the same TMPDIR. The runtime removes its own tree
after its GUI exits. The builder restores the measured prefix after appimagetool writes its optional
MD5 field. SHA256SUMS identifies the resulting complete outer file.
