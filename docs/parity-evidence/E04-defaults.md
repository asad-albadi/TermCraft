# Original SCP-profile preflight (E04 / V07 dependency)

The supplied Personal 26.6 executable has SHA-256 `e9a66144b3a1c4d32f8062b1eb34e26e9ed5298106abf1982b4654ab107f13a5`. Recovered UI help at file offset `0xf96777` explicitly identifies four browser choices: None, SFTP protocol, SCP (enhanced speed), and SCP (normal speed). This verifies the presence of two SCP profiles; it does not measure their wire behavior, performance or defaults.

Existing recovered function evidence `ev_96c4a98f431c924551748ec1c8430804c7d7587b78694a9e685132063adabb63` at `0x9e0720` suppresses automatic SCP fallback for server banners considered switches/routers (OpenSSH FIPS and RomSShell). TermCraft keeps SFTP as default and requires explicit SCP choice, independently of that heuristic.

The companion JSON records offsets, artifact/hash and confidence. No default or speed equivalence was inferred from labels. The original runtime cannot be exercised here (no Wine/native Windows); E04's other defaults/limits and V07 runtime comparison remain pending. F05 may introduce an explicitly labeled independent compatibility backend after this preflight; it cannot claim measured enhanced/normal equivalence.
