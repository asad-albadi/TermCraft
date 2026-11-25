use crate::wire;
use rustls::{
    DigitallySignedStruct, SignatureScheme,
    client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier},
    pki_types::{CertificateDer, ServerName, UnixTime},
};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};
pub fn fingerprint(cert: &[u8]) -> String {
    Sha256::digest(cert)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
pub struct Verifier {
    inner: Arc<dyn ServerCertVerifier>,
    pin: Option<String>,
    pub seen: Arc<Mutex<Option<String>>>,
}
impl std::fmt::Debug for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RdpCertificateVerifier")
    }
}
impl Verifier {
    pub fn make(
        config: &wire::Config,
        pin: Option<String>,
    ) -> Result<(Arc<Self>, Arc<rustls::ClientConfig>), String> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        let roots = if let Some(path) = &config.certificate_authority {
            use rustls::pki_types::pem::PemObject;
            use std::io::Read;
            let mut options = std::fs::OpenOptions::new();
            options.read(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.custom_flags(libc::O_NONBLOCK);
            }
            let file = options
                .open(path)
                .map_err(|_| "Desktop CA file is unavailable")?;
            if !file
                .metadata()
                .map_err(|_| "Cannot inspect desktop CA file")?
                .is_file()
            {
                return Err("Desktop CA must be a regular PEM file".into());
            }
            let mut bytes = Vec::new();
            file.take(4 * 1024 * 1024 + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| "Cannot read desktop CA file")?;
            if bytes.len() > 4 * 1024 * 1024 {
                return Err("Desktop CA file exceeds 4 MiB".into());
            }
            let certs = CertificateDer::pem_slice_iter(&bytes)
                .collect::<Result<Vec<_>, _>>()
                .map_err(|_| "Desktop CA file must contain PEM certificates")?;
            if certs.is_empty() || certs.len() > 128 {
                return Err("Desktop CA file must contain 1–128 certificates".into());
            }
            certs
        } else {
            Vec::new()
        };
        let inner =
            rustls_platform_verifier::Verifier::new_with_extra_roots(roots, provider.clone())
                .map_err(|_| "Cannot initialize system TLS verification")?;
        let verifier = Arc::new(Self {
            inner: Arc::new(inner),
            pin,
            seen: Arc::new(Mutex::new(None)),
        });
        let mut tls = rustls::ClientConfig::builder_with_provider(provider)
            .with_safe_default_protocol_versions()
            .map_err(|_| "Cannot initialize TLS protocols")?
            .dangerous()
            .with_custom_certificate_verifier(verifier.clone())
            .with_no_client_auth();
        tls.resumption = rustls::client::Resumption::disabled();
        Ok((verifier, Arc::new(tls)))
    }
}
impl ServerCertVerifier for Verifier {
    fn verify_server_cert(
        &self,
        cert: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        name: &ServerName<'_>,
        ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let actual = fingerprint(cert);
        *self.seen.lock().unwrap_or_else(|p| p.into_inner()) = Some(actual.clone());
        if let Some(pin) = &self.pin {
            return if pin == &actual {
                Ok(ServerCertVerified::assertion())
            } else {
                Err(rustls::Error::InvalidCertificate(
                    rustls::CertificateError::ApplicationVerificationFailure,
                ))
            };
        }
        self.inner
            .verify_server_cert(cert, intermediates, name, ocsp, now)
    }
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        sig: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls12_signature(message, cert, sig)
    }
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        sig: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        self.inner.verify_tls13_signature(message, cert, sig)
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.inner.supported_verify_schemes()
    }
}
