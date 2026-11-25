# S01–S07 pre-flight research (no session implementation yet)

Date: 2026-11-24. Workspace review fix-pass verification is the current gate. All session work below remains planned.

Primary sources inspected:

- [Mobatek documentation](https://mobaxterm.mobatek.net/documentation.html): bookmarks live in INI; folder export/import and shared configuration are documented. This does not define the packed record schema.
- [Mobatek CSV import documentation, 2026-03-03](https://blog.mobatek.net/post/mobaxterm-import-csv/): columns include folder/name/type, endpoint/user/port, key and command, plus proxy/gateway fields. A supported CSV fixture can be authored against this specification; it is not an original 26.6 export capture.
- [Ruzgfpegk’s firsthand format study, version 26.3](https://gist.github.com/Ruzgfpegk/ab597838e4abbe8de30d7224afd062ea): INI bookmark sections, nested SubRep folders and CP1252 export encoding; packed SSH group indexes identify host/port/user, forwarding/compression and private-key path. Icon IDs are presentation, not protocol identity. Treat synthetic fixtures derived from this study as format evidence, not captured 26.6 output. Unsupported fields/versions require explicit preview warnings.
- [Official PuTTY 0.85 source archive](https://the.earth.li/~sgtatham/putty/0.85/putty-0.85.tar.gz), SHA256 `13fd4db2936d03b73812a7bcc2a658e4dd29cc776a56c3670a7fc6f1a0ee8af8`. Inspected `conf.h`, `settings.c`, `windows/storage.c`, `windows/utils/registry.c` in owned `/tmp/termcraft-putty-0.85-source/`. Writer keys include HostName, PortNumber, UserName and PublicKeyFile; Windows storage uses the per-user Sessions registry namespace. An authored .reg fixture is not an actual Windows export capture.

Shared-interface decisions to ledger when S01 starts:

- S01 produces persisted stable session IDs for S02–S04. Old sessions acquire IDs once and persist on next successful save; edit/rename preserve IDs, duplication/import-as-new generates fresh IDs. Saved-session IDs remain distinct from runtime terminal/connection generations.
- S02 external snapshots remain source-scoped and read-only; refreshing a same-name remote session never overwrites local IDs. Copy-to-local makes a new local ID. Sources without upstream stable IDs cannot promise rename identity beyond replacing a source snapshot.
- S03 folder launch and S04 IPC/shortcuts resolve saved IDs, not display names. Quote platform arguments; never execute import/shortcut text as a shell command.
- S05 credentials/vault and S06 key/agent references remain separate from portable session definitions. S01 drops credential values and reports excluded field names/locations without storing unknown raw values that could hide secrets.
- S07 advanced options are mapped only when actual backend propagation exists; unsupported import settings remain preview warnings. Key paths are preserved literally; PPK/drive-placeholder conversion must not be invented.
- File reads/parsing run on a bounded cancellable background worker. Parse/preview does not mutate Store; Apply validates all candidates before changing it. Duplicate policies are explicit, with safe Skip as default.

Native MobaXterm 26.6 exports, Windows Registry export and native command/agent/vault behavior remain external evidence requirements. No scope exclusion or passing runtime claim follows from missing native access.
