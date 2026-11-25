# S07 advanced SSH/proxy implementation evidence

2026-11-24, Linux. This is an implementation checkpoint, not full original/native parity.

- `red.log`: missing planned APIs/schema observed before implementation.
- `transport-green.log`: seven targeted cases, real owned OpenSSH endpoints and loopback proxy relays. Manual clock causes real encrypted keepalive traffic at 60 seconds, not at 59. Two different configured gateway hosts authenticate before the target. HTTP Basic/SOCKS5 responses come through private typed challenges.
- `ui-vault-green.log`: actual background vault controller links proxy-only opaque references, encrypts both prompt fields and clears deletion references. Test passed without a preceding behavioral RED; no regression claim for that additional case.
- `full.log`: first default suite, 356 passed / 6 ignored. `review.md` and its RED/GREEN logs: independent review and the single fix pass. `post-review-full.log`: final suite, 359 passed / 6 ignored. `fmt.log`: clean formatting. A subsequent source-only config regression is recorded in `source-only-{red,green}.log`; `final-full.log` passes 360 top-level tests / 6 ignored plus one isolated helper case.

Configuration parsing supports the declared subset only. Unknown semantics stop embedded networking and require explicit OpenSSH compatibility; directive values are not copied into diagnostics. The selected native file is passed with `-F`. No TLS-to-proxy, HTTP Digest/NTLM or proprietary SOCKS authentication is claimed. Keepalive uses target/gateway SSH ping replies; cancellation drops the connection runtime. Native Windows/macOS and original Personal 26.6 interoperability remain pending. See the execution ledger for precedence, timeout and credential-binding rulings.
