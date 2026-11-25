//! Test-only native RGBx source. It has no capture/input/clipboard APIs.
use pipewire::{self as pw, properties::properties, spa};
use std::{
    os::fd::OwnedFd,
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(
        std::env::var("TERMCRAFT_TEST_WAYLAND_ISOLATED").as_deref(),
        Ok("1")
    );
    assert!(std::env::var_os("DISPLAY").is_none());
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    let socket = std::path::PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap())
        .join("e03-private-pipewire");
    let fd = OwnedFd::from(UnixStream::connect(socket)?);
    pw::init();
    let main = pw::main_loop::MainLoopRc::new(None)?;
    let context = pw::context::ContextRc::new(&main, None)?;
    let core = context.connect_fd_rc(fd, None)?;
    let stream = pw::stream::StreamBox::new(
        &core,
        "Owned synthetic pixel fixture",
        properties! {*pw::keys::NODE_NAME=>"e03-synthetic",*pw::keys::MEDIA_TYPE=>"Video",*pw::keys::MEDIA_CATEGORY=>"Capture",*pw::keys::MEDIA_CLASS=>"Video/Source","node.driver"=>"true","node.always-process"=>"true"},
    )?;
    let _listener = stream
        .add_local_listener_with_user_data(0u8)
        .param_changed(|stream, _, id, param| {
            if id != spa::param::ParamType::Format.as_raw() || param.is_none() {
                return;
            }
            use spa::pod::{ChoiceValue, Object, Property, PropertyFlags, Value};
            use spa::utils::{Choice, ChoiceEnum, ChoiceFlags};
            let buffers = Object {
                type_: spa::utils::SpaTypes::ObjectParamBuffers.as_raw(),
                id: spa::param::ParamType::Buffers.as_raw(),
                properties: vec![
                    Property {
                        key: spa::sys::SPA_PARAM_BUFFERS_buffers,
                        flags: PropertyFlags::empty(),
                        value: Value::Choice(ChoiceValue::Int(Choice(
                            ChoiceFlags::empty(),
                            ChoiceEnum::Range {
                                default: 4,
                                min: 2,
                                max: 8,
                            },
                        ))),
                    },
                    Property {
                        key: spa::sys::SPA_PARAM_BUFFERS_size,
                        flags: PropertyFlags::empty(),
                        value: Value::Int(64 * 48 * 4),
                    },
                    Property {
                        key: spa::sys::SPA_PARAM_BUFFERS_stride,
                        flags: PropertyFlags::empty(),
                        value: Value::Int(64 * 4),
                    },
                ],
            };
            let bytes = spa::pod::serialize::PodSerializer::serialize(
                std::io::Cursor::new(Vec::new()),
                &Value::Object(buffers),
            )
            .unwrap()
            .0
            .into_inner();
            stream
                .update_params(&mut [spa::pod::Pod::from_bytes(&bytes).unwrap()])
                .unwrap();
        })
        .process(|stream, counter| {
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            let Some(plane) = buffer.datas_mut().first_mut() else {
                return;
            };
            let Some(bytes) = plane.data() else { return };
            if bytes.len() < 64 * 48 * 4 {
                return;
            }
            *counter = counter.wrapping_add(1);
            for (i, pixel) in bytes[..64 * 48 * 4].chunks_exact_mut(4).enumerate() {
                pixel.copy_from_slice(&[*counter, (i % 64) as u8, (i / 64) as u8, 255]);
            }
            let chunk = plane.chunk_mut();
            *chunk.offset_mut() = 0;
            *chunk.stride_mut() = 64 * 4;
            *chunk.size_mut() = 64 * 48 * 4;
        })
        .register()?;
    let format = spa::pod::object!(
        spa::utils::SpaTypes::ObjectParamFormat,
        spa::param::ParamType::EnumFormat,
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaType,
            Id,
            spa::param::format::MediaType::Video
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::MediaSubtype,
            Id,
            spa::param::format::MediaSubtype::Raw
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFormat,
            Id,
            spa::param::video::VideoFormat::RGBx
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Rectangle,
            spa::utils::Rectangle {
                width: 64,
                height: 48
            }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Fraction,
            spa::utils::Fraction { num: 30, denom: 1 }
        )
    );
    let values = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(format),
    )?
    .0
    .into_inner();
    stream.connect(
        spa::utils::Direction::Output,
        None,
        pw::stream::StreamFlags::MAP_BUFFERS | pw::stream::StreamFlags::DRIVER,
        &mut [spa::pod::Pod::from_bytes(&values).unwrap()],
    )?;
    let mut next = Instant::now();
    loop {
        main.loop_()
            .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(5)));
        if Instant::now() >= next {
            let _ = stream.trigger_process();
            next = Instant::now() + Duration::from_millis(33);
        }
    }
}
