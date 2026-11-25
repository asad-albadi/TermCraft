FROM termcraft-validation-v08:arch-gui
USER root
RUN pacman -S --noconfirm --needed xorg-server-xvfb xdotool imagemagick && pacman -Q > /opt/termcraft-validation-input-packages.txt
USER 1000:1000
