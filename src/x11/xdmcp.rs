//! Bounded XDMCP discovery. Replies are unauthenticated advertisements, never credentials.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    net::{IpAddr, Ipv4Addr, SocketAddr, UdpSocket},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QueryMode {
    #[default]
    Direct,
    Indirect,
    Broadcast,
}
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClipboardPolicy {
    #[default]
    Disabled,
    Bidirectional,
    WindowsBidirectional,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct XdmcpConfig {
    pub mode: QueryMode,
    pub selected_endpoint: Option<SocketAddr>,
    pub display: Option<u16>,
    pub monitor: Option<String>,
    pub fullscreen: bool,
    pub keyboard: String,
    pub clipboard: ClipboardPolicy,
    /// Explicit local parent X11 CLIPBOARD endpoint. No implicit DISPLAY clipboard access.
    pub clipboard_display: Option<String>,
    /// Optional selected Xauthority; otherwise read the inherited authority without modifying it.
    pub clipboard_authority: Option<std::path::PathBuf>,
    /// Address advertised to the manager. The private Windows and macOS engines also bind to this address.
    pub advertise_address: Option<Ipv4Addr>,
    /// Explicit permission for this session's unencrypted XDMCP and X11 TCP listener.
    pub allow_network_listener: bool,
    /// Explicit broadcast network choice. Never inferred from host/interface enumeration.
    pub discovery_network: Option<BroadcastNetwork>,
}
impl XdmcpConfig {
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BroadcastNetwork {
    pub interface: Ipv4Addr,
    pub prefix: u8,
}
impl BroadcastNetwork {
    fn mask(self) -> Result<u32, String> {
        if !(16..=30).contains(&self.prefix)
            || self.interface.is_unspecified()
            || self.interface.is_multicast()
            || self.interface.is_loopback()
        {
            return Err(
                "Choose a local IPv4 interface and a /16 through /30 broadcast network".into(),
            );
        }
        Ok(u32::MAX << (32 - self.prefix))
    }
    fn destination(self, port: u16) -> Result<SocketAddr, String> {
        let mask = self.mask()?;
        Ok(SocketAddr::new(
            IpAddr::V4(Ipv4Addr::from(u32::from(self.interface) | !mask)),
            port,
        ))
    }
    fn admits(self, address: IpAddr) -> bool {
        matches!(address, IpAddr::V4(ip) if self.mask().is_ok_and(|mask| u32::from(ip)&mask == u32::from(self.interface)&mask))
    }
}
#[derive(Clone, Debug)]
pub struct DiscoveryRequest {
    pub endpoint: SocketAddr,
    pub mode: QueryMode,
    pub network: Option<BroadcastNetwork>,
}
impl DiscoveryRequest {
    pub fn direct(endpoint: SocketAddr) -> Self {
        Self {
            endpoint,
            mode: QueryMode::Direct,
            network: None,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.endpoint.port() == 0
            || self.endpoint.ip().is_unspecified()
            || self.endpoint.ip().is_multicast()
        {
            return Err("Choose a unicast XDMCP endpoint and nonzero port".into());
        }
        if self.mode == QueryMode::Broadcast {
            self.network
                .ok_or("Broadcast discovery requires an explicit interface/network choice")?
                .mask()?;
        } else if self.endpoint.ip() == IpAddr::V4(Ipv4Addr::BROADCAST) {
            return Err("Choose Broadcast mode and an explicit network".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiscoveredEndpoint {
    pub address: SocketAddr,
    pub hostname: String,
    pub status: String,
}
fn field<'a>(bytes: &mut &'a [u8]) -> Option<&'a [u8]> {
    if bytes.len() < 2 {
        return None;
    };
    let n = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;
    if n > 256 || bytes.len() < n + 2 {
        return None;
    };
    let result = &bytes[2..n + 2];
    *bytes = &bytes[n + 2..];
    Some(result)
}
fn advertisement(bytes: &[u8]) -> Option<String> {
    let value = std::str::from_utf8(bytes).ok()?;
    if value.chars().any(char::is_control) {
        return None;
    }
    Some(value.to_string())
}
fn parse_willing(bytes: &[u8]) -> Option<(String, String)> {
    if bytes.len() < 6
        || bytes.len() > 1024
        || bytes[..4] != [0, 1, 0, 5]
        || u16::from_be_bytes([bytes[4], bytes[5]]) as usize != bytes.len() - 6
    {
        return None;
    }
    let mut body = &bytes[6..];
    let _authentication = field(&mut body)?;
    let host = advertisement(field(&mut body)?)?;
    let status = advertisement(field(&mut body)?)?;
    if !body.is_empty() {
        return None;
    }
    Some((host, status))
}
/// Execute on a worker. At most 3 packets, 128 replies, 32 admitted endpoints and 600 ms.
/// XDMCP has no QUERY transaction ID; fresh ephemeral socket and source checks correlate replies.
pub fn discover(
    request: &DiscoveryRequest,
    cancelled: &AtomicBool,
) -> Result<Vec<DiscoveredEndpoint>, String> {
    request.validate()?;
    check_cancelled(cancelled)?;
    let bind = match request
        .network
        .filter(|_| request.mode == QueryMode::Broadcast)
    {
        Some(network) => SocketAddr::new(network.interface.into(), 0),
        None => {
            if request.endpoint.is_ipv4() {
                "0.0.0.0:0".parse().unwrap()
            } else {
                "[::]:0".parse().unwrap()
            }
        }
    };
    let socket =
        UdpSocket::bind(bind).map_err(|e| format!("Could not bind XDMCP discovery: {e}"))?;
    socket
        .set_read_timeout(Some(Duration::from_millis(30)))
        .map_err(|e| e.to_string())?;
    let target = if request.mode == QueryMode::Broadcast {
        socket.set_broadcast(true).map_err(|e| e.to_string())?;
        request
            .network
            .unwrap()
            .destination(request.endpoint.port())?
    } else {
        request.endpoint
    };
    let opcode = match request.mode {
        QueryMode::Direct => 2,
        QueryMode::Indirect => 3,
        QueryMode::Broadcast => 1,
    };
    let packet = [0, 1, 0, opcode, 0, 1, 0];
    let start = Instant::now();
    let mut next_send = Duration::ZERO;
    let mut sends = 0;
    let mut packets = 0;
    let mut results = BTreeMap::new();
    while start.elapsed() < Duration::from_millis(600) && packets < 128 && results.len() < 32 {
        check_cancelled(cancelled)?;
        if sends < 3 && start.elapsed() >= next_send {
            socket
                .send_to(&packet, target)
                .map_err(|e| format!("XDMCP query failed: {e}"))?;
            sends += 1;
            next_send = Duration::from_millis(sends * 200);
        }
        let mut buffer = [0; 1025];
        match socket.recv_from(&mut buffer) {
            Ok((n, source)) => {
                packets += 1;
                let allowed = if request.mode == QueryMode::Broadcast {
                    source.port() == target.port() && request.network.unwrap().admits(source.ip())
                } else {
                    source == target
                };
                if allowed {
                    if let Some((hostname, status)) = parse_willing(&buffer[..n]) {
                        results.insert(
                            source,
                            DiscoveredEndpoint {
                                address: source,
                                hostname,
                                status,
                            },
                        );
                    }
                }
            }
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) => {}
            // Winsock reports an ICMP port-unreachable on a previous datagram
            // as a receive error. A peer can close after answering our first query.
            #[cfg(windows)]
            Err(e)
                if e.raw_os_error()
                    == Some(windows_sys::Win32::Networking::WinSock::WSAECONNRESET) => {}
            Err(e) => return Err(format!("XDMCP reply failed: {e}")),
        }
    }
    check_cancelled(cancelled)?;
    Ok(results.into_values().collect())
}
pub(crate) fn check_cancelled(cancelled: &AtomicBool) -> Result<(), String> {
    if cancelled.load(Ordering::Relaxed) {
        Err("XDMCP cancelled".into())
    } else {
        Ok(())
    }
}
pub struct Discovery {
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<Vec<DiscoveredEndpoint>, String>>,
}
impl Discovery {
    pub fn start(request: DiscoveryRequest) -> Result<Self, String> {
        request.validate()?;
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let (sender, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("termcraft-xdmcp-discovery".into())
            .spawn(move || {
                let _ = sender.send(discover(&request, &stop));
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { cancelled, result })
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed)
    }
    pub fn try_finish(&self) -> Option<Result<Vec<DiscoveredEndpoint>, String>> {
        match self.result.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err("XDMCP discovery worker ended".into())),
        }
    }
}
impl Drop for Discovery {
    fn drop(&mut self) {
        self.cancel()
    }
}

#[cfg(any(target_os = "linux", all(windows, target_arch = "x86_64")))]
fn valid_layout_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id.as_bytes()[0].is_ascii_lowercase()
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

#[cfg(any(target_os = "linux", all(windows, target_arch = "x86_64")))]
pub(super) fn registered_layout(catalog: &str, id: &str) -> bool {
    catalog
        .lines()
        .map(str::trim)
        .skip_while(|line| *line != "! layout")
        .skip(1)
        .take_while(|line| !line.starts_with('!'))
        .any(|line| line.split_whitespace().next() == Some(id))
}

#[cfg(all(windows, target_arch = "x86_64"))]
pub(super) fn windows_options(config: &XdmcpConfig, width: u32, height: u32) -> Result<(), String> {
    if config.mode == QueryMode::Broadcast {
        return Err(
            "Select a discovered endpoint and choose Direct or Indirect before connecting".into(),
        );
    }
    if !config.allow_network_listener {
        return Err("Explicitly allow unencrypted XDMCP and its X11 TCP listener for this session on a trusted network".into());
    }
    if config
        .display
        .is_some_and(|display| !(1..=59535).contains(&display))
        || !(200..=8192).contains(&width)
        || !(200..=8192).contains(&height)
    {
        return Err("Choose display 1–59535 and desktop dimensions 200–8192".into());
    }
    windows_monitor(config.monitor.as_deref(), config.fullscreen)?;
    if !config.keyboard.is_empty() && !valid_layout_id(&config.keyboard) {
        return Err("Enter one XKB keyboard layout ID (for example us, de, or fr), or clear it to inherit the host keyboard".into());
    }
    if config.clipboard == ClipboardPolicy::Bidirectional
        || (config.clipboard == ClipboardPolicy::WindowsBidirectional
            && (config.clipboard_display.is_some() || config.clipboard_authority.is_some()))
    {
        return Err("Clipboard sharing with a parent X11 display requires the Linux engine. Select Windows clipboard explicitly for this session.".into());
    }
    if config.advertise_address.is_some_and(|address| {
        address.is_unspecified() || address.is_multicast() || address.is_broadcast()
    }) {
        return Err("Choose a local unicast IPv4 address to advertise to the XDMCP manager".into());
    }
    Ok(())
}

#[cfg(windows)]
pub(super) fn windows_monitor(
    monitor: Option<&str>,
    fullscreen: bool,
) -> Result<Option<u16>, String> {
    let Some(monitor) = monitor else {
        return Ok(None);
    };
    if !fullscreen {
        return Err("Monitor selection requires full screen mode".into());
    }
    monitor
        .parse::<u16>()
        .ok()
        .filter(|value| *value > 0 && monitor.bytes().all(|byte| byte.is_ascii_digit()))
        .map(Some)
        .ok_or_else(|| "Choose a Windows monitor number starting at 1".into())
}

/// Validate the resolved endpoint. The provider must reserve the returned local address before launch.
#[cfg(all(windows, target_arch = "x86_64"))]
pub(super) fn windows_network(
    config: &XdmcpConfig,
    width: u32,
    height: u32,
) -> Result<(std::net::SocketAddrV4, Ipv4Addr), String> {
    windows_options(config, width, height)?;
    let endpoint = config
        .selected_endpoint
        .ok_or("Discover and select an XDMCP endpoint first")?;
    DiscoveryRequest::direct(endpoint).validate()?;
    let SocketAddr::V4(endpoint) = endpoint else {
        return Err("The private Windows XDMCP engine requires a unicast IPv4 endpoint".into());
    };
    let address = config.advertise_address.unwrap_or(Ipv4Addr::LOCALHOST);
    if !endpoint.ip().is_loopback() && address.is_loopback() {
        return Err(
            "Choose an explicit local non-loopback IPv4 address to advertise to this XDMCP manager"
                .into(),
        );
    }
    Ok((endpoint, address))
}

/// Validated Xephyr options. Broadcast is discovery-only; connect to a selected endpoint.
pub fn engine_arguments(
    config: &XdmcpConfig,
    display: u16,
    width: u32,
    height: u32,
) -> Result<Vec<String>, String> {
    let endpoint = config
        .selected_endpoint
        .ok_or("Discover and select an XDMCP endpoint first")?;
    DiscoveryRequest::direct(endpoint).validate()?;
    if !endpoint.is_ipv4() {
        return Err("The owned Xephyr XDMCP candidate currently requires IPv4".into());
    }
    if !config.allow_network_listener {
        return Err("XDMCP needs an inbound X11 TCP listener. Explicitly allow its all-interface listener for this session on a trusted network".into());
    }
    if display < 1
        || display > 59535
        || !(200..=8192).contains(&width)
        || !(200..=8192).contains(&height)
    {
        return Err("Choose display 1–59535 and desktop dimensions 200–8192".into());
    }
    if config.display.is_some_and(|chosen| chosen != display) {
        return Err("Conflicting XDMCP display selection".into());
    }
    if config.clipboard == ClipboardPolicy::WindowsBidirectional {
        return Err("The Windows clipboard setting requires the private Windows engine".into());
    }
    if config.clipboard == ClipboardPolicy::Bidirectional {
        #[cfg(target_os = "linux")]
        {
            let parent = config
                .clipboard_display
                .as_deref()
                .ok_or("Select the local parent X11 clipboard display explicitly")?;
            let (number, _) = super::xdmcp_clipboard::validate_display(parent)?;
            if number == display {
                return Err(
                    "Parent clipboard display must differ from the owned nested display".into(),
                );
            }
        }
        #[cfg(not(target_os = "linux"))]
        return Err("Bidirectional XDMCP clipboard requires the qualified Linux engine".into());
    }
    if config.mode == QueryMode::Broadcast {
        return Err(
            "Select a discovered endpoint and choose Direct or Indirect before connecting".into(),
        );
    }
    let source = config.advertise_address.unwrap_or(Ipv4Addr::LOCALHOST);
    if source.is_unspecified()
        || source.is_loopback()
        || source.is_multicast()
        || (!endpoint.ip().is_loopback()
            && (config.advertise_address.is_none() || source.is_loopback()))
    {
        return Err(
            "Choose the reachable local IPv4 address to advertise to this display manager".into(),
        );
    }
    let mut args = vec![
        format!(":{display}"),
        "-port".into(),
        endpoint.port().to_string(),
        match config.mode {
            QueryMode::Indirect => "-indirect",
            _ => "-query",
        }
        .into(),
        endpoint.ip().to_string(),
        "-from".into(),
        source.to_string(),
        "-listen".into(),
        "tcp".into(),
        "-nopn".into(),
    ];
    // An indirect chooser's first client disconnects after selection. Xephyr
    // must reset and requery to reach the selected manager; -once would exit.
    if config.mode == QueryMode::Direct {
        args.push("-once".into());
    }
    if config.monitor.is_none() {
        args.extend(["-screen".into(), format!("{width}x{height}")]);
    }
    if config.fullscreen && config.monitor.is_none() {
        args.push("-fullscreen".into());
    }
    if let Some(monitor) = &config.monitor {
        if !config.fullscreen {
            return Err("Monitor selection requires full screen mode".into());
        }
        if monitor.is_empty()
            || monitor.len() > 128
            || !monitor
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.".contains(&b))
        {
            return Err("Choose a valid XRandR output name".into());
        }
        args.extend(["-output".into(), monitor.clone()]);
    }
    if !config.keyboard.is_empty() {
        #[cfg(not(target_os = "linux"))]
        return Err("Session keyboard layouts require the qualified Linux engine".into());
        #[cfg(target_os = "linux")]
        {
            if !valid_layout_id(&config.keyboard) {
                return Err("Enter one XKB keyboard layout ID (for example us, de, or fr), or clear it to inherit the host keyboard".into());
            }
            args.extend(["-termcraft-keyboard-layout".into(), config.keyboard.clone()]);
        }
    }
    Ok(args)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{SocketAddr, UdpSocket};

    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn windows_xdmcp_only_defaults_to_loopback_for_loopback_managers() {
        let mut config = XdmcpConfig {
            selected_endpoint: Some("127.0.0.1:1177".parse().unwrap()),
            allow_network_listener: true,
            ..Default::default()
        };
        assert_eq!(
            windows_network(&config, 800, 600).unwrap().1,
            Ipv4Addr::LOCALHOST
        );
        config.selected_endpoint = Some("192.0.2.2:177".parse().unwrap());
        for address in [None, Some(Ipv4Addr::LOCALHOST)] {
            config.advertise_address = address;
            assert!(
                windows_network(&config, 800, 600)
                    .unwrap_err()
                    .contains("explicit local non-loopback")
            );
        }
        let address = Ipv4Addr::new(192, 0, 2, 1);
        config.advertise_address = Some(address);
        assert_eq!(
            windows_network(&config, 800, 600).unwrap(),
            ("192.0.2.2:177".parse().unwrap(), address)
        );
        for bad in [
            Ipv4Addr::UNSPECIFIED,
            Ipv4Addr::BROADCAST,
            Ipv4Addr::new(224, 0, 0, 1),
        ] {
            config.advertise_address = Some(bad);
            assert!(
                windows_network(&config, 800, 600)
                    .unwrap_err()
                    .contains("local unicast IPv4")
            );
        }
    }

    #[cfg(all(windows, target_arch = "x86_64"))]
    #[test]
    fn windows_xdmcp_requires_consent_and_rejects_unsupported_options() {
        let mut config = XdmcpConfig {
            selected_endpoint: Some("127.0.0.1:1177".parse().unwrap()),
            ..Default::default()
        };
        assert!(
            windows_network(&config, 800, 600)
                .unwrap_err()
                .contains("listener")
        );
        config.allow_network_listener = true;
        for endpoint in ["[::1]:1177", "127.0.0.1:0", "255.255.255.255:1177"] {
            config.selected_endpoint = Some(endpoint.parse().unwrap());
            assert!(windows_network(&config, 800, 600).is_err());
        }
        config.selected_endpoint = Some("127.0.0.1:1177".parse().unwrap());
        assert!(windows_network(&config, 199, 600).is_err());
        assert!(windows_network(&config, 800, 8193).is_err());
        config.display = Some(59536);
        assert!(windows_network(&config, 800, 600).is_err());
        config.display = Some(59535);
        assert!(windows_network(&config, 800, 600).is_ok());
        config.mode = QueryMode::Broadcast;
        assert!(
            windows_network(&config, 800, 600)
                .unwrap_err()
                .contains("Direct or Indirect")
        );
        config.mode = QueryMode::Indirect;
        config.monitor = Some("DISPLAY1".into());
        assert!(
            windows_network(&config, 800, 600)
                .unwrap_err()
                .contains("Monitor")
        );
        config.monitor = Some("1".into());
        assert!(windows_network(&config, 800, 600).is_err());
        config.fullscreen = true;
        assert!(windows_network(&config, 800, 600).is_ok());
        config.monitor = None;
        config.keyboard = "de,us".into();
        assert!(
            windows_network(&config, 800, 600)
                .unwrap_err()
                .contains("keyboard")
        );
        config.keyboard = "de".into();
        assert!(windows_network(&config, 800, 600).is_ok());
        config.keyboard.clear();
        config.clipboard = ClipboardPolicy::Bidirectional;
        assert!(
            windows_network(&config, 800, 600)
                .unwrap_err()
                .contains("Clipboard")
        );
        config.clipboard = serde_json::from_str("\"windows_bidirectional\"").unwrap();
        assert!(windows_network(&config, 800, 600).is_ok());
        config.clipboard_display = Some(":0".into());
        assert!(windows_network(&config, 800, 600).is_err());
    }
    #[test]
    fn nondefault_port_is_used_and_only_correlated_willing_is_admitted() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        let addr = server.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let mut buf = [0; 512];
            let (n, peer) = server.recv_from(&mut buf).unwrap();
            assert_eq!(&buf[..n], &[0, 1, 0, 2, 0, 1, 0]);
            let forged = UdpSocket::bind("127.0.0.1:0").unwrap();
            forged
                .send_to(&[0, 1, 0, 5, 0, 6, 0, 0, 0, 0, 0, 0], peer)
                .unwrap();
            server.send_to(&[0, 1, 0, 5, 0, 1, 0], peer).unwrap();
            server
                .send_to(
                    &[
                        0, 1, 0, 5, 0, 14, 0, 0, 0, 4, b't', b'e', b's', b't', 0, 4, b'o', b'k',
                        b'a', b'y',
                    ],
                    peer,
                )
                .unwrap();
        });
        let result = discover(&DiscoveryRequest::direct(addr), &AtomicBool::new(false)).unwrap();
        worker.join().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].address, addr);
        assert_eq!(result[0].hostname, "test");
    }
    #[test]
    fn cancellation_during_discovery_finishes_promptly() {
        let server = UdpSocket::bind("127.0.0.1:0").unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let discovery =
            Discovery::start(DiscoveryRequest::direct(server.local_addr().unwrap())).unwrap();
        server.recv_from(&mut [0; 32]).unwrap();
        let start = Instant::now();
        discovery.cancel();
        loop {
            if let Some(result) = discovery.try_finish() {
                assert!(result.unwrap_err().contains("cancel"));
                break;
            }
            assert!(start.elapsed() < Duration::from_millis(500));
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn old_profiles_default_to_no_listener_and_round_trip_custom_options() {
        let config: XdmcpConfig = serde_json::from_str("{}").unwrap();
        assert!(config.is_default());
        assert!(!config.allow_network_listener);
        let config = XdmcpConfig {
            selected_endpoint: Some("192.0.2.2:1177".parse().unwrap()),
            display: Some(177),
            keyboard: "us".into(),
            ..Default::default()
        };
        assert_eq!(
            serde_json::from_str::<XdmcpConfig>(&serde_json::to_string(&config).unwrap()).unwrap(),
            config
        );
    }
    #[test]
    fn cancellation_prevents_any_packet() {
        let endpoint: SocketAddr = "127.0.0.1:1177".parse().unwrap();
        assert!(
            discover(&DiscoveryRequest::direct(endpoint), &AtomicBool::new(true))
                .unwrap_err()
                .contains("cancel")
        );
    }
    #[test]
    fn malformed_and_oversized_replies_are_rejected() {
        assert!(parse_willing(&[0, 1, 0, 5, 0, 1, 0]).is_none());
        assert!(parse_willing(&vec![0; 9000]).is_none());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn single_literal_layout_ids_and_catalogue_section() {
        for bad in [
            "",
            "US",
            "-de",
            "2de",
            "de/us",
            "../de",
            "de,us",
            "de(nodeadkeys)",
            "a+b",
            "a;b",
            "a b",
            &"a".repeat(33),
        ] {
            assert!(!valid_layout_id(bad), "{bad}");
        }
        let list = "! model\n  pc105 Generic\n! layout\n  de German\n  fr French\n! variant\n  neo de: Neo";
        assert!(registered_layout(list, "fr"));
        assert!(!registered_layout(list, "pc105"));
        assert!(!registered_layout(list, "neo"));
        assert!(!registered_layout(list, "d"));
    }
    #[test]
    fn listener_consent_and_selected_endpoint_are_required() {
        let mut config = XdmcpConfig::default();
        assert!(engine_arguments(&config, 120, 800, 600).is_err());
        config.selected_endpoint = Some("127.0.0.1:1177".parse().unwrap());
        assert!(
            engine_arguments(&config, 120, 800, 600)
                .unwrap_err()
                .contains("listener")
        );
        config.allow_network_listener = true;
        config.advertise_address = Some("192.0.2.1".parse().unwrap());
        config.fullscreen = true;
        config.monitor = Some("HEADLESS-1".into());
        config.keyboard = String::new();
        config.mode = QueryMode::Indirect;
        let args = engine_arguments(&config, 120, 800, 600).unwrap();
        for pair in [
            ["-port", "1177"],
            ["-indirect", "127.0.0.1"],
            ["-output", "HEADLESS-1"],
        ] {
            assert!(args.windows(2).any(|a| a == pair));
        }
        assert!(
            !args.contains(&"-fullscreen".into()),
            "-fullscreen causes upstream Xephyr to ignore -output"
        );
        assert!(
            !args.contains(&"-screen".into()),
            "-screen and -output create two separate Xephyr screens"
        );
        #[cfg(target_os = "linux")]
        for code in ["us", "de", "fr", "latam"] {
            config.keyboard = code.to_owned();
            assert!(
                engine_arguments(&config, 120, 800, 600)
                    .unwrap()
                    .windows(2)
                    .any(|args| args == ["-termcraft-keyboard-layout", code])
            );
        }
        config.keyboard = "unrecognized legacy layout".into();
        assert!(
            engine_arguments(&config, 120, 800, 600)
                .unwrap_err()
                .contains("keyboard")
        );
        config.keyboard.clear();
        assert!(!args.contains(&"-ac".into()));
        assert!(!args.iter().any(|arg| arg == "-once"));
        config.mode = QueryMode::Direct;
        assert!(
            engine_arguments(&config, 120, 800, 600)
                .unwrap()
                .iter()
                .any(|arg| arg == "-once")
        );
        config.clipboard = ClipboardPolicy::Bidirectional;
        assert!(
            engine_arguments(&config, 120, 800, 600)
                .unwrap_err()
                .contains("clipboard")
        );
        #[cfg(target_os = "linux")]
        {
            config.clipboard_display = Some(":0".into());
            assert!(engine_arguments(&config, 120, 800, 600).is_ok());
            config.clipboard_display = Some(":120.1".into());
            assert!(
                engine_arguments(&config, 120, 800, 600)
                    .unwrap_err()
                    .contains("differ")
            );
            config.clipboard_display = Some("remote:0".into());
            assert!(engine_arguments(&config, 120, 800, 600).is_err());
        }
        config.clipboard = ClipboardPolicy::WindowsBidirectional;
        assert!(
            engine_arguments(&config, 120, 800, 600)
                .unwrap_err()
                .contains("Windows clipboard")
        );
        config.clipboard = ClipboardPolicy::Disabled;
        assert!(engine_arguments(&config, 120, 800, 600).is_ok());
    }
    #[test]
    fn broadcast_requires_explicit_interface_choice() {
        let mut request = DiscoveryRequest::direct("127.0.0.1:1177".parse().unwrap());
        request.mode = QueryMode::Broadcast;
        assert!(request.validate().unwrap_err().contains("interface"));
    }
}
