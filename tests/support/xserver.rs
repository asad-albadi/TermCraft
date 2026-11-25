use harbor_workspace::x11::{XServerLease, XServerManager, XServerOptions};
use std::os::unix::{fs::PermissionsExt, net::UnixStream};
use std::path::PathBuf;
use x11rb::connection::Connection;
use x11rb::protocol::xproto::{ConnectionExt, CreateWindowAux, ImageFormat, MapState, WindowClass};
use x11rb::rust_connection::{DefaultStream, RustConnection};

pub fn packaged_manager() -> XServerManager {
    assert_eq!(
        std::env::var("TERMCRAFT_TEST_XSERVER_ISOLATED").as_deref(),
        Ok("1"),
        "run the live tests through tests/support/run_xserver_isolated.sh"
    );
    let root = std::env::var_os("TERMCRAFT_TEST_XSERVER_PACKAGE")
        .expect("stage the pinned engine and set TERMCRAFT_TEST_XSERVER_PACKAGE before running");
    XServerManager::for_installation(PathBuf::from(root))
}

pub fn start() -> XServerLease {
    packaged_manager()
        .start(XServerOptions::default())
        .expect("packaged Xwayland must open a private local display")
}

pub fn socket(display: &str) -> PathBuf {
    let number = display.strip_prefix(':').unwrap();
    PathBuf::from(format!("/tmp/.X11-unix/X{number}"))
}

pub fn client(
    lease: &XServerLease,
    valid_cookie: bool,
) -> Result<RustConnection, x11rb::errors::ConnectError> {
    let authority = lease.authority_path();
    assert_eq!(
        authority.metadata().unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert_eq!(
        authority
            .parent()
            .unwrap()
            .metadata()
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
    let record = std::fs::read(authority).unwrap();
    let mut cookie = record[record.len() - 16..].to_vec();
    if !valid_cookie {
        cookie[0] ^= 0xff;
    }
    let stream = UnixStream::connect(socket(lease.display())).unwrap();
    let (stream, _) = DefaultStream::from_unix_stream(stream).unwrap();
    RustConnection::connect_to_stream_with_auth_info(
        stream,
        0,
        b"MIT-MAGIC-COOKIE-1".to_vec(),
        cookie,
    )
}

pub fn window_is_visible_on_display(client: &RustConnection) -> bool {
    let screen = &client.setup().roots[0];
    let id = client.generate_id().unwrap();
    client
        .create_window(
            x11rb::COPY_DEPTH_FROM_PARENT,
            id,
            screen.root,
            24,
            24,
            150,
            80,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new().background_pixel(screen.white_pixel),
        )
        .unwrap()
        .check()
        .unwrap();
    client.map_window(id).unwrap().check().unwrap();
    let attributes = client.get_window_attributes(id).unwrap().reply().unwrap();
    let pixels = client
        .get_image(ImageFormat::Z_PIXMAP, id, 0, 0, 1, 1, u32::MAX)
        .unwrap()
        .reply()
        .unwrap();
    attributes.map_state == MapState::VIEWABLE && pixels.data.iter().any(|byte| *byte != 0)
}
