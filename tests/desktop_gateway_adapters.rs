use harbor_workspace::{
    connection,
    model::{Preferences, Session},
};
#[test]
fn gateway_profiles_cannot_fall_back_to_direct_installed_clients() {
    for protocol in ["RDP", "VNC", "Telnet"] {
        let session = Session {
            protocol: protocol.into(),
            host: "target.example".into(),
            jump: "alice@gateway.example:22".into(),
            ..Default::default()
        };
        let result = connection::launch(&session, &Preferences::default());
        assert!(
            result.is_err(),
            "{protocol} silently ignored its SSH gateway"
        );
        assert!(
            result.unwrap_err().to_lowercase().contains("gateway"),
            "{protocol} must explain its unavailable gateway path"
        );
    }
}
