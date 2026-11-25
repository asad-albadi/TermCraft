# D01 engine evaluation sources (not implementation evidence)

IronRDP 0.17.0, MIT OR Apache-2.0; examples copied from the published crate. Source commit `11a0810cfbbabd8b8023875a05e3041216d4b01b`. These are upstream reference examples, not TermCraft code or passing parity tests. Original licenses accompany the files.

Primary source: https://github.com/Devolutions/IronRDP/tree/11a0810cfbbabd8b8023875a05e3041216d4b01b/crates/ironrdp . The Tokio adapter reference is from published ironrdp-tokio 0.10.0. No product dependency is pinned yet. Engine evaluation is preliminary while S07 review fixes are gated.

Evaluation command: `CARGO_BUILD_JOBS=2 cargo run --offline --manifest-path docs/parity-evidence/rdp-prototype/Cargo.toml`. `engine-green.log` records actual TLS/NLA connection, decoded red RGBA pixel, server-observed pointer/key events and natural server-side transport closure. This does not establish workspace integration or full RDP parity.

Ruling: explicitly advertise the bitmap codecs actually decoded by the client. Server-only Cargo feature unification can otherwise enable PDU advertisement of QOI/QOIZ without the corresponding client decoder, yielding ignored surface updates. The controlled pixel proof selects the lossless baseline codec list explicitly; it does not enable optional server codecs by accident. Product defaults initially use that tested lossless path; broader codec and native Windows interoperability remain validation work.
