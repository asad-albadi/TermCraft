FROM termcraft-validation-v08:clean-input
USER root
RUN pacman -S --noconfirm --needed tesseract tesseract-data-eng && pacman -Q > /opt/termcraft-validation-fix1-ocr-packages.txt
USER 1000:1000
