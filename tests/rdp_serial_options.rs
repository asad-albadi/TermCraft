use harbor_workspace::desktop::rdp_options::{RdpOptions, SerialMapping};
#[test]
fn serial_mappings_are_explicit_bounded_and_reject_ambiguous_devices() {
    let mapping = SerialMapping {
        name: "COM1".into(),
        path: "/dev/ttyS0".into(),
        read_only: true,
    };
    let mut options = RdpOptions {
        serial_mappings: vec![mapping.clone()],
        ..Default::default()
    };
    assert!(options.validate().is_ok());
    options.serial_mappings.push(SerialMapping {
        name: "com1".into(),
        ..mapping.clone()
    });
    assert!(options.validate().is_err());
    for name in ["COM0", "COM257", "COM01", "/dev/ttyS0", "COM1;other"] {
        options.serial_mappings = vec![SerialMapping {
            name: name.into(),
            ..mapping.clone()
        }];
        assert!(options.validate().is_err(), "{name}");
    }
    options.serial_mappings = vec![SerialMapping {
        path: "relative-device".into(),
        ..mapping
    }];
    assert!(options.validate().is_err());
}
#[test]
fn explicit_serial_mappings_roundtrip_and_are_not_silently_sent_to_installed_clients() {
    let mut session = harbor_workspace::model::Session {
        protocol: "RDP".into(),
        host: "example".into(),
        ..Default::default()
    };
    session.rdp_options.serial_mappings.push(SerialMapping {
        name: "COM2".into(),
        path: "/dev/ttyS1".into(),
        read_only: false,
    });
    let decoded: harbor_workspace::model::Session =
        serde_json::from_slice(&serde_json::to_vec(&session).unwrap()).unwrap();
    assert_eq!(
        decoded.rdp_options.serial_mappings,
        session.rdp_options.serial_mappings
    );
    assert!(harbor_workspace::desktop::rdp_options::windows_profile(&session).is_err());
    assert!(harbor_workspace::connection::freerdp_arguments(&session).is_err());
}
