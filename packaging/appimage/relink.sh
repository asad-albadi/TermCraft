#!/bin/sh
# Run from the relink directory, replacing the desired library archive first.
set -eu
output=$1
clang -nostdlib -static-pie -Wl,--gc-sections,--build-id=none,-Map,runtime.map -T data_sections.ld \
    rcrt1.o crti.o runtime.o -Wl,--start-group libsquashfuse.a libsquashfuse_ll.a \
    libzstd.a libz.a libfuse3.a libc.a -Wl,--end-group crtn.o -o "$output"
strip --strip-unneeded "$output"
printf 'AI\002' | dd of="$output" bs=1 seek=8 count=3 conv=notrunc
