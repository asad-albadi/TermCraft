//! VeNCrypt 0.2, verified X509None/X509Vnc only. No anonymous TLS or downgrade.
use super::*;
use rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
fn config(path: Option<&std::path::Path>) -> Result<Arc<rustls::ClientConfig>, String> {
    let provider = Arc::new(rustls::crypto::ring::default_provider());
    let mut roots = Vec::new();
    if let Some(path) = path {
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
            .map_err(|_| "VNC CA file is unavailable")?;
        if !file
            .metadata()
            .map_err(|_| "Cannot inspect VNC CA file")?
            .is_file()
        {
            return Err("VNC CA must be a regular PEM file".into());
        }
        let mut data = Vec::new();
        file.take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut data)
            .map_err(|_| "Cannot read VNC CA file")?;
        if data.len() > 4 * 1024 * 1024 {
            return Err("VNC CA file exceeds 4 MiB".into());
        }
        roots = CertificateDer::pem_slice_iter(&data)
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| "VNC CA file must contain PEM certificates")?;
        if roots.is_empty() || roots.len() > 128 {
            return Err("VNC CA file must contain 1–128 certificates".into());
        }
    }
    let verifier =
        rustls_platform_verifier::Verifier::new_with_extra_roots(roots, provider.clone())
            .map_err(|_| "Cannot initialize VNC certificate verification")?;
    let mut config = rustls::ClientConfig::builder_with_provider(provider)
        .with_safe_default_protocol_versions()
        .map_err(|_| "Cannot initialize VNC TLS protocols")?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
    config.resumption = rustls::client::Resumption::disabled();
    Ok(Arc::new(config))
}
pub(super) async fn negotiate(
    mut stream: BoxStream,
    session: &Session,
) -> Result<(BoxStream, String), String> {
    if bytes(&mut stream, 2).await? != [0, 2] {
        return Err("Only VeNCrypt version 0.2 is supported".into());
    }
    send(&mut stream, &[0, 2]).await?;
    if bytes(&mut stream, 1).await? != [0] {
        return Err("VNC server rejected VeNCrypt version 0.2".into());
    }
    let count = bytes(&mut stream, 1).await?[0] as usize;
    if count == 0 || count > 64 {
        return Err("Invalid VeNCrypt subtype count".into());
    }
    let selected = if session.vnc_options.security.password() {
        261u32
    } else {
        260
    };
    let mut found = false;
    for _ in 0..count {
        found |= number(&mut stream).await? == selected;
    }
    if !found {
        return Err("VNC server does not offer the selected verified X509 security subtype; no fallback was attempted".into());
    }
    send(&mut stream, &selected.to_be_bytes()).await?;
    if bytes(&mut stream, 1).await? != [1] {
        return Err("VNC server rejected the selected verified TLS subtype".into());
    }
    let host = session
        .host
        .trim_start_matches('[')
        .trim_end_matches(']')
        .to_owned();
    let name = ServerName::try_from(host).map_err(|_| "Invalid VNC TLS server name")?;
    let connector = tokio_rustls::TlsConnector::from(config(
        session.vnc_options.certificate_authority.as_deref(),
    )?);
    let encrypted = connector.connect(name, stream).await.map_err(
        |_| "VNC TLS certificate/hostname validation or handshake failed; no credentials were sent",
    )?;
    use sha2::{Digest, Sha256};
    let certificate = encrypted
        .get_ref()
        .1
        .peer_certificates()
        .and_then(|certs| certs.first())
        .ok_or("VNC TLS server presented no certificate")?;
    let fingerprint = Sha256::digest(certificate.as_ref())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    Ok((Box::new(encrypted), fingerprint))
}
