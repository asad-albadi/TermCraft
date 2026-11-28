use ironrdp_connector::{
    ConnectorError, ConnectorErrorKind,
    sspi::{ErrorKind, credssp::NStatusCode as Nt},
};
use std::{error::Error as _, io};

#[derive(Clone, Copy)]
pub enum Stage {
    SignIn,
    DesktopSetup,
}

impl Stage {
    fn name(self) -> &'static str {
        match self {
            Self::SignIn => "RDP sign-in",
            Self::DesktopSetup => "RDP desktop setup",
        }
    }

    pub fn timeout(self) -> String {
        format!(
            "{} transport timed out waiting for the server.",
            self.name()
        )
    }

    // Render only locally defined labels and numeric error fields. Connector
    // reason strings, source messages, and SSPI descriptions can contain
    // peer-controlled data or authentication material.
    pub fn failure(self, error: &ConnectorError) -> String {
        let stage = self.name();
        match error.kind() {
            ConnectorErrorKind::Credssp(e) => {
                // SSPI peers may encode SEC_E_LOGON_DENIED in the NTWIN32
                // facility rather than returning STATUS_LOGON_FAILURE.
                const SSPI_LOGON_DENIED: Nt = Nt(0xC007_030C);
                let prefix = match e.nstatus {
                    Some(
                        Nt::LOGON_FAILURE
                        | Nt::NO_SUCH_USER
                        | Nt::WRONG_PASSWORD
                        | SSPI_LOGON_DENIED,
                    ) => "RDP sign-in rejected",
                    Some(
                        Nt::ACCOUNT_LOCKED_OUT
                        | Nt::ACCOUNT_DISABLED
                        | Nt::ACCOUNT_RESTRICTION
                        | Nt::INVALID_LOGON_HOURS
                        | Nt::INVALID_WORKSTATION
                        | Nt::PASSWORD_EXPIRED
                        | Nt::PASSWORD_MUST_CHANGE
                        | Nt::LOGON_TYPE_NOT_GRANTED
                        | Nt::LOGON_NOT_GRANTED
                        | Nt::SMARTCARD_LOGON_REQUIRED
                        | Nt::CTX_LOGON_DISABLED,
                    ) => "RDP sign-in policy denied access",
                    None if e.error_type == ErrorKind::LogonDenied => "RDP sign-in rejected",
                    None if matches!(
                        e.error_type,
                        ErrorKind::SmartCardLogonRequired
                            | ErrorKind::DelegationPolicy
                            | ErrorKind::DelegationRequired
                    ) =>
                    {
                        "RDP sign-in policy denied access"
                    }
                    _ => "RDP sign-in protocol failed",
                };
                let mut detail = format!(
                    "{prefix} (CredSSP, SSPI {:?} 0x{:08X}",
                    e.error_type,
                    u32::from(e.error_type)
                );
                if let Some(status) = e.nstatus {
                    detail.push_str(&format!(", NTSTATUS 0x{:08X}", status.0));
                    if let Some(label) = status.name() {
                        detail.push(' ');
                        detail.push_str(label);
                    }
                }
                detail.push(')');
                detail
            }
            ConnectorErrorKind::AccessDenied => match self {
                Self::SignIn => "RDP sign-in rejected by the server (access denied).".into(),
                Self::DesktopSetup => {
                    "RDP desktop setup protocol failed (server access denied).".into()
                }
            },
            ConnectorErrorKind::EntraAuth(code) => format!(
                "{stage} protocol failed during Entra authentication (HRESULT 0x{code:08X})."
            ),
            ConnectorErrorKind::Negotiation(reason) => format!(
                "{stage} protocol negotiation failed (RDP status 0x{:08X}).",
                u32::from(reason.code())
            ),
            _ => {
                if let Some(io) = error.source().and_then(|e| e.downcast_ref::<io::Error>()) {
                    if matches!(
                        io.kind(),
                        io::ErrorKind::UnexpectedEof
                            | io::ErrorKind::ConnectionReset
                            | io::ErrorKind::ConnectionAborted
                            | io::ErrorKind::TimedOut
                            | io::ErrorKind::BrokenPipe
                            | io::ErrorKind::NotConnected
                            | io::ErrorKind::WriteZero
                            | io::ErrorKind::NetworkDown
                            | io::ErrorKind::NetworkUnreachable
                    ) {
                        return format!("{stage} transport failed ({:?}).", io.kind());
                    }
                }
                let cause = match error.kind() {
                    ConnectorErrorKind::Decode(_) => "message decoding",
                    ConnectorErrorKind::Encode(_) => "message encoding",
                    ConnectorErrorKind::Custom => "message processing",
                    _ => "connector sequence",
                };
                // A line in the pinned implementation identifies the failing
                // check even for Reason(String), without exposing its contents
                // or any absolute build path.
                let location = error.location();
                let file = location
                    .file()
                    .rsplit(['/', '\\'])
                    .next()
                    .unwrap_or("connector");
                format!(
                    "{stage} protocol failed during {cause} ({file}, line {}).",
                    location.line()
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironrdp_connector::sspi::Error as SspiError;

    #[test]
    fn server_status_takes_precedence_over_generic_invalid_token() {
        for (status, prefix) in [
            (Nt::WRONG_PASSWORD, "RDP sign-in rejected"),
            (Nt::ACCOUNT_LOCKED_OUT, "RDP sign-in policy"),
            (Nt(0xF1234567), "RDP sign-in protocol"),
        ] {
            let sspi =
                SspiError::new_with_nstatus(ErrorKind::InvalidToken, "secret token value", status);
            let err = ConnectorError::new("sensitive context", ConnectorErrorKind::Credssp(sspi));
            let message = Stage::SignIn.failure(&err);
            assert!(message.starts_with(prefix));
            assert!(message.contains(&format!("NTSTATUS 0x{:08X}", status.0)));
            assert!(message.contains("SSPI InvalidToken 0x80090308"));
            assert!(!message.contains("secret") && !message.contains("sensitive"));
        }
    }

    #[test]
    fn transport_diagnostics_preserve_stage_without_rendering_inner_errors() {
        let err = ConnectorError::new("sensitive context", ConnectorErrorKind::Custom).with_source(
            io::Error::new(io::ErrorKind::UnexpectedEof, "secret server response"),
        );
        assert_eq!(
            Stage::SignIn.failure(&err),
            "RDP sign-in transport failed (UnexpectedEof)."
        );
        assert_eq!(
            Stage::DesktopSetup.failure(&err),
            "RDP desktop setup transport failed (UnexpectedEof)."
        );
        let opaque =
            ConnectorError::new("token", ConnectorErrorKind::Reason("secret payload".into()));
        let result = Stage::DesktopSetup.failure(&opaque);
        assert!(result.starts_with("RDP desktop setup protocol failed"));
        assert!(!result.contains("token") && !result.contains("secret") && !result.contains('/'));
    }
}
