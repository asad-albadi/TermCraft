use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use std::{path::Path, sync::Arc};

pub fn localhost_server(root: &Path) -> Arc<rustls::ServerConfig> {
    prepare_localhost_identity(root);
    let certificate = CertificateDer::from_pem_file(root.join("server.pem")).unwrap();
    let key = PrivateKeyDer::from_pem_file(root.join("server.key")).unwrap();
    Arc::new(
        rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![certificate], key)
        .unwrap(),
    )
}

pub fn prepare_localhost_identity(root: &Path) {
    let openssl = |args: &[&str]| {
        let output = std::process::Command::new("openssl")
            .current_dir(root)
            .args(args)
            .output()
            .expect("TLS fixture requires openssl");
        assert!(
            output.status.success(),
            "TLS fixture openssl {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    };
    openssl(&[
        "req",
        "-x509",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-days",
        "1",
        "-subj",
        "/CN=TermCraft loopback Test CA",
        "-addext",
        "basicConstraints=critical,CA:TRUE,pathlen:0",
        "-addext",
        "keyUsage=critical,keyCertSign,cRLSign",
        "-keyout",
        "ca.key",
        "-out",
        "ca.pem",
    ]);
    openssl(&[
        "req",
        "-new",
        "-newkey",
        "rsa:2048",
        "-nodes",
        "-subj",
        "/CN=localhost",
        "-keyout",
        "server.key",
        "-out",
        "server.csr",
    ]);
    std::fs::write(
        root.join("server.ext"),
        "subjectAltName=DNS:localhost\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\n",
    )
    .unwrap();
    openssl(&[
        "x509",
        "-req",
        "-days",
        "1",
        "-set_serial",
        "2",
        "-CA",
        "ca.pem",
        "-CAkey",
        "ca.key",
        "-in",
        "server.csr",
        "-extfile",
        "server.ext",
        "-out",
        "server.pem",
    ]);
}
