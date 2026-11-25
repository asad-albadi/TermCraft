#[path = "options.rs"]
mod types;
pub use types::*;

/// Authentication identity of the saved definition, excluding display/clipboard choices.
pub fn auth_profile_binding(session: &crate::model::Session) -> String {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(&(
        session.protocol.as_str(),
        session.host.to_ascii_lowercase(),
        session.port,
        session.user.as_str(),
        session.rdp_options.domain.as_str(),
        session.desktop_engine,
        session.rdp_options.security,
        session.jump.as_str(),
    ))
    .expect("RDP identity is serializable");
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Call only for the transport's typed authentication challenge after TLS verification.
pub fn credential_binding(
    session: &crate::model::Session,
    host: &str,
    port: u16,
    certificate: &str,
    user: &str,
    domain: &str,
) -> Result<crate::credentials::CredentialBinding, String> {
    if session.protocol != "RDP" || !session.host.eq_ignore_ascii_case(host) || session.port != port
    {
        return Err("RDP challenge does not match the session endpoint".into());
    }
    let binding = crate::credentials::CredentialBinding::Rdp {
        host: host.to_ascii_lowercase(),
        port,
        user: user.into(),
        domain: domain.into(),
        certificate: certificate.to_ascii_lowercase(),
        profile: auth_profile_binding(session),
    };
    binding.validate()?;
    Ok(binding)
}

/// Password-free UTF-16LE profile consumed by mstsc, never unsupported /u flags.
pub fn windows_profile(session: &crate::model::Session) -> Result<Vec<u8>, String> {
    crate::model::validate_host(&session.host)?;
    if session.port == 0 || session.user.len() > 4096 || session.user.chars().any(char::is_control)
    {
        return Err("Invalid RDP endpoint or username".into());
    }
    session.rdp_options.validate()?;
    if session.rdp_options.smartcard_reader.is_some() {
        return Err("Selecting a local PC/SC reader requires the embedded Linux RDP engine".into());
    }
    if session.rdp_options.printer_mapping.is_some() {
        return Err("Explicit local printer mapping requires the embedded RDP engine".into());
    }
    if !session.rdp_options.serial_mappings.is_empty() {
        return Err("Explicit local serial mappings require the embedded RDP engine".into());
    }
    if session.view_only
        || session.rdp_options.certificate_authority.is_some()
        || session.rdp_options.keyboard_layout != 0
    {
        return Err("mstsc compatibility cannot enforce view-only, a custom certificate authority or a custom keyboard layout".into());
    }
    if session
        .rdp_options
        .drives
        .iter()
        .any(|drive| drive.read_only)
    {
        return Err("mstsc profiles cannot enforce read-only redirected drives".into());
    }
    let options = &session.rdp_options;
    let host = if session.host.contains(':') && !session.host.starts_with('[') {
        format!("[{}]", session.host)
    } else {
        session.host.clone()
    };
    let mut text = format!(
        "full address:s:{host}:{}\r\nusername:s:{}\r\ndomain:s:{}\r\nscreen mode id:i:{}\r\ndesktopwidth:i:{}\r\ndesktopheight:i:{}\r\nadministrative session:i:{}\r\nauthentication level:i:2\r\nenablecredsspsupport:i:{}\r\nredirectclipboard:i:{}\r\nredirectprinters:i:{}\r\nredirectcomports:i:{}\r\nredirectsmartcards:i:{}\r\nkeyboardhook:i:{}\r\n",
        session.port,
        session.user,
        options.domain,
        if session.fullscreen { 2 } else { 1 },
        session.width,
        session.height,
        u8::from(options.administrative),
        u8::from(options.security == RdpSecurity::Nla),
        u8::from(options.clipboard != ClipboardPolicy::Disabled),
        u8::from(options.printers),
        u8::from(options.serial_ports),
        u8::from(options.smartcard),
        if options.keyboard == KeyboardRouting::Remote {
            1
        } else {
            0
        }
    );
    if !matches!(
        options.clipboard,
        ClipboardPolicy::Disabled | ClipboardPolicy::Bidirectional
    ) {
        return Err("mstsc compatibility supports disabled or bidirectional clipboard only".into());
    }
    if !options.drives.is_empty() {
        return Err("Selected path-based drive mappings require an embedded drive provider; mstsc exposes drive letters instead".into());
    }
    text.push_str("drivestoredirect:s:\r\nredirectdrives:i:0\r\nredirectposdevices:i:0\r\n");
    let mut result = vec![0xff, 0xfe];
    for unit in text.encode_utf16() {
        result.extend_from_slice(&unit.to_le_bytes());
    }
    Ok(result)
}
