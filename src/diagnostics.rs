//! Stable public error identifiers and safe, actionable presentation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Context {
    App,
    Startup,
    CommandLine,
    Settings,
    Session,
    Ssh,
    Rdp,
    Vnc,
    Files,
    Transfer,
    Server,
    X11,
    Keys,
    Vault,
    Macro,
    Tunnel,
    Import,
    Shared,
    Browser,
    Game,
    Plugin,
    Clipboard,
    Shortcut,
    Scheduler,
    Package,
    Tools,
    Highlight,
}
impl Context {
    pub const ALL: &'static [Self] = &[
        Self::App,
        Self::Startup,
        Self::CommandLine,
        Self::Settings,
        Self::Session,
        Self::Ssh,
        Self::Rdp,
        Self::Vnc,
        Self::Files,
        Self::Transfer,
        Self::Server,
        Self::X11,
        Self::Keys,
        Self::Vault,
        Self::Macro,
        Self::Tunnel,
        Self::Import,
        Self::Shared,
        Self::Browser,
        Self::Game,
        Self::Plugin,
        Self::Clipboard,
        Self::Shortcut,
        Self::Scheduler,
        Self::Package,
        Self::Tools,
        Self::Highlight,
    ];
    pub fn prefix(self) -> &'static str {
        match self {
            Self::App => "APP",
            Self::Startup => "START",
            Self::CommandLine => "CLI",
            Self::Settings => "SETTINGS",
            Self::Session => "SESSION",
            Self::Ssh => "SSH",
            Self::Rdp => "RDP",
            Self::Vnc => "VNC",
            Self::Files => "FILES",
            Self::Transfer => "TRANSFER",
            Self::Server => "SERVER",
            Self::X11 => "X11",
            Self::Keys => "KEYS",
            Self::Vault => "VAULT",
            Self::Macro => "MACRO",
            Self::Tunnel => "TUNNEL",
            Self::Import => "IMPORT",
            Self::Shared => "SHARED",
            Self::Browser => "BROWSER",
            Self::Game => "GAME",
            Self::Plugin => "PLUGIN",
            Self::Clipboard => "CLIPBOARD",
            Self::Shortcut => "SHORTCUT",
            Self::Scheduler => "SCHEDULER",
            Self::Package => "PACKAGE",
            Self::Tools => "TOOLS",
            Self::Highlight => "HIGHLIGHT",
        }
    }
}
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub code: String,
    pub summary: String,
    pub next_step: String,
    pub details: String,
}
impl Context {
    pub fn next_step(self) -> &'static str {
        match self {
            Self::Startup => {
                "Start TermCraft from your graphical desktop session. If it still fails, copy these details for support."
            }
            Self::CommandLine => "Check the command and arguments with --help, then try again.",
            Self::Settings => {
                "Check the workspace location and write permissions. Preserve a backup before changing a damaged settings file."
            }
            Self::Session => "Correct the connection settings, then try again.",
            Self::Ssh | Self::Rdp | Self::Vnc | Self::Tunnel => {
                "Check the host, port, network and selected connection settings, then try again."
            }
            Self::Files | Self::Transfer => {
                "Check the selected path, available space and permissions. For remote files, also check the connection."
            }
            Self::Server => {
                "Check the listen address, port and shared resource settings, then try again."
            }
            Self::X11 => "Check the X server settings and display availability, then try again.",
            Self::Keys => {
                "Check the selected key, its format, permissions and agent availability, then try again."
            }
            Self::Vault => {
                "Check the vault state and selected credential. Unlock the vault if required, then try again."
            }
            Self::Macro => "Review the macro and its target session, then try again.",
            Self::Import => {
                "Check the selected file format and contents. Keep a backup of the original file."
            }
            Self::Shared => {
                "Check the shared source address, access and definition, then refresh it."
            }
            Self::Browser => "Check the address, connection and browser provider, then try again.",
            Self::Game => {
                "Check the game settings or selected save file, then try again. Keep the current game open until your changes are saved."
            }
            Self::Plugin => {
                "Check the plugin configuration, permission choices and runtime provider, then try again."
            }
            Self::Clipboard => {
                "Check that the original target is still ready. Try keyboard paste if the clipboard service is unavailable."
            }
            Self::Shortcut => {
                "Review the shortcut or keybinding and its destination, then try again."
            }
            Self::Scheduler => {
                "Review the schedule, command and platform requirements, then try again."
            }
            Self::Package => {
                "Check the selected runtime package and provider settings, then try again."
            }
            Self::Tools => "Check the tool inputs and required runtime provider, then try again.",
            Self::Highlight => {
                "Correct the highlighting rule and try again. The last valid rules remain available."
            }
            Self::App => {
                "Review the operation and its settings, then try again. Copy these details if the problem continues."
            }
        }
    }
}

