FROM termcraft-validation-v08:clean-input
USER root
RUN pacman -S --noconfirm --needed alsa-lib at-spi2-core cairo dbus expat glib2 glibc gtk3 hicolor-icon-theme libcups libffi libgcc libgcrypt libpulse libstdc++ libva libx11 libxcb libxcomposite libxdamage libxext libxfixes libxkbcommon libxrandr libxss mesa nspr nss pango pciutils systemd systemd-libs ttf-liberation xdg-utils zlib fontconfig brotli libjpeg-turbo openh264 simdutf highway dav1d flac libdrm libxml2 zstd libwebp minizip opus double-conversion harfbuzz freetype2 perl openssh && pacman -Q > /opt/termcraft-validation-runtime-packages.txt
USER 1000:1000
