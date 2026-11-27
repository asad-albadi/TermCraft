use super::{capabilities::XCapabilities, native_transport};
use std::{
    error::Error,
    io,
    net::{Ipv4Addr, SocketAddr},
    path::PathBuf,
    sync::atomic::AtomicBool,
};
use x11rb::{
    connection::Connection,
    protocol::{
        composite::{ConnectionExt as _, Redirect},
        glx::ConnectionExt as _,
        randr::ConnectionExt as _,
        xproto::{
            ConnectionExt as _, CreateGCAux, CreateWindowAux, ImageFormat, ImageOrder, Rectangle,
            WindowClass,
        },
    },
};

type NativeError = Box<dyn Error + Send + Sync>;
type NativeResult<T> = Result<T, NativeError>;

pub(super) fn probe() -> Result<XCapabilities, String> {
    let address = trusted_target()?;
    if native_transport::setup(address, None, &AtomicBool::new(false))? {
        return Err("Owned Windows X server accepted cookie-free setup".into());
    }
    let (connection, screen_index) =
        x11rb::connect(None).map_err(|error| format!("Could not connect to X display: {error}"))?;
    let screen = &connection.setup().roots[screen_index];
    let geometry = connection
        .get_geometry(screen.root)
        .map_err(|error| error.to_string())?
        .reply()
        .map_err(|error| format!("Could not read root geometry: {error}"))?;
    let mut report = XCapabilities {
        dimensions: [geometry.width.into(), geometry.height.into()],
        ..Default::default()
    };

    match connection
        .glx_query_version(1, 4)
        .map_err(|error| error.to_string())
        .and_then(|cookie| cookie.reply().map_err(|error| error.to_string()))
    {
        Ok(version) => {
            report.glx_version = Some(format!(
                "{}.{}",
                version.major_version, version.minor_version
            ));
            report.errors.push(
                "GLX protocol verified; no OpenGL context was created because indirect GLX remains disabled"
                    .into(),
            );
        }
        Err(error) => report.errors.push(format!("GLX unavailable: {error}")),
    }

    match connection
        .randr_query_version(1, 6)
        .map_err(|error| error.to_string())
        .and_then(|cookie| cookie.reply().map_err(|error| error.to_string()))
    {
        Ok(version) => {
            report.randr_version = Some(format!(
                "{}.{}",
                version.major_version, version.minor_version
            ));
            report
                .errors
                .push("RANDR protocol verified; no mode change was requested".into());
        }
        Err(error) => report.errors.push(format!("RANDR unavailable: {error}")),
    }

    match probe_composite_pixels(&connection, screen_index) {
        Ok(version) => {
            report.composite_version = Some(version);
            report.composite_pixels = true;
        }
        Err(error) => report
            .errors
            .push(format!("Composite unavailable: {error}")),
    }
    let final_geometry = connection
        .get_geometry(screen.root)
        .map_err(|error| error.to_string())?
        .reply()
        .map_err(|error| format!("Could not recheck root geometry: {error}"))?;
    report.initial_mode_restored =
        (geometry.width, geometry.height) == (final_geometry.width, final_geometry.height);
    Ok(report)
}

fn trusted_target() -> Result<SocketAddr, String> {
    let display = std::env::var("DISPLAY")
        .map_err(|_| "Windows X probe requires an owned 127.0.0.1 loopback DISPLAY".to_string())?;
    let display = display
        .strip_prefix("127.0.0.1:")
        .ok_or("Windows X probe requires an owned 127.0.0.1 loopback DISPLAY")?;
    let (display_number, screen) = display.split_once('.').unwrap_or((display, "0"));
    if display_number.is_empty()
        || screen.is_empty()
        || screen.contains('.')
        || screen.parse::<u16>().is_err()
    {
        return Err("Windows X probe requires an owned 127.0.0.1 loopback DISPLAY".into());
    }
    let display_number = display_number
        .parse::<u16>()
        .map_err(|_| "Windows X probe requires an owned 127.0.0.1 loopback DISPLAY")?;
    let port = display_number
        .checked_add(6000)
        .ok_or("Windows X probe DISPLAY exceeds the TCP display range")?;
    let authority = std::env::var_os("XAUTHORITY")
        .map(PathBuf::from)
        .ok_or("Windows X probe requires an explicit XAUTHORITY file")?;
    if !authority.is_absolute() {
        return Err("Windows X probe requires an explicit absolute XAUTHORITY file".into());
    }
    let metadata = std::fs::symlink_metadata(&authority)
        .map_err(|error| format!("Could not inspect explicit XAUTHORITY file: {error}"))?;
    if !metadata.file_type().is_file() || !(1..=65_536).contains(&metadata.len()) {
        return Err("Windows X probe requires a bounded regular XAUTHORITY file".into());
    }
    Ok(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))
}

