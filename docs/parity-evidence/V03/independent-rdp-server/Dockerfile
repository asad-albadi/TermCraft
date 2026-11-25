FROM debian:bookworm
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends freerdp2-shadow-x11 xvfb xauth xfonts-base xrdp xorgxrdp xserver-xorg-core xterm xclip x11-utils openssl dbus-x11 fuse3 python3 && rm -rf /var/lib/apt/lists/*
