# Linux Iperf glibc compatibility rebuild — 2026-11-27

The inherited engines could not load on Debian 12, and caused the Linux release
packaging test to fail during linked-library discovery. This is a separate ABI
repair after the Windows/macOS branch integration, not new native macOS/Windows
Iperf qualification or an application release approval.

## Reproduction and fix

`red-ldd.log` records missing GLIBC_2.43/GLIBC_2.38 on the original Iperf2 and
GLIBC_2.38 on the original Iperf3. `red-packaging.log` records the unchanged
release test failing in `native_libraries`. `red-abi-test.log` shows the added
baseline assertion failing on both original executables before replacement.

Both candidates were built offline from the tracked, hash-checked source archives
in the existing native Debian 12 builder image
`termcraft-validation-p08:debian-builder`, image ID
`sha256:1e7cd41f77c63d2093907d8a33cc65562eafd5cfbed60c829b9faedf652f24a0`.
`toolchain.txt` records the compiler, libc and resolved libraries. Iperf2 uses
`./configure`; Iperf3 uses
`./configure --disable-shared --enable-static --without-openssl`; both use
`make -j2`. `packaging/iperf/rebuild-linux.sh` repeats this offline procedure
without editing approval pins. The source archives and license files are unchanged.

## Current executable pins

| Tool | SHA-256 | Highest GLIBC import | Other C++ ABI imports |
| --- | --- | --- | --- |
| iperf2 2.2.1 | `487898553ead5875deea12ca44720c7a3893768657fb15cc4c6a13f7e4f32722` | 2.34 | GLIBCXX_3.4, CXXABI_1.3.9 |
| iperf3 3.19.1 | `6b5000d9918471cf785f1dd2b09fb8294cdf4053db48e9fabb749f0ac46ec9c1` | 2.34 | none |

The binaries remain dynamically dependent on host libc/libm, with libstdc++ and
libgcc_s additionally required by Iperf2. No RPATH/RUNPATH is present. The tested
baseline is Debian 12/glibc 2.36, not every distribution with GLIBC_2.34.

## Fresh qualification

Run from the repository root in the Debian 12 builder with the repository mounted
read-only and Docker networking disabled:

```sh
python3 tests/packaging_iperf.py
```

`green-packaging.log`: 5 tests passed. This checks source/license/binary staging and
tamper rejection; GLIBC symbol ceiling and absent runtime search paths; real
staged Iperf2 and Iperf3 servers and clients over both TCP and UDP on loopback,
with positive transferred byte counts; and actual release archive inclusion plus
linked-library declaration. The endpoint fixtures use their own processes and
ports and terminate their servers. These are local software endpoint checks;
external network performance, hardware and full application service qualification
are not claimed. Historical B03/V08/P08 artifacts and reports remain historical
and retain their original hashes. No source or license pin was changed.

`green-host-packaging.log`: the same 5 tests also passed on the current Arch
Linux host. `rebuild-script-check.log` records a successful independent invocation
of the offline rebuild helper inside Debian 12. Its separately generated
candidates were not repinned: the approved hashes above remain the original
Debian-built candidates qualified by the test run. Ubuntu 24.04 was not directly
executed in this check; Debian 12 supplies an older glibc baseline.
