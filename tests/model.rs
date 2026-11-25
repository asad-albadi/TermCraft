use harbor_workspace::model::*;
#[test]
fn roundtrip_keeps_sessions_and_macros() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/test-roundtrip.json");
    let mut s = Store::default();
    s.sessions.push(Session {
        name: "lab".into(),
        host: "example.test".into(),
        ..Default::default()
    });
    s.macros.push(Macro {
        script: None,
        name: "uptime".into(),
        text: "uptime".into(),
        append_enter: true,
    });
    s.save(&root).unwrap();
    let loaded = Store::load(&root).unwrap();
    assert_eq!(loaded.sessions[0].host, "example.test");
    assert_eq!(loaded.macros[0].text, "uptime");
    std::fs::remove_file(root).unwrap();
}
#[test]
fn tunnel_does_not_interpolate_destination() {
    let mut t = Tunnel {
        host: "example.test".into(),
        destination: "db.internal".into(),
        ..Default::default()
    };
    let a = t.args().unwrap();
    assert!(a.contains(&"127.0.0.1:8080:db.internal:80".into()));
    t.destination = "$(touch bad)".into();
    assert!(t.args().is_err());
}
#[test]
fn file_copy_never_overwrites_a_destination() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/copy-test");
    std::fs::create_dir_all(&root).unwrap();
    let source = root.join("source.txt");
    let dest = root.join("destination.txt");
    std::fs::write(&source, "new").unwrap();
    std::fs::write(&dest, "keep").unwrap();
    assert!(copy_new_file(&source, &dest).is_err());
    assert_eq!(std::fs::read_to_string(&dest).unwrap(), "keep");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ipv6_tunnels_preserve_address_boundaries() {
    let mut tunnel = Tunnel {
        host: "example.test".into(),
        bind: "::1".into(),
        destination: "2001:db8::10".into(),
        ..Default::default()
    };
    assert!(
        tunnel
            .args()
            .unwrap()
            .contains(&"[::1]:8080:[2001:db8::10]:80".into())
    );
    tunnel.mode = "Dynamic".into();
    assert!(tunnel.args().unwrap().contains(&"[::1]:8080".into()));
}
#[test]
fn unknown_tunnel_modes_are_rejected() {
    let tunnel = Tunnel {
        host: "example.test".into(),
        mode: "Typo".into(),
        ..Default::default()
    };
    assert!(tunnel.args().is_err());
}
#[test]
fn layout_preferences_roundtrip_and_old_stores_load() {
    let preferences: Preferences = serde_json::from_value(
        serde_json::json!({"sidebar_visible": false, "sidebar_width": 310.0}),
    )
    .unwrap();
    let saved = serde_json::to_value(preferences).unwrap();
    assert_eq!(saved["sidebar_visible"], false);
    assert_eq!(saved["sidebar_width"], 310.0);
    let old: Preferences = serde_json::from_str("{}").unwrap();
    assert_eq!(serde_json::to_value(old).unwrap()["sidebar_visible"], true);
}

#[test]
fn ssh_file_transfer_keeps_connection_options() {
    let ssh = Session {
        host: "example.test".into(),
        port: 2222,
        user: "alice".into(),
        key: "/keys/with spaces".into(),
        jump: "bob@gateway:2200".into(),
        command: "uname".into(),
        ..Default::default()
    };
    let sftp = ssh.sftp_session().unwrap();
    assert_eq!(sftp.protocol, "SFTP");
    assert_eq!(sftp.port, ssh.port);
    assert_eq!(sftp.user, ssh.user);
    assert_eq!(sftp.key, ssh.key);
    assert_eq!(sftp.jump, ssh.jump);
    assert!(sftp.command.is_empty());
    assert!(!sftp.x11);
    assert!(
        Session {
            protocol: "RDP".into(),
            ..Default::default()
        }
        .sftp_session()
        .is_err()
    );
}

#[test]
fn old_ssh_sessions_keep_openssh_configuration_compatibility() {
    let old: Session = serde_json::from_str(r#"{"host":"legacy-alias"}"#).unwrap();
    assert_eq!(old.ssh_engine, SshEngine::OpenSsh);
    assert_eq!(Session::default().ssh_engine, SshEngine::Embedded);
    let saved = serde_json::to_string(&Session::default()).unwrap();
    let loaded: Session = serde_json::from_str(&saved).unwrap();
    assert_eq!(loaded.ssh_engine, SshEngine::Embedded);
}
