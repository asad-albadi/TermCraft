# Qualified managed Iperf engines

The B03 payload contains upstream Iperf 2.2.1 and Iperf 3.19.1, source-built and
interoperability tested on Linux x86_64. They are separate, incompatible protocols.
The committed runtime manifest pins executable and corresponding source SHA-256.
`stage-runtime.py MANIFEST PAYLOAD_ROOT DESTINATION` checks these hashes and copies
both engines, corresponding source archives and licenses to an application resource
folder. The application verifies hashes and native architecture before each launch;
Linux launches a sealed snapshot, preventing replacement after verification.

The engines are foreground child processes owned by the managed service. Output is
discarded, avoiding an unbounded server transcript. Starting/stopping never edits
system daemon configuration. Loopback listening is the default. External listening
requires an explicit selection. Iperf2 supports TCP or UDP servers; Iperf3 clients
select TCP or UDP. Address, port and report interval are passed as separate arguments.

Build provenance for the current payload (2026-11-27 compatibility rebuild):

- Iperf2 source: https://downloads.sourceforge.net/project/iperf2/iperf-2.2.1.tar.gz
  Configure with `./configure`; build with `make -j2`.
- Iperf3 source: https://downloads.es.net/pub/iperf/iperf-3.19.1.tar.gz
  Configure with `./configure --disable-shared --enable-static --without-openssl`;
  build with `make -j2`. libiperf is linked into the executable; host libc and libm
  remain shared libraries. OpenSSL authentication is not enabled in this build.
- Compiler: GCC 12.2.0 (`Debian 12.2.0-14+deb12u1`), Debian 12 x86_64,
  glibc `2.36-9+deb12u14`. Both binaries import GLIBC symbols through 2.34.
  Iperf2 additionally needs host libstdc++ (GLIBCXX_3.4, CXXABI_1.3.9) and
  libgcc_s. This is a Debian 12 baseline tested on Linux x86_64; it is not a
  universal Linux binary or native Windows/macOS evidence. Installed providers
  remain an explicitly selected compatibility option.
- `rebuild-linux.sh NEW_OUTPUT_DIRECTORY` rebuilds offline from the checked-in
  source archives after checking their existing source and license pins. Run it
  in a native Debian 12 builder with Python 3, GCC/G++, make, tar, binutils and
  the libc development headers. It writes candidates and logs separately, never
  approving new bytes or editing the manifest. Paths/toolchains can change debug
  information and binary hashes; each output still requires qualification.
- The ABI regression checks that no GLIBC import exceeds 2.36 and no RPATH or
  RUNPATH is embedded. Staging tests run the pinned engines as real TCP and UDP
  loopback clients and servers, and check release packaging/dependency discovery.
  See [compatibility evidence](../../docs/parity-evidence/iperf-glibc-compat/README.md).

The earlier B03/V08/P08 historical qualification used GCC 16.2.1/glibc 2.44
and executable hashes `570328443a3fbeb426dba301e2c892b3026645c2d7b3da8bc2fea9688978a834`
(Iperf2) and `b24644f3fe69e19f332acd64645f1a85336e5e07f80634b92ed84bdc4eb6f8c9`
(Iperf3). Those records describe those old bytes. They do not qualify this rebuild.
The old engines failed to load on Debian 12: Iperf2 required GLIBC_2.43 and
GLIBC_2.38; Iperf3 required GLIBC_2.38. The current pin deliberately replaces
both executables while retaining their exact upstream source and license pins.

A rebuild may differ across toolchains; it must be independently qualified and its
binary checksum deliberately updated. Source checksums alone do not authorize new
binary bytes. The included source archives and licenses must accompany distribution.
