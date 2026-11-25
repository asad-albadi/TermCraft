pub mod model;
pub use model::{protocol_port, ssh_arguments};
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol_defaults_match_sessions() {
        assert_eq!(protocol_port("SSH"), 22);
        assert_eq!(protocol_port("RDP"), 3389);
        assert_eq!(protocol_port("VNC"), 5900);
        assert_eq!(protocol_port("Telnet"), 23);
    }
    #[test]
    fn ssh_preserves_key_paths_without_shell_interpolation() {
        let a = ssh_arguments(
            "example.test",
            "alice",
            2222,
            "/tmp/key with spaces",
            "bastion",
            true,
            true,
        )
        .unwrap();
        assert!(a.windows(2).any(|a| a == ["-i", "/tmp/key with spaces"]));
        assert_eq!(a.last().unwrap(), "alice@example.test");
        assert!(a.contains(&"-Y".to_string()));
    }
    #[test]
    fn rejects_option_injection_and_bad_hosts() {
        assert!(ssh_arguments("-oProxyCommand=bad", "", 22, "", "", false, false).is_err());
        assert!(ssh_arguments("server name", "", 22, "", "", false, false).is_err());
    }
}
pub mod connection;
pub mod http_server;
pub mod process;
pub mod serial;
pub mod services;
pub mod terminal;

pub mod branding;

pub mod remote_files;

pub mod ssh;

pub mod workspace;

pub mod sessions;

#[path = "sessions/shared_network.rs"]
pub mod shared_network;

pub mod platform;

pub mod cli;
pub mod ipc;

pub mod credentials;

pub mod desktop;

pub mod protocols;

pub mod tools;

pub mod macros;

pub mod x11;

pub mod browser;

pub mod runtime;

pub mod environment;
pub mod packages;

pub mod plugins;
