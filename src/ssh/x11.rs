//! Cookie-spoofed trusted X11 forwarding, scoped to this authenticated connection.
use crate::x11::XClientDisplay;
use std::{
    io::Read,
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use subtle::ConstantTimeEq;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use zeroize::Zeroizing;
const PROTOCOL: &[u8] = b"MIT-MAGIC-COOKIE-1";
enum Endpoint {
    #[cfg(unix)]
    Unix(std::path::PathBuf),
    Tcp(u16),
}
struct DisplayTarget {
    endpoint: Endpoint,
    number: u16,
    screen: u32,
}
impl DisplayTarget {
    fn parse_owned(display: &str) -> Result<Self, String> {
        #[cfg(unix)]
        if Path::new(display).is_absolute() {
            // A private XQuartz socket includes its fixed :0 suffix. Only the
            // manager-created display generation can reach this path branch.
            if !display.ends_with(":0")
                || display.contains(['\0', '\n', '\r'])
                || Path::new(display)
                    .components()
                    .any(|v| matches!(v, std::path::Component::ParentDir))
            {
                return Err("Invalid private X display socket".into());
            }
            return Ok(Self {
                endpoint: Endpoint::Unix(display.into()),
                number: 0,
                screen: 0,
            });
        }
        Self::parse(display)
    }
    fn parse(display: &str) -> Result<Self, String> {
        let (host, tail) = display
            .rsplit_once(':')
            .ok_or("DISPLAY must contain a display number")?;
        let (number, screen) = tail.split_once('.').unwrap_or((tail, "0"));
        let number: u16 = number.parse().map_err(|_| "Invalid X11 display number")?;
        let screen = screen.parse().map_err(|_| "Invalid X11 screen number")?;
        let endpoint = match host {
            "" | "unix" => {
                #[cfg(unix)] {Endpoint::Unix(format!("/tmp/.X11-unix/X{number}").into())}
                #[cfg(not(unix))] {return Err("Use localhost:N for an authenticated local X server on this platform".into());}
            }
            "localhost" | "127.0.0.1" => Endpoint::Tcp(6000u16.checked_add(number).ok_or("X11 TCP port is out of range")?),
            _ => return Err("Embedded X11 requires a local DISPLAY (:N or localhost:N). Use OpenSSH compatibility for other display transports.".into()),
        };
        Ok(Self {
            endpoint,
            number,
            screen,
        })
    }
}
pub(crate) fn authority_cookie(
    bytes: &[u8],
    display: u16,
    hostname: &[u8],
    unix: bool,
) -> Result<Zeroizing<[u8; 16]>, String> {
    fn word(bytes: &mut &[u8]) -> Option<u16> {
        let b = bytes.get(..2)?;
        let n = u16::from_be_bytes([b[0], b[1]]);
        *bytes = &bytes[2..];
        Some(n)
    }
    fn field<'a>(bytes: &mut &'a [u8]) -> Option<&'a [u8]> {
        let n = word(bytes)? as usize;
        let value = bytes.get(..n)?;
        *bytes = &bytes[n..];
        Some(value)
    }
    let mut remaining = bytes;
    while !remaining.is_empty() {
        let mut record = || -> Option<_> {
            Some((
                word(&mut remaining)?,
                field(&mut remaining)?,
                field(&mut remaining)?,
                field(&mut remaining)?,
                field(&mut remaining)?,
            ))
        };
        let (family, address, number, protocol, key) =
            record().ok_or("Malformed Xauthority file")?;
        let host_matches = family == 65535
            || (family == 256 && unix && address == hostname)
            || (!unix
                && ((family == 0 && address == [127, 0, 0, 1])
                    || (family == 6 && address == std::net::Ipv6Addr::LOCALHOST.octets())));
        if host_matches
            && number == display.to_string().as_bytes()
            && protocol == PROTOCOL
            && key.len() == 16
        {
            let mut cookie = Zeroizing::new([0; 16]);
            cookie.copy_from_slice(key);
            return Ok(cookie);
        }
    }
    Err("No matching MIT-MAGIC-COOKIE-1 entry in Xauthority. Configure an authenticated local X server or use OpenSSH compatibility.".into())
}
async fn read_setup<R: AsyncRead + Unpin>(
    remote: &mut R,
    fake: &[u8; 16],
    real: &[u8; 16],
) -> Result<Zeroizing<Vec<u8>>, String> {
    let mut header = [0; 12];
    remote
        .read_exact(&mut header)
        .await
        .map_err(|e| e.to_string())?;
    let word = |i| match header[0] {
        b'l' => Ok(u16::from_le_bytes([header[i], header[i + 1]])),
        b'B' => Ok(u16::from_be_bytes([header[i], header[i + 1]])),
        _ => Err("Invalid X11 byte order".to_string()),
    };
    if word(2)? != 11 || word(4)? != 0 || word(6)? != 18 || word(8)? != 16 {
        return Err("Unsupported X11 setup or authentication format".into());
    }
    let mut setup = Zeroizing::new(vec![0; 48]);
    setup[..12].copy_from_slice(&header);
    remote
        .read_exact(&mut setup[12..])
        .await
        .map_err(|e| e.to_string())?;
    if &setup[12..30] != PROTOCOL || !bool::from(setup[32..48].ct_eq(fake)) {
        return Err("X11 forwarding cookie verification failed".into());
    }
    setup[32..48].copy_from_slice(real);
    Ok(setup)
}
pub(crate) struct Forwarder {
    target: DisplayTarget,
    real: Zeroizing<[u8; 16]>,
    fake: Zeroizing<[u8; 16]>,
    slots: Arc<tokio::sync::Semaphore>,
    owned: Option<XClientDisplay>,
    requested: AtomicBool,
    expired: tokio::sync::watch::Sender<bool>,
}
impl Forwarder {
    pub fn from_environment() -> Result<Arc<Self>, String> {
        let display = std::env::var("DISPLAY")
            .map_err(|_| "X11 forwarding needs DISPLAY and an authenticated local X server")?;
        let path = std::env::var_os("XAUTHORITY")
            .map(std::path::PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".Xauthority"))
            })
            .ok_or("Set XAUTHORITY to your local X server authorization file")?;
        Self::from_authority(&display, &path, None)
    }

    pub(crate) fn from_owned(display: XClientDisplay) -> Result<Arc<Self>, String> {
        Self::from_authority(
            display.display(),
            display.authority_path(),
            Some(display.clone()),
        )
    }

    fn from_authority(
        display: &str,
        path: &Path,
        owned: Option<XClientDisplay>,
    ) -> Result<Arc<Self>, String> {
        let target = if owned.is_some() {
            DisplayTarget::parse_owned(display)?
        } else {
            DisplayTarget::parse(display)?
        };
        let mut bytes = Zeroizing::new(Vec::new());
        std::fs::File::open(path)
            .map_err(
                |_| "Cannot read Xauthority; configure XAUTHORITY or use OpenSSH compatibility",
            )?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > 1024 * 1024 {
            return Err("Xauthority file exceeds 1 MiB limit".into());
        }
        let mut hostname = [0u8; 256];
        #[cfg(unix)]
        if unsafe { libc::gethostname(hostname.as_mut_ptr().cast(), hostname.len()) } != 0 {
            return Err("Could not determine local Xauthority hostname".into());
        }
        let hostname = &hostname[..hostname
            .iter()
            .position(|b| *b == 0)
            .unwrap_or(hostname.len())];
        #[cfg(unix)]
        let unix = matches!(target.endpoint, Endpoint::Unix(_));
        #[cfg(not(unix))]
        let unix = false;
        let real = authority_cookie(&bytes, target.number, hostname, unix)?;
        let mut fake = Zeroizing::new([0; 16]);
        getrandom::fill(&mut *fake).map_err(|e| e.to_string())?;
        Ok(Arc::new(Self {
            target,
            real,
            fake,
            slots: Arc::new(tokio::sync::Semaphore::new(16)),
            owned,
            requested: AtomicBool::new(false),
            expired: tokio::sync::watch::Sender::new(false),
        }))
    }
    pub(crate) fn expire(&self) {
        self.expired.send_replace(true);
    }

    pub fn request_cookie(&self) -> Result<String, String> {
        if let Some(display) = &self.owned {
            display.ensure_running()?;
        }
        self.requested.store(true, Ordering::Release);
        Ok(self.fake.iter().map(|b| format!("{b:02x}")).collect())
    }
    pub fn screen(&self) -> u32 {
        self.target.screen
    }
    pub fn permit(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        if *self.expired.borrow()
            || !self.requested.load(Ordering::Acquire)
            || self
                .owned
                .as_ref()
                .is_some_and(|display| display.ensure_running().is_err())
        {
            return None;
        }
        self.slots.clone().try_acquire_owned().ok()
    }
    pub async fn bridge<R: AsyncRead + AsyncWrite + Unpin>(&self, remote: R) -> Result<(), String> {
        let mut expired = self.expired.subscribe();
        let display_stopped = async {
            match &self.owned {
                Some(display) => display.stopped().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            biased;
            _ = expired.wait_for(|closed| *closed) => Ok(()),
            _ = display_stopped => Ok(()),
            result = self.relay(remote) => result,
        }
    }

    async fn relay<R: AsyncRead + AsyncWrite + Unpin>(&self, mut remote: R) -> Result<(), String> {
        // No local connection is made until the remote setup cookie has been verified.
        let setup = tokio::time::timeout(
            Duration::from_secs(5),
            read_setup(&mut remote, &self.fake, &self.real),
        )
        .await
        .map_err(|_| "X11 setup timed out")??;
        async fn relay<R: AsyncRead + AsyncWrite + Unpin, L: AsyncRead + AsyncWrite + Unpin>(
            remote: &mut R,
            mut local: L,
            setup: &[u8],
        ) -> Result<(), String> {
            local.write_all(setup).await.map_err(|e| e.to_string())?;
            tokio::io::copy_bidirectional(remote, &mut local)
                .await
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        match &self.target.endpoint {
            #[cfg(unix)]
            Endpoint::Unix(path) => {
                let local = tokio::time::timeout(
                    Duration::from_secs(5),
                    tokio::net::UnixStream::connect(path),
                )
                .await
                .map_err(|_| "Local X server connection timed out")?
                .map_err(|e| format!("Local X server: {e}"))?;
                relay(&mut remote, local, &setup).await
            }
            Endpoint::Tcp(port) => {
                let local = tokio::time::timeout(
                    Duration::from_secs(5),
                    tokio::net::TcpStream::connect(("127.0.0.1", *port)),
                )
                .await
                .map_err(|_| "Local X server connection timed out")?
                .map_err(|e| format!("Local X server: {e}"))?;
                relay(&mut remote, local, &setup).await
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::AsyncWriteExt;
    fn setup(little: bool, cookie: &[u8]) -> Vec<u8> {
        let mut b = vec![0; 12];
        b[0] = if little { b'l' } else { b'B' };
        for (offset, n) in [(2, 11u16), (6, 18), (8, cookie.len() as u16)] {
            b[offset..offset + 2].copy_from_slice(&if little {
                n.to_le_bytes()
            } else {
                n.to_be_bytes()
            });
        }
        b.extend_from_slice(b"MIT-MAGIC-COOKIE-1");
        b.extend_from_slice(&[0, 0]);
        b.extend_from_slice(cookie);
        b
    }
    #[tokio::test]
    async fn substitutes_only_verified_cookie_in_both_byte_orders() {
        for little in [true, false] {
            let fake = [0x12; 16];
            let real = [0x34; 16];
            let bytes = setup(little, &fake);
            let (mut source, mut remote) = tokio::io::duplex(128);
            tokio::spawn(async move {
                for b in bytes {
                    source.write_all(&[b]).await.unwrap();
                }
            });
            let replaced = read_setup(&mut remote, &fake, &real).await.unwrap();
            assert_eq!(&*replaced, &setup(little, &real));
        }
    }
    #[tokio::test]
    async fn rejects_wrong_cookie_and_oversized_handshakes() {
        for bytes in [setup(true, &[9; 16]), setup(false, &[1; 300])] {
            let (mut source, mut remote) = tokio::io::duplex(512);
            source.write_all(&bytes).await.unwrap();
            assert!(read_setup(&mut remote, &[1; 16], &[2; 16]).await.is_err());
        }
    }
    #[test]
    fn authority_requires_matching_display_host_and_protocol() {
        fn record(
            family: u16,
            host: &[u8],
            display: &[u8],
            protocol: &[u8],
            key: &[u8],
        ) -> Vec<u8> {
            let mut bytes = family.to_be_bytes().to_vec();
            for field in [host, display, protocol, key] {
                bytes.extend_from_slice(&(field.len() as u16).to_be_bytes());
                bytes.extend_from_slice(field);
            }
            bytes
        }
        let bytes = record(256, b"test-host", b"7", b"MIT-MAGIC-COOKIE-1", &[42; 16]);
        assert_eq!(
            &*authority_cookie(&bytes, 7, b"test-host", true).unwrap(),
            &[42; 16]
        );
        assert!(authority_cookie(&bytes, 8, b"test-host", true).is_err());
        assert!(authority_cookie(&bytes, 7, b"other-host", true).is_err());
        assert!(authority_cookie(&bytes[..bytes.len() - 1], 7, b"test-host", true).is_err());
    }
    #[tokio::test]
    async fn unauthorized_x11_never_connects_to_local_server() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let forwarder = Forwarder {
            target: DisplayTarget {
                endpoint: Endpoint::Tcp(listener.local_addr().unwrap().port()),
                number: 0,
                screen: 0,
            },
            real: Zeroizing::new([2; 16]),
            fake: Zeroizing::new([1; 16]),
            slots: Arc::new(tokio::sync::Semaphore::new(16)),
            owned: None,
            requested: AtomicBool::new(false),
            expired: tokio::sync::watch::Sender::new(false),
        };
        let (mut source, remote) = tokio::io::duplex(128);
        source.write_all(&setup(true, &[9; 16])).await.unwrap();
        assert!(
            forwarder
                .bridge(remote)
                .await
                .unwrap_err()
                .contains("cookie verification failed")
        );
        assert_eq!(
            listener.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    #[cfg(unix)]
    #[test]
    fn owned_socket_display_does_not_enable_system_path_connections() {
        let name = "/private/owned-runtime/display:0";
        assert!(DisplayTarget::parse(name).is_err());
        let target = DisplayTarget::parse_owned(name).unwrap();
        assert_eq!(target.number, 0);
        assert_eq!(target.screen, 0);
        assert!(matches!(target.endpoint, Endpoint::Unix(path) if path == Path::new(name)));
        for invalid in [
            "/private/../other/display:0",
            "/private/display:1",
            "/private/display:0.5",
            "/private/display:0\0",
        ] {
            assert!(DisplayTarget::parse_owned(invalid).is_err());
        }
    }
    #[test]
    fn display_parser_rejects_remote_targets_and_port_overflow() {
        assert!(DisplayTarget::parse("evil.example:0").is_err());
        assert!(DisplayTarget::parse("localhost:65535").is_err());
        assert_eq!(DisplayTarget::parse("localhost:7.2").unwrap().screen, 2);
        #[cfg(unix)]
        assert!(matches!(
            DisplayTarget::parse(":7").unwrap().endpoint,
            Endpoint::Unix(_)
        ));
    }
}
