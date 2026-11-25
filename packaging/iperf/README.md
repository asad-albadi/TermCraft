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

Build provenance for this qualified payload:

- Iperf2 source: https://downloads.sourceforge.net/project/iperf2/iperf-2.2.1.tar.gz
  Configure with `./configure`; build with `make -j2`.
- Iperf3 source: https://downloads.es.net/pub/iperf/iperf-3.19.1.tar.gz
  Configure with `./configure --disable-shared --enable-static --without-openssl`;
  build with `make -j2`. libiperf is linked into the executable; host libc and libm
  remain shared libraries. OpenSSL authentication is not enabled in this build.
- Compiler: GCC 16.2.1 20260810; glibc 2.44 host ABI. Iperf2 additionally needs host
  libstdc++ and libgcc_s. This is a Linux x86_64 host-qualified payload, not a universal
  Linux binary or native Windows/macOS evidence. Native engines for those platforms
  are not claimed. Installed providers remain an explicitly selected compatibility
  option and are never presented as bundled engines.

A rebuild may differ across toolchains; it must be independently qualified and its
binary checksum deliberately updated. Source checksums alone do not authorize new
binary bytes. The included source archives and licenses must accompany distribution.