impl Diagnostic {
    pub fn new(context: Context, raw: &str) -> Self {
        let details = sanitize(raw);
        let lower = details.to_lowercase();
        let contains = |needles: &[&str]| needles.iter().any(|s| lower.contains(s));
        // Recognize reported causes only. Unknown or localized provider errors retain
        // their domain code and original explanation; numeric OS codes are not guessed.
        let (suffix, title, next) = if contains(&[
            "host key mismatch",
            "host key has changed",
            "host identification has changed",
            "host key changed",
        ]) {
            (
                "113",
                "The server identity has changed.",
                "Stop and verify the server’s host-key fingerprint through a trusted channel before updating the saved host key.",
            )
        } else if contains(&[
            "certificate verification failed",
            "certificate verify failed",
            "invalid peer certificate",
            "certificate expired",
            "certnotvalidforname",
            "unknownissuer",
        ]) {
            (
                "112",
                "The server certificate could not be verified.",
                "Check the server name, system clock and trusted certificate authority; verify the server identity before reconnecting.",
            )
        } else if contains(&[
            "xopendisplayfailed",
            "failed to connect to wayland",
            "wayland display is not set",
            "provided display handle is not supported",
            "noglutinconfigs",
        ]) {
            (
                "114",
                "The graphical display could not be opened.",
                "Start TermCraft from your graphical desktop session and check that its display environment is available.",
            )
        } else if contains(&[
            "password authentication failed",
            "authentication failed",
            "authentication rejected",
            "authentication failure",
            "invalid credentials",
            "incorrect password",
            "wrong password",
        ]) {
            (
                "104",
                "Sign-in was rejected.",
                "Check the username and selected password, key or credential. Confirm that this sign-in method is allowed by the server.",
            )
        } else if contains(&["connection refused", "actively refused"]) {
            (
                "101",
                "The connection was refused.",
                "Check the host and port, and confirm that the remote service is running and allows connections.",
            )
        } else if contains(&[
            "timed out",
            "timeout expired",
            "connection timeout",
            "semaphore timeout period",
            "connected party did not properly respond",
        ]) {
            (
                "102",
                "The operation timed out.",
                "Check the network and remote service, then try again. A timeout does not confirm whether a remote change was applied.",
            )
        } else if contains(&[
            "name or service not known",
            "nodename nor servname",
            "failed to resolve",
            "name resolution",
            "no such host",
        ]) {
            (
                "103",
                "The host name could not be resolved.",
                "Check the host name and your DNS or network connection, then try again.",
            )
        } else if contains(&[
            "permission denied",
            "access is denied",
            "access denied",
            "read-only file system",
            "operation not permitted",
        ]) {
            (
                "105",
                "Access was denied.",
                "Check the account, file or device permissions and whether the destination is read-only. Request access from its owner if needed.",
            )
        } else if contains(&[
            "no such file or directory",
            "cannot find the file",
            "cannot find the path",
            "file not found",
            "path not found",
        ]) {
            (
                "106",
                "The required file or folder was not found.",
                "Check the selected path and whether the file, device or runtime provider is still available.",
            )
        } else if contains(&["file exists", "already exists", "destination exists"]) {
            (
                "107",
                "The destination already exists.",
                "Choose another name or review the explicit replacement option before trying again.",
            )
        } else if contains(&[
            "no space left",
            "disk full",
            "not enough space",
            "quota exceeded",
        ]) {
            (
                "108",
                "The destination has insufficient space.",
                "Check free space and storage quotas on the destination, then try again.",
            )
        } else if contains(&[
            "connection reset",
            "broken pipe",
            "connection closed unexpectedly",
            "unexpected eof",
            "forcibly closed by the remote host",
        ]) {
            (
                "109",
                "The connection ended unexpectedly.",
                "Check the remote service and reconnect. Review the operation’s result before repeating a change.",
            )
        } else if contains(&[
            "queue is full",
            "queue full",
            "resource busy",
            "address already in use",
        ]) {
            (
                "110",
                "The required resource is busy.",
                "Wait for the current operation to finish, or choose another listen address or port where applicable.",
            )
        } else if contains(&[
            "network is unreachable",
            "no route to host",
            "network location cannot be reached",
        ]) {
            (
                "118",
                "The remote network could not be reached.",
                "Check your network connection, VPN and route to the remote host before reconnecting.",
            )
        } else if contains(&["unsupported", "not supported", "no embedded provider"]) {
            (
                "115",
                "",
                "Review the supported options and select an available provider. Retrying the same unsupported operation will not add support.",
            )
        } else if contains(&[
            "invalid",
            "must not be empty",
            "must be between",
            "is required",
            "cannot be empty",
        ]) {
            (
                "116",
                "",
                "Correct the supplied settings or input, then try again.",
            )
        } else if contains(&["remote file changed", "remote version changed"]) {
            (
                "117",
                "",
                "Compare your edits with the current remote version. Choose replacement only after reviewing both versions.",
            )
        } else {
            ("000", "", context.next_step())
        };
        let summary = if title.is_empty() {
            let first = details
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("The operation could not be completed.");
            // Keep useful, already-authored validation messages; do not expose Rust debug dumps as the headline.
            if first.contains("OsError {") || first.contains("Error {") {
                "The operation could not be completed.".into()
            } else {
                bounded(first, 240)
            }
        } else {
            title.into()
        };
        Self {
            code: format!("TC-{}-{suffix}", context.prefix()),
            summary,
            next_step: next.into(),
            details,
        }
    }
    pub fn report(&self) -> String {
        format!(
            "{}: {}\nNext step: {}\nDetails: {}",
            self.code, self.summary, self.next_step, self.details
        )
    }
}

