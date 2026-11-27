# Untrusted HTTPS test identity

The browser's negative trust fixture uses the checked-in `localhost` identity.
The private key is public test data. Tests that require successful trust
generate a fresh, one-day CA and leaf through `tests/support/tls.rs` instead.
