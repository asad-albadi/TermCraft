use super::Limits;
use rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
use std::{io::Read, path::Path, sync::Arc};
use tokio::net::TcpStream;
use tokio_rustls::{TlsConnector, client::TlsStream};

pub(super) struct Tls {
    connector: TlsConnector,
    name: ServerName<'static>,
}

impl Tls {
    pub(super) fn new(name: ServerName<'static>, ca: Option<&Path>) -> Result<Self, String> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let roots = ca.map(read_roots).transpose()?.unwrap_or_default();
        let verifier =
            rustls_platform_verifier::Verifier::new_with_extra_roots(roots, provider.clone())
                .map_err(|_| "Cannot initialize FTP certificate verification.")?;
        let config = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|_| "Cannot initialize FTP TLS versions.")?
            .dangerous()
            .with_custom_certificate_verifier(Arc::new(verifier))
            .with_no_client_auth();
        // The control and data TLS connections use the same verified DNS/IP
        // name and config. Servers may require a resumed control TLS session.
        Ok(Self {
            connector: TlsConnector::from(Arc::new(config)),
            name,
        })
    }

    pub(super) async fn wrap(
        &self,
        stream: TcpStream,
        limits: &Limits,
        channel: &'static str,
    ) -> Result<TlsStream<TcpStream>, String> {
        limits
            .run("TLS handshake", async {
                let tls = self
                    .connector
                    .connect(self.name.clone(), stream)
                    .await
                    .map_err(|_| {
                        format!("FTP {channel} TLS certificate validation or handshake failed.")
                    })?;
                Ok(tls)
            })
            .await
    }
}

pub(super) fn fingerprint(stream: &TlsStream<TcpStream>) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let certificate = stream
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|chain| chain.first())
        .ok_or("The FTP TLS peer provided no certificate identity.")?;
    Ok(Sha256::digest(certificate.as_ref())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn read_roots(path: &Path) -> Result<Vec<CertificateDer<'static>>, String> {
    const MAX_PEM_BYTES: u64 = 4 * 1024 * 1024;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options
        .open(path)
        .map_err(|_| "Cannot open the FTP certificate authority file.")?;
    if !file
        .metadata()
        .map_err(|_| "Cannot inspect the FTP CA file.")?
        .is_file()
    {
        return Err("The FTP certificate authority must be a regular PEM file.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_PEM_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Cannot read the FTP CA file.")?;
    if bytes.len() as u64 > MAX_PEM_BYTES {
        return Err("The FTP CA file exceeds 4 MiB.".into());
    }
    let roots = CertificateDer::pem_slice_iter(&bytes)
        .take(129)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| "The FTP CA file contains invalid PEM certificates.")?;
    if roots.is_empty() || roots.len() > 128 {
        return Err("The FTP CA file must contain 1–128 PEM certificates.".into());
    }
    Ok(roots)
}
