use harbor_workspace::desktop::carrier::SocketRelay;
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    time::{Duration, Instant},
};

const WAIT: Duration = Duration::from_secs(3);

fn pair() -> (TcpStream, TcpStream) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let target = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let client = listener.accept().unwrap().0;
    for stream in [&target, &client] {
        stream.set_read_timeout(Some(WAIT)).unwrap();
        stream.set_write_timeout(Some(WAIT)).unwrap();
    }
    (target, client)
}

fn until(condition: impl Fn() -> bool) {
    let deadline = Instant::now() + WAIT;
    while !condition() {
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn relay_accepts_one_loopback_connection_and_moves_exact_bytes() {
    let (mut target, stream) = pair();
    let relay = SocketRelay::for_rdp(stream, None, WAIT).unwrap();
    assert!(relay.address().ip().is_loopback());

    let mut local = TcpStream::connect_timeout(&relay.address(), WAIT).unwrap();
    local.set_read_timeout(Some(WAIT)).unwrap();
    local.write_all(&relay.token()).unwrap();
    local.write_all(b"client\0\xff").unwrap();
    let mut actual = [0; 8];
    target.read_exact(&mut actual).unwrap();
    assert_eq!(&actual, b"client\0\xff");
    target.write_all(b"server\0\xff").unwrap();
    local.read_exact(&mut actual).unwrap();
    assert_eq!(&actual, b"server\0\xff");
    assert!(TcpStream::connect_timeout(&relay.address(), WAIT).is_err());

    relay.cancel();
    until(|| relay.is_finished());
    assert!(relay.take_error().is_none());
    assert_eq!(local.read(&mut [0]).unwrap(), 0);
    assert_eq!(target.read(&mut [0]).unwrap(), 0);
}

#[test]
fn dropping_relay_releases_pending_listener_and_target_socket() {
    let (mut target, stream) = pair();
    let relay = SocketRelay::for_rdp(stream, None, WAIT).unwrap();
    let address = relay.address();
    drop(relay);

    assert_eq!(target.read(&mut [0]).unwrap(), 0);
    assert!(TcpStream::connect_timeout(&address, WAIT).is_err());
}

#[test]
fn idle_relay_reports_startup_timeout_and_closes_target() {
    let (mut target, stream) = pair();
    let relay = SocketRelay::for_rdp(stream, None, Duration::from_millis(75)).unwrap();

    until(|| relay.is_finished());
    let error = relay.take_error().unwrap();
    assert!(error.contains("timed out"), "{error}");
    assert_eq!(target.read(&mut [0]).unwrap(), 0);
    assert!(TcpStream::connect_timeout(&relay.address(), WAIT).is_err());
}

#[test]
fn rdp_relay_rejects_untrusted_loopback_peer_before_forwarding() {
    let (mut target, stream) = pair();
    let relay = SocketRelay::for_rdp(stream, None, WAIT).unwrap();
    let token = relay.token();
    let mut wrong = token;
    wrong[0] ^= 1;
    let mut intruder = TcpStream::connect_timeout(&relay.address(), WAIT).unwrap();
    intruder.set_read_timeout(Some(WAIT)).unwrap();
    intruder.write_all(&wrong).unwrap();
    assert_eq!(intruder.read(&mut [0]).unwrap(), 0);

    let mut stalled = TcpStream::connect_timeout(&relay.address(), WAIT).unwrap();
    stalled.set_read_timeout(Some(WAIT)).unwrap();
    stalled.write_all(&token[..8]).unwrap();

    let mut helper = TcpStream::connect_timeout(&relay.address(), WAIT).unwrap();
    helper.set_read_timeout(Some(WAIT)).unwrap();
    helper.write_all(&token).unwrap();
    helper.write_all(b"authorized").unwrap();
    let mut data = [0; 10];
    target.read_exact(&mut data).unwrap();
    assert_eq!(&data, b"authorized");
    target.write_all(b"remote").unwrap();
    let mut data = [0; 6];
    helper.read_exact(&mut data).unwrap();
    assert_eq!(&data, b"remote");
    assert_eq!(stalled.read(&mut [0]).unwrap(), 0);
    assert!(TcpStream::connect_timeout(&relay.address(), WAIT).is_err());
}
