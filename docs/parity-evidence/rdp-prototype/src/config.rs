// Adapted from IronRDP 0.17.0 screenshot example; MIT OR Apache-2.0. See adjacent licenses.
use ironrdp::{
    connector::{self, Credentials},
    pdu::{
        gcc::KeyboardType,
        rdp::{
            capability_sets::MajorPlatformType,
            client_info::{CompressionType, PerformanceFlags, TimezoneInfo},
        },
    },
};
pub fn build_config(
    username: String,
    password: String,
    domain: Option<String>,
    compression_enabled: bool,
    compression_level: u32,
) -> anyhow::Result<connector::Config> {
    let compression_type = if compression_enabled {
        Some(compression_type_from_level(compression_level)?)
    } else {
        None
    };

    Ok(connector::Config {
        credentials: Credentials::UsernamePassword { username, password },
        domain,
        enable_tls: false, // This example does not expose any frontend.
        enable_credssp: true,
        keyboard_type: KeyboardType::IbmEnhanced,
        keyboard_subtype: 0,
        keyboard_layout: 0,
        keyboard_functional_keys_count: 12,
        ime_file_name: String::new(),
        dig_product_id: String::new(),
        desktop_size: connector::DesktopSize {
            width: 1280,
            height: 1024,
        },
        bitmap: None,
        client_build: 0,
        client_name: "ironrdp-screenshot-example".to_owned(),
        client_dir: "C:\\Windows\\System32\\mstscax.dll".to_owned(),

        #[cfg(windows)]
        platform: MajorPlatformType::WINDOWS,
        #[cfg(target_os = "macos")]
        platform: MajorPlatformType::MACINTOSH,
        #[cfg(target_os = "ios")]
        platform: MajorPlatformType::IOS,
        #[cfg(target_os = "linux")]
        platform: MajorPlatformType::UNIX,
        #[cfg(target_os = "android")]
        platform: MajorPlatformType::ANDROID,
        #[cfg(target_os = "freebsd")]
        platform: MajorPlatformType::UNIX,
        #[cfg(target_os = "dragonfly")]
        platform: MajorPlatformType::UNIX,
        #[cfg(target_os = "openbsd")]
        platform: MajorPlatformType::UNIX,
        #[cfg(target_os = "netbsd")]
        platform: MajorPlatformType::UNIX,

        enable_server_pointer: false, // Disable custom pointers (there is no user interaction anyway).
        request_data: None,
        autologon: false,
        enable_audio_playback: false,
        compression_type,
        pointer_software_rendering: true,
        multitransport_flags: None,
        performance_flags: PerformanceFlags::default(),
        desktop_scale_factor: 0,
        hardware_id: None,
        license_cache: None,
        timezone_info: TimezoneInfo::default(),
        alternate_shell: String::new(),
        work_dir: String::new(),
    })
}

fn compression_type_from_level(level: u32) -> anyhow::Result<CompressionType> {
    match level {
        0 => Ok(CompressionType::K8),
        1 => Ok(CompressionType::K64),
        2 => Ok(CompressionType::Rdp6),
        3 => Ok(CompressionType::Rdp61),
        _ => anyhow::bail!("Invalid compression level. Valid values are 0, 1, 2, 3."),
    }
}
