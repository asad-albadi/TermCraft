use std::{
    collections::BTreeMap,
    io::ErrorKind,
    net::{IpAddr, SocketAddr, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const PORT_LIMIT: usize = 256;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(150);

pub struct ScanTarget {
    pub address: IpAddr,
    pub ports: Vec<u16>,
}

impl ScanTarget {
    pub fn validate(&self) -> Result<(), String> {
        let address = self.address.to_canonical();
        if address.is_unspecified()
            || address.is_multicast()
            || address == IpAddr::V4(std::net::Ipv4Addr::BROADCAST)
        {
            return Err("Enter a specific unicast IPv4 or IPv6 address".into());
        }
        if self.ports.is_empty() || self.ports.len() > PORT_LIMIT || self.ports.contains(&0) {
            return Err("Specify 1–256 TCP ports numbered 1–65535".into());
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortStatus {
    Open,
    Closed,
    TimedOut,
    Unreachable,
}

#[derive(Debug)]
pub struct ScanReport {
    pub address: IpAddr,
    pub ports: BTreeMap<u16, PortStatus>,
    pub cancelled: bool,
}

pub struct PortScan;

impl PortScan {
    pub fn start(mut target: ScanTarget) -> Result<ScanHandle, String> {
        target.validate()?;
        target.ports.sort_unstable();
        target.ports.dedup();

        let cancel = Arc::new(AtomicBool::new(false));
        let worker_cancel = Arc::clone(&cancel);
        let worker = thread::Builder::new()
            .name("tcp-port-check".into())
            .spawn(move || {
                let mut ports = BTreeMap::new();
                for port in target.ports {
                    if worker_cancel.load(Ordering::Relaxed) {
                        break;
                    }
                    let endpoint = SocketAddr::new(target.address, port);
                    let status = match TcpStream::connect_timeout(&endpoint, CONNECT_TIMEOUT) {
                        Ok(_) => PortStatus::Open,
                        Err(error) => match error.kind() {
                            ErrorKind::ConnectionRefused => PortStatus::Closed,
                            ErrorKind::TimedOut => PortStatus::TimedOut,
                            _ => PortStatus::Unreachable,
                        },
                    };
                    ports.insert(port, status);
                }
                ScanReport {
                    address: target.address,
                    ports,
                    cancelled: worker_cancel.load(Ordering::Relaxed),
                }
            })
            .map_err(|error| format!("Could not start port check: {error}"))?;
        Ok(ScanHandle {
            cancel,
            worker: Some(worker),
        })
    }
}

pub struct ScanHandle {
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<ScanReport>>,
}

impl ScanHandle {
    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }

    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::Relaxed);
    }

    pub fn wait(mut self) -> Result<ScanReport, String> {
        self.worker
            .take()
            .unwrap()
            .join()
            .map_err(|_| "Port check worker stopped unexpectedly".into())
    }
}

impl Drop for ScanHandle {
    fn drop(&mut self) {
        self.cancel();
    }
}
