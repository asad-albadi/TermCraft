use ironrdp_connector::{BitmapConfig, Config, Credentials, DesktopSize};
use ironrdp_pdu::{
    gcc::{ConnectionType, KeyboardType},
    rdp::{
        capability_sets::{MajorPlatformType, RailSupportLevel, client_codecs_capabilities},
        client_info::{PerformanceFlags, TimezoneInfo},
    },
};
pub fn connector(wire: &crate::wire::Config) -> Config {
    Config {
        desktop_size: DesktopSize {
            width: wire.width,
            height: wire.height,
        },
        monitor_layout: None,
        desktop_scale_factor: 0,
        desktop_physical_size: None,
        desktop_orientation: None,
        device_scale_factor: None,
        monitor_extended_layout: None,
        relative_mouse_input: false,
        enable_tls: false,
        enable_credssp: true,
        enable_standard_rdp_security: false,
        credentials: Credentials::UsernamePassword {
            username: wire.user.clone(),
            password: String::new(),
        },
        domain: if wire.options.domain.is_empty() {
            None
        } else {
            Some(wire.options.domain.clone())
        },
        client_build: 0,
        client_name: "TermCraft".into(),
        keyboard_type: KeyboardType::IBM_ENHANCED,
        keyboard_subtype: 0,
        keyboard_functional_keys_count: 12,
        keyboard_layout: wire.options.keyboard_layout,
        connection_type: ConnectionType::Lan,
        ime_file_name: String::new(),
        bitmap: Some(BitmapConfig {
            lossy_compression: false,
            color_depth: 32,
            codecs: client_codecs_capabilities(&[]).expect("default RemoteFX capability set"),
        }),
        dig_product_id: String::new(),
        client_dir: "TermCraft".into(),
        alternate_shell: String::new(),
        work_dir: String::new(),
        remote_application_mode: false,
        rail_support_level: RailSupportLevel::empty(),
        platform: if cfg!(windows) {
            MajorPlatformType::WINDOWS
        } else if cfg!(target_os = "macos") {
            MajorPlatformType::MACINTOSH
        } else {
            MajorPlatformType::UNIX
        },
        hardware_id: None,
        request_data: None,
        autologon: true,
        enable_audio_playback: false,
        enable_audio_capture: false,
        performance_flags: PerformanceFlags::default(),
        license_cache: None,
        timezone_info: TimezoneInfo::default(),
        compression_type: None,
        enable_server_pointer: true,
        pointer_software_rendering: true,
        multitransport_flags: None,
        support_dyn_vc_gfx_protocol: true,
    }
}

/// Request session zero; do not advertise accepting automatic server redirects.
pub fn cluster(
    options: &crate::rdp_options::RdpOptions,
) -> Option<ironrdp_pdu::gcc::ClientClusterData> {
    options
        .administrative
        .then_some(ironrdp_pdu::gcc::ClientClusterData {
            flags: ironrdp_pdu::gcc::RedirectionFlags::REDIRECTED_SESSION_FIELD_VALID,
            redirection_version: ironrdp_pdu::gcc::RedirectionVersion::V1,
            redirected_session_id: 0,
        })
}

#[cfg(test)]
mod tests {
    #[test]
    fn advertises_classic_remotefx_and_dynamic_graphics_with_the_existing_color_policy() {
        use ironrdp_pdu::rdp::capability_sets::{CodecProperty, RemoteFxContainer};
        let wire: crate::wire::Config = serde_json::from_value(serde_json::json!({
            "version": crate::wire::VERSION, "host": "localhost", "port": 3389,
            "user": "fixture", "width": 640, "height": 480, "timeout_secs": 5
        }))
        .unwrap();
        let config = super::connector(&wire);
        assert!(config.support_dyn_vc_gfx_protocol);
        let bitmap = config.bitmap.unwrap();
        assert_eq!(bitmap.color_depth, 32);
        assert!(!bitmap.lossy_compression);
        assert!(bitmap.codecs.0.iter().any(|c| matches!(
            c.property,
            CodecProperty::RemoteFx(RemoteFxContainer::ClientContainer(_))
        )));
    }

    #[test]
    fn administrative_session_requests_console_without_advertising_redirects() {
        let options = crate::rdp_options::RdpOptions::default();
        assert!(super::cluster(&options).is_none());
        let options = crate::rdp_options::RdpOptions {
            administrative: true,
            ..options
        };
        let encoded = ironrdp_core::encode_vec(&super::cluster(&options).unwrap()).unwrap();
        assert_eq!(encoded, [2, 0, 0, 0, 0, 0, 0, 0]);
    }
}
