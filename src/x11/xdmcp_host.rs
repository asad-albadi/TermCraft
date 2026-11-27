//! Explicit-action hostname resolution. DNS answers are not authentication.
use super::xdmcp::{DiscoveryRequest, QueryMode};
use std::{
    net::{IpAddr, SocketAddr, ToSocketAddrs},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
        mpsc::{self, Receiver},
    },
    time::{Duration, Instant},
};
const LIMIT: usize = 2;
const BUDGET: Duration = Duration::from_secs(2);
static ACTIVE: AtomicUsize = AtomicUsize::new(0);
#[cfg(test)]
pub(crate) static TEST_SYSTEM_RESOLVER: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostRequest {
    pub host: String,
    pub port: u16,
    pub mode: QueryMode,
    pub generation: u64,
}
impl HostRequest {
    pub fn validate(&self) -> Result<(), String> {
        if self.port == 0 || self.mode == QueryMode::Broadcast {
            return Err(
                "Hostname resolution needs Direct or Indirect mode and a nonzero port".into(),
            );
        }
        if self.host.parse::<IpAddr>().is_ok() {
            return Ok(());
        }
        let name = self.host.strip_suffix('.').unwrap_or(&self.host);
        if self.host.len() > 253
            || name.is_empty()
            || !name.split('.').all(|label| {
                !label.is_empty()
                    && label.len() <= 63
                    && label.as_bytes()[0].is_ascii_alphanumeric()
                    && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                    && label
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'-')
            })
        {
            return Err("Enter a hostname (ASCII labels, at most 253 bytes) or a numeric IP address; commands, URLs and control characters are not allowed".into());
        }
        Ok(())
    }
}
pub struct Resolution {
    request: HostRequest,
    deadline: Instant,
    cancelled: Arc<AtomicBool>,
    result: Receiver<Result<Vec<SocketAddr>, String>>,
}
impl Resolution {
    pub fn start(request: HostRequest) -> Result<Self, String> {
        request.validate()?;
        let host = request.host.clone();
        let port = request.port;
        if let Ok(ip) = host.parse::<IpAddr>() {
            let (sender, result) = mpsc::sync_channel(1);
            let _ = sender.send(ipv4_candidates(vec![SocketAddr::new(ip, port)], port));
            return Ok(Self {
                request,
                deadline: Instant::now() + BUDGET,
                cancelled: Arc::new(AtomicBool::new(false)),
                result,
            });
        }
        Self::spawn(request, BUDGET, &ACTIVE, move || {
            (host.as_str(), port)
                .to_socket_addrs()
                .map(|values| values.take(33).collect())
                .map_err(|e| format!("Could not resolve XDMCP hostname: {e}"))
        })
    }
    fn spawn(
        request: HostRequest,
        budget: Duration,
        active: &'static AtomicUsize,
        resolver: impl FnOnce() -> Result<Vec<SocketAddr>, String> + Send + 'static,
    ) -> Result<Self, String> {
        request.validate()?;
        active
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < LIMIT).then_some(n + 1)
            })
            .map_err(|_| {
                "XDMCP resolver is busy with two earlier requests; retry after they finish"
                    .to_owned()
            })?;
        let permit = Permit(active);
        let deadline = Instant::now() + budget;
        let cancelled = Arc::new(AtomicBool::new(false));
        let stop = cancelled.clone();
        let port = request.port;
        let (sender, result) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("termcraft-xdmcp-resolve".into())
            .spawn(move || {
                let _permit = permit;
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                let value = resolver().and_then(|ips| ipv4_candidates(ips, port));
                if !stop.load(Ordering::Relaxed) && Instant::now() < deadline {
                    let _ = sender.try_send(value);
                }
            })
            .map_err(|e| format!("Could not start XDMCP resolver: {e}"))?;
        Ok(Self {
            request,
            deadline,
            cancelled,
            result,
        })
    }
    pub fn request(&self) -> &HostRequest {
        &self.request
    }
    pub fn try_finish(&self, current: &HostRequest) -> Option<Result<Vec<SocketAddr>, String>> {
        if current != &self.request {
            return Some(Err(
                "XDMCP host, port, mode or generation changed; resolve again".into(),
            ));
        }
        if self.cancelled.load(Ordering::Relaxed) {
            return Some(Err("XDMCP resolution cancelled".into()));
        }
        if Instant::now() >= self.deadline {
            self.cancel();
            return Some(Err(
                "XDMCP hostname resolution timed out after its bounded phase (2 seconds maximum)"
                    .into(),
            ));
        }
        match self.result.try_recv() {
            Ok(value) => Some(value),
            Err(mpsc::TryRecvError::Empty) => None,
            Err(_) => Some(Err("XDMCP hostname resolver ended without a result".into())),
        }
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }
}
impl Drop for Resolution {
    fn drop(&mut self) {
        self.cancel();
    }
}
struct Permit(&'static AtomicUsize);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
fn ipv4_candidates(addresses: Vec<SocketAddr>, port: u16) -> Result<Vec<SocketAddr>, String> {
    if addresses.len() > 32 {
        return Err(
            "XDMCP hostname returned more than 32 addresses; use an explicit endpoint".into(),
        );
    }
    let mut values = Vec::new();
    for address in addresses.into_iter().filter(SocketAddr::is_ipv4) {
        if address.port() != port {
            return Err("Resolved XDMCP endpoint port does not match the request".into());
        }
        DiscoveryRequest::direct(address).validate()?;
        values.push(address);
    }
    values.sort();
    values.dedup();
    if values.is_empty() {
        return Err("This XDMCP engine requires IPv4; the hostname has no supported IPv4 endpoint (IPv6 is unavailable)".into());
    }
    Ok(values)
}
/// Resolve inside a connection worker. Cancellation never joins the system resolver.
/// The resolution phase lasts at most 2s, followed by the separately bounded engine phase.
pub(crate) fn resolve_for_connection(
    request: HostRequest,
    cancelled: &AtomicBool,
) -> Result<Vec<SocketAddr>, String> {
    super::xdmcp::check_cancelled(cancelled)?;
    let pending = Resolution::start(request.clone())?;
    loop {
        super::xdmcp::check_cancelled(cancelled)?;
        if let Some(value) = pending.try_finish(&request) {
            return value;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn request(host: &str, port: u16) -> HostRequest {
        HostRequest {
            host: host.into(),
            port,
            mode: QueryMode::Direct,
            generation: 1,
        }
    }
    #[test]
    fn rejects_shell_flags_controls_invalid_labels_and_bounds() {
        for host in [
            "",
            "-query",
            "a;b",
            "a\0b",
            "a\nb",
            "a b",
            "http://host",
            ".local",
            "a..b",
            "a-.b",
        ] {
            assert!(request(host, 177).validate().is_err(), "{host:?}");
        }
        assert!(request(&"a".repeat(254), 177).validate().is_err());
        assert!(request("localhost", 0).validate().is_err());
    }
    #[test]
    fn localhost_resolves_explicit_ipv4_at_exact_nondefault_port() {
        let _system_resolver = TEST_SYSTEM_RESOLVER.lock().unwrap();
        let req = request("localhost", 1177);
        let job = Resolution::start(req.clone()).unwrap();
        let start = Instant::now();
        loop {
            if let Some(result) = job.try_finish(&req) {
                let ips = result.unwrap();
                assert!(!ips.is_empty());
                assert!(
                    ips.iter()
                        .all(|ip| ip.is_ipv4() && ip.ip().is_loopback() && ip.port() == 1177)
                );
                break;
            }
            assert!(start.elapsed() < Duration::from_secs(3));
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    #[test]
    fn cancellation_and_exact_request_association_reject_results() {
        let req = request("127.0.0.1", 177);
        let job = Resolution::start(req.clone()).unwrap();
        for changed in [
            HostRequest {
                host: "localhost".into(),
                ..req.clone()
            },
            HostRequest {
                port: 178,
                ..req.clone()
            },
            HostRequest {
                mode: QueryMode::Indirect,
                ..req.clone()
            },
            HostRequest {
                generation: 2,
                ..req.clone()
            },
        ] {
            assert!(
                job.try_finish(&changed)
                    .unwrap()
                    .unwrap_err()
                    .contains("changed")
            );
        }
        job.cancel();
        assert!(
            job.try_finish(&req)
                .unwrap()
                .unwrap_err()
                .contains("cancel")
        );
    }
    #[test]
    fn blocked_resolver_cancel_timeout_and_capacity_are_bounded_without_join() {
        let active = Box::leak(Box::new(AtomicUsize::new(0)));
        let (release1, wait1) = mpsc::channel();
        let (release2, wait2) = mpsc::channel();
        let (started1, ready1) = mpsc::channel();
        let (started2, ready2) = mpsc::channel();
        let req = request("localhost", 1177);
        let first = Resolution::spawn(req.clone(), Duration::from_millis(40), active, move || {
            started1.send(()).unwrap();
            wait1.recv().unwrap();
            Ok(vec!["127.0.0.1:1177".parse().unwrap()])
        })
        .unwrap();
        let second = Resolution::spawn(req.clone(), Duration::from_millis(40), active, move || {
            started2.send(()).unwrap();
            wait2.recv().unwrap();
            Err("controlled resolver failure".into())
        })
        .unwrap();
        ready1.recv_timeout(Duration::from_secs(1)).unwrap();
        ready2.recv_timeout(Duration::from_secs(1)).unwrap();
        assert!(
            Resolution::spawn(req.clone(), BUDGET, active, || Ok(vec![]))
                .err()
                .unwrap()
                .contains("busy")
        );
        let now = Instant::now();
        first.cancel();
        assert!(
            first
                .try_finish(&req)
                .unwrap()
                .unwrap_err()
                .contains("cancel")
        );
        drop(first);
        assert!(now.elapsed() < Duration::from_millis(50));
        std::thread::sleep(Duration::from_millis(60));
        assert!(
            second
                .try_finish(&req)
                .unwrap()
                .unwrap_err()
                .contains("timed out")
        );
        drop(second);
        assert_eq!(active.load(Ordering::Acquire), 2);
        release1.send(()).unwrap();
        release2.send(()).unwrap();
        let end = Instant::now() + Duration::from_secs(1);
        while active.load(Ordering::Acquire) != 0 {
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn rejects_invalid_candidates_and_mismatched_ports() {
        assert!(ipv4_candidates(vec!["0.0.0.0:1177".parse().unwrap()], 1177).is_err());
        assert!(ipv4_candidates(vec!["127.0.0.1:1178".parse().unwrap()], 1177).is_err());
        assert!(ipv4_candidates(vec!["255.255.255.255:1177".parse().unwrap()], 1177).is_err());
    }
    #[test]
    fn owned_localhost_resolves_then_queries_nondefault_udp_port() {
        let _system_resolver = TEST_SYSTEM_RESOLVER.lock().unwrap();
        let server = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        server
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let addr = server.local_addr().unwrap();
        let worker = std::thread::spawn(move || {
            let mut packet = [0; 64];
            let (n, peer) = server.recv_from(&mut packet).unwrap();
            assert_eq!(&packet[..n], &[0, 1, 0, 2, 0, 1, 0]);
            server
                .send_to(&[0, 1, 0, 5, 0, 6, 0, 0, 0, 0, 0, 0], peer)
                .unwrap();
        });
        let ips =
            resolve_for_connection(request("localhost", addr.port()), &AtomicBool::new(false))
                .unwrap();
        assert_eq!(ips, vec![addr]);
        let endpoints = super::super::xdmcp::discover(
            &DiscoveryRequest::direct(ips[0]),
            &AtomicBool::new(false),
        )
        .unwrap();
        worker.join().unwrap();
        assert_eq!(endpoints.len(), 1);
        assert_eq!(endpoints[0].address, addr);
    }
    #[test]
    fn controlled_resolver_failure_returns_without_candidates() {
        let active = Box::leak(Box::new(AtomicUsize::new(0)));
        let req = request("localhost", 177);
        let job = Resolution::spawn(req.clone(), BUDGET, active, || {
            Err("owned resolver fixture failure".into())
        })
        .unwrap();
        let end = Instant::now() + Duration::from_secs(1);
        loop {
            if let Some(result) = job.try_finish(&req) {
                assert!(result.unwrap_err().contains("fixture failure"));
                break;
            }
            assert!(Instant::now() < end);
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    #[test]
    fn controlled_hostname_with_only_aaaa_answers_reports_ipv4_capability() {
        let active = Box::leak(Box::new(AtomicUsize::new(0)));
        let req = request("localhost", 1177);
        let job = Resolution::spawn(req.clone(), BUDGET, active, || {
            Ok(vec!["[::1]:1177".parse().unwrap()])
        })
        .unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        loop {
            if let Some(result) = job.try_finish(&req) {
                assert!(result.unwrap_err().contains("IPv6 is unavailable"));
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            ipv4_candidates(
                vec![
                    "[::1]:1177".parse().unwrap(),
                    "127.0.0.1:1177".parse().unwrap()
                ],
                1177
            )
            .unwrap(),
            vec!["127.0.0.1:1177".parse::<SocketAddr>().unwrap()]
        );
    }
    #[test]
    fn ipv6_only_is_an_explicit_capability_error() {
        let req = request("::1", 1177);
        let job = Resolution::start(req.clone()).unwrap();
        loop {
            if let Some(result) = job.try_finish(&req) {
                assert!(result.unwrap_err().contains("IPv4"));
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
