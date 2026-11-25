use harbor_workspace::{
    desktop::{
        DesktopConfig,
        rdp_options::ClipboardPolicy,
        vnc::{Security, VncOptions},
    },
    model::Session,
};
use std::{
    io::{Read, Write},
    net::{Ipv4Addr, TcpListener, TcpStream},
    time::{Duration, Instant},
};

pub fn endpoint(view_only: bool, clipboard: ClipboardPolicy) -> (TcpListener, DesktopConfig) {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    listener.set_nonblocking(true).unwrap();
    let session = Session {
        protocol: "VNC".into(),
        host: Ipv4Addr::LOCALHOST.to_string(),
        port: listener.local_addr().unwrap().port(),
        view_only,
        vnc_options: VncOptions {
            embedded: true,
            allow_unencrypted: true,
            security: Security::None,
            clipboard,
            ..Default::default()
        },
        ..Default::default()
    };
    (
        listener,
        DesktopConfig::new(100, session, Default::default()),
    )
}

pub fn accept(listener: TcpListener) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut stream = loop {
        match listener.accept() {
            Ok((stream, _)) => break stream,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("VNC fixture accept: {error}"),
        }
    };
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream
        .set_write_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    stream.write_all(b"RFB 003.008\n").unwrap();
    assert_eq!(read(&mut stream, 12), b"RFB 003.008\n");
    stream.write_all(&[1, 1]).unwrap();
    assert_eq!(read(&mut stream, 1), [1]);
    stream.write_all(&[0; 4]).unwrap();
    assert_eq!(read(&mut stream, 1), [1]);
    let mut init = vec![0, 20, 0, 4];
    init.extend([0; 16]);
    init.extend(0u32.to_be_bytes());
    stream.write_all(&init).unwrap();
    assert_eq!(read(&mut stream, 20)[4..8], [32, 24, 0, 1]);
    stream
}

pub fn read(stream: &mut TcpStream, length: usize) -> Vec<u8> {
    let mut data = vec![0; length];
    stream.read_exact(&mut data).unwrap();
    data
}

pub fn encodings(stream: &mut TcpStream) -> Vec<i32> {
    let header = read(stream, 4);
    assert_eq!(header[0], 2);
    (0..u16::from_be_bytes(header[2..].try_into().unwrap()))
        .map(|_| i32::from_be_bytes(read(stream, 4).try_into().unwrap()))
        .collect()
}

pub fn update(
    stream: &mut TcpStream,
    (x, y, w, h): (u16, u16, u16, u16),
    encoding: i32,
    payload: &[u8],
) {
    let mut message = vec![0, 0, 0, 1];
    for number in [x, y, w, h] {
        message.extend(number.to_be_bytes());
    }
    message.extend(encoding.to_be_bytes());
    message.extend(payload);
    stream.write_all(&message).unwrap();
}

pub fn extended_clipboard(stream: &mut TcpStream, flags: u32, data: &[u8]) {
    let mut buffer = vec![3, 0, 0, 0];
    buffer.extend((-(4 + data.len() as i32)).to_be_bytes());
    buffer.extend(flags.to_be_bytes());
    buffer.extend(data);
    stream.write_all(&buffer).unwrap();
}

pub fn read_extended_clipboard(stream: &mut TcpStream) -> (u32, Vec<u8>) {
    let header = read(stream, 8);
    assert_eq!(header[0], 6);
    let size = i32::from_be_bytes(header[4..].try_into().unwrap());
    assert!((-32768..=-4).contains(&size));
    let body = read(stream, size.unsigned_abs() as usize);
    (
        u32::from_be_bytes(body[..4].try_into().unwrap()),
        body[4..].to_vec(),
    )
}