fn probe_composite_pixels(
    connection: &x11rb::rust_connection::RustConnection,
    screen_index: usize,
) -> NativeResult<String> {
    let version = connection.composite_query_version(0, 4)?.reply()?;
    let screen = &connection.setup().roots[screen_index];
    let source = connection.generate_id()?;
    let source_gc = connection.generate_id()?;
    let named_pixmap = connection.generate_id()?;
    let output = connection.generate_id()?;
    let output_gc = connection.generate_id()?;

    connection
        .create_window(
            screen.root_depth,
            source,
            screen.root,
            0,
            0,
            32,
            32,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new().background_pixel(screen.black_pixel),
        )?
        .check()?;
    connection
        .composite_redirect_window(source, Redirect::MANUAL)?
        .check()?;
    connection.map_window(source)?.check()?;
    connection
        .create_gc(
            source_gc,
            source,
            &CreateGCAux::new().foreground(screen.white_pixel),
        )?
        .check()?;
    connection
        .poly_fill_rectangle(
            source,
            source_gc,
            &[Rectangle {
                x: 0,
                y: 0,
                width: 32,
                height: 32,
            }],
        )?
        .check()?;
    connection
        .composite_name_window_pixmap(source, named_pixmap)?
        .check()?;

    connection
        .create_window(
            screen.root_depth,
            output,
            screen.root,
            40,
            0,
            64,
            32,
            0,
            WindowClass::INPUT_OUTPUT,
            screen.root_visual,
            &CreateWindowAux::new().background_pixel(screen.black_pixel),
        )?
        .check()?;
    connection.map_window(output)?.check()?;
    connection
        .create_gc(
            output_gc,
            output,
            &CreateGCAux::new().foreground(screen.black_pixel),
        )?
        .check()?;
    connection
        .poly_fill_rectangle(
            output,
            output_gc,
            &[Rectangle {
                x: 0,
                y: 0,
                width: 64,
                height: 32,
            }],
        )?
        .check()?;
    connection
        .copy_area(named_pixmap, output, output_gc, 0, 0, 0, 0, 32, 32)?
        .check()?;
    connection.flush()?;

    let image = connection
        .get_image(ImageFormat::Z_PIXMAP, output, 0, 0, 64, 32, u32::MAX)?
        .reply()?;
    let format = connection
        .setup()
        .pixmap_formats
        .iter()
        .find(|format| format.depth == image.depth)
        .ok_or_else(|| io::Error::other(format!("no pixmap format for depth {}", image.depth)))?;
    let bits_per_pixel = usize::from(format.bits_per_pixel);
    let pad_bits = usize::from(format.scanline_pad);
    if bits_per_pixel == 0 || bits_per_pixel % 8 != 0 || pad_bits == 0 || pad_bits % 8 != 0 {
        return Err(io::Error::other("invalid server pixmap format").into());
    }
    let stride = (64usize * bits_per_pixel).div_ceil(pad_bits) * pad_bits / 8;
    if image.data.len() != stride * 32 {
        return Err(io::Error::other(format!(
            "unexpected GetImage payload: expected {}, got {}",
            stride * 32,
            image.data.len()
        ))
        .into());
    }
    let bytes_per_pixel = bits_per_pixel / 8;
    let lsb_first = connection.setup().image_byte_order == ImageOrder::LSB_FIRST;
    for (x, expected) in [
        (8usize, screen.white_pixel),
        (24, screen.white_pixel),
        (40, screen.black_pixel),
        (56, screen.black_pixel),
    ] {
        let offset = 16 * stride + x * bytes_per_pixel;
        let bytes = &image.data[offset..offset + bytes_per_pixel];
        let pixel = if lsb_first {
            bytes.iter().enumerate().fold(0u32, |value, (shift, byte)| {
                value | (u32::from(*byte) << (shift * 8))
            })
        } else {
            bytes
                .iter()
                .fold(0u32, |value, byte| (value << 8) | u32::from(*byte))
        };
        if pixel != expected {
            return Err(io::Error::other(format!(
                "software Composite pixel mismatch at x={x}: expected {expected:#x}, got {pixel:#x}"
            ))
            .into());
        }
    }

    let _ = connection.free_gc(source_gc);
    let _ = connection.free_gc(output_gc);
    let _ = connection.free_pixmap(named_pixmap);
    let _ = connection.composite_unredirect_window(source, Redirect::MANUAL);
    let _ = connection.destroy_window(source);
    let _ = connection.destroy_window(output);
    let _ = connection.flush();
    Ok(format!(
        "{}.{}",
        version.major_version, version.minor_version
    ))
}