fn bounded(raw: &str, limit: usize) -> String {
    let mut out = String::new();
    for c in raw.chars() {
        if out.len() + c.len_utf8() > limit {
            break;
        }
        if !c.is_control() || matches!(c, '\n' | '\t') {
            out.push(c);
        }
    }
    out
}

/// Error details are never copied verbatim into the UI or clipboard.
pub fn sanitize(raw: &str) -> String {
    use std::sync::OnceLock;
    static RULES: OnceLock<Vec<(regex::Regex, &'static str)>> = OnceLock::new();
    let rules=RULES.get_or_init(|| [
        (r"(?s)-----BEGIN [^-\r\n]*PRIVATE KEY-----.*?(?:-----END [^-\r\n]*PRIVATE KEY-----|$)","[private key redacted]"),
        (r"(?i)(authorization\s*:\s*)[^\r\n]+","${1}[redacted]"),
        (r"([a-zA-Z][a-zA-Z0-9+.-]*://[^/\s:@]+:)[^@/\s]+@","${1}[redacted]@"),
        (r#"(?i)(\"?(?:password|passwd|passphrase|access_token|refresh_token|token|api_key|client_secret|secret|private_key|authorization)\"?\s*[=:]\s*)(?:\"[^\"]*\"|'[^']*'|[^\s,;&]+)"#,"${1}[redacted]"),
        (r#"(?i)((?:--password|--passphrase|--token|-pw)\s+)(?:"[^"]*"|'[^']*'|[^\s]+)"#,"${1}[redacted]"),
        (r"\x1b\[[0-?]*[ -/]*[@-~]", ""),
    ].into_iter().map(|(pattern,replacement)|(regex::Regex::new(pattern).expect("static diagnostic redaction rule"),replacement)).collect());
    let mut out = raw.chars().take(32768).collect::<String>();
    for (pattern, replacement) in rules {
        out = pattern.replace_all(&out, *replacement).into_owned();
    }
    bounded(&out, 16384)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn windows_and_unix_causes_share_codes_without_guessing_numeric_os_errors() {
        for text in [
            "Connection refused (os error 111)",
            "No connection could be made because the target machine actively refused it. (os error 10061)",
        ] {
            assert_eq!(Diagnostic::new(Context::Ssh, text).code, "TC-SSH-101");
        }
        assert_eq!(
            Diagnostic::new(Context::Files, "os error 13").code,
            "TC-FILES-000"
        );
    }
    #[test]
    fn json_credentials_and_cli_secret_flags_are_redacted() {
        let d = Diagnostic::new(
            Context::App,
            r#"{"authorization":"Bearer SECRET_JSON","client_secret":"CLIENT_SECRET"} --password 'CLI SECRET' --token CLI_TOKEN"#,
        );
        for secret in ["SECRET_JSON", "CLIENT_SECRET", "CLI SECRET", "CLI_TOKEN"] {
            assert!(!d.report().contains(secret), "Leaked {secret}");
        }
    }
    #[test]
    fn refused_connection_has_a_stable_code_and_safe_next_step() {
        let d = Diagnostic::new(
            Context::Ssh,
            "connect to example.test:22: Connection refused (os error 111)",
        );
        assert_eq!(d.code, "TC-SSH-101");
        assert_eq!(d.summary, "The connection was refused.");
        assert!(d.next_step.contains("port"));
        assert!(d.details.contains("example.test:22"));
    }
    #[test]
    fn display_failure_does_not_recommend_forcing_another_backend() {
        let d = Diagnostic::new(
            Context::Startup,
            "WinitEventLoop(OsError { error: XNotSupported(XOpenDisplayFailed) })",
        );
        assert_eq!(d.code, "TC-START-114");
        assert!(d.next_step.contains("desktop"));
        assert!(!d.next_step.contains("unset"));
    }
    #[test]
    fn trust_errors_never_suggest_disabling_verification() {
        for text in ["host key mismatch", "certificate verification failed"] {
            let d = Diagnostic::new(Context::Ssh, text);
            assert!(d.next_step.contains("verify"));
            assert!(!d.next_step.contains("disable"));
            assert!(!d.next_step.contains("ignore"));
        }
    }
    #[test]
    fn report_redacts_credentials_urls_tokens_and_private_keys() {
        let d = Diagnostic::new(
            Context::Browser,
            "password=hunter42\nAuthorization: Bearer abc123\nhttps://alice:p455@example.test/?token=tok123\n-----BEGIN OPENSSH PRIVATE KEY-----\nprivateblob\n-----END OPENSSH PRIVATE KEY-----",
        );
        let report = d.report();
        for secret in ["hunter42", "abc123", "p455", "tok123", "privateblob"] {
            assert!(!report.contains(secret), "Leaked {secret}");
        }
        assert!(report.contains("example.test"));
    }
    #[test]
    fn unknown_error_keeps_its_context_without_guessing_a_cause() {
        let d = Diagnostic::new(
            Context::Transfer,
            "Remote operation failed with extension status 891",
        );
        assert_eq!(d.code, "TC-TRANSFER-000");
        assert!(d.summary.contains("891"));
        assert!(!d.next_step.contains("password"));
    }
    #[test]
    fn domains_are_unique_and_error_reports_are_bounded() {
        let prefixes = Context::ALL
            .iter()
            .map(|c| c.prefix())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(prefixes.len(), Context::ALL.len());
        let d = Diagnostic::new(Context::App, &"x".repeat(30000));
        assert!(d.details.len() < 20000);
        assert!(d.summary.chars().count() <= 240);
    }
    #[test]
    fn harmless_password_authentication_description_is_retained() {
        let d = Diagnostic::new(
            Context::Ssh,
            "Password authentication failed; check the selected username.",
        );
        assert!(d.details.contains("Password authentication failed"));
        assert_eq!(d.code, "TC-SSH-104");
    }
}
