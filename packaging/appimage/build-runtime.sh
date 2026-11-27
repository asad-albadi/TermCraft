#!/bin/sh
# Run in runtime.Dockerfile with /inputs and /recipe read-only, /out writable.
set -eu
export SOURCE_DATE_EPOCH=1743465600 LC_ALL=C TZ=UTC
export CFLAGS="-O2 -fPIC -ffile-prefix-map=/build=."
while read -r expected name url; do
    echo "$expected  /inputs/$name" | sha256sum -c -
    tar xf "/inputs/$name"
done </recipe/runtime-inputs.tsv
mkdir -p /out/relink /out/notices /out/build
prefix=/opt/termcraft
mkdir -p "$prefix"
cd /build/musl-1.2.5
./configure --prefix="$prefix" --disable-shared
make -j2
make -j2 install
cp COPYRIGHT /out/notices/musl.txt
cd /build/zlib-1.3.1
./configure --static --prefix="$prefix"
make -j2
make install
cp LICENSE /out/notices/zlib.txt
cd /build/zstd-1.5.7
make -C lib -j2 libzstd.a
install -m644 lib/libzstd.a "$prefix/lib"
cp lib/zstd*.h "$prefix/include/"
cp LICENSE /out/notices/zstd.txt
cd /build
runtime=$(echo /build/type2-runtime-*)
cd fuse-3.15.0
patch -p1 <"$runtime/patches/libfuse/mount.c.diff"
meson setup build --prefix="$prefix" --libdir=lib --default-library=static -Dexamples=false -Dtests=false
meson compile -C build -j2
meson install -C build --no-rebuild
cp LICENSE /out/notices/libfuse.txt
cp LGPL2.txt /out/notices/libfuse-LGPL2.txt
cd /build/squashfuse-0.5.2
./autogen.sh
export PKG_CONFIG_PATH="$prefix/lib/pkgconfig"
CPPFLAGS="-I$prefix/include" LDFLAGS="-L$prefix/lib" ./configure --prefix="$prefix" --enable-static --disable-shared
make -j2
make install
cp ./*.h "$prefix/include/squashfuse/"
cp LICENSE /out/notices/squashfuse.txt
cd "$runtime"
patch -p1 </recipe/runtime-cleanup.patch
cd src/runtime
clang -std=gnu99 -Os -fPIE -D_FILE_OFFSET_BITS=64 -DGIT_COMMIT='"dd6cebedcbddde9c82f89b011e8e1d40b6e43868"' \
  -ffunction-sections -fdata-sections -I"$prefix/include" -I"$prefix/include/squashfuse" -I"$prefix/include/fuse3" -c runtime.c -o /out/relink/runtime.o
cd /out/relink
for name in rcrt1.o crti.o crtn.o libsquashfuse.a libsquashfuse_ll.a libzstd.a libz.a libfuse3.a libc.a; do
    cp "$prefix/lib/$name" .
done
cp "$runtime/src/runtime/data_sections.ld" .
cp /recipe/relink.sh .
sh relink.sh /out/runtime
cp "$runtime/LICENSE" /out/notices/type2-runtime.txt
apk info -vv | sort >/out/build/apk-packages.txt
clang --version >/out/build/clang-version.txt
ld --version >/out/build/ld-version.txt
