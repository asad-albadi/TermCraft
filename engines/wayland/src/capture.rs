//! Portal-fd-only PipeWire transport. No default remote or host socket is opened.
use super::{Cancel, Frame, decode};
use pipewire::{self as pw, properties::properties, spa};
use std::{
    io,
    os::fd::OwnedFd,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
#[derive(Default)]
struct Slot {
    frame: Option<Frame>,
    error: bool,
    revision: u64,
}
pub struct Capture {
    slot: Arc<Mutex<Slot>>,
    stop: Arc<AtomicBool>,
    worker: Option<thread::JoinHandle<()>>,
    last: Instant,
    revision: u64,
}
impl Capture {
    pub fn open(fd: OwnedFd, node: u32, serial: Option<u64>, cancel: Cancel) -> io::Result<Self> {
        let slot = Arc::new(Mutex::new(Slot::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let owned = slot.clone();
        let stopped = stop.clone();
        let worker = thread::Builder::new()
            .name("vnc-portal-pipewire".into())
            .spawn(move || {
                if run(fd, node, serial, &owned, &stopped, &cancel).is_err() {
                    owned.lock().unwrap().error = true;
                }
            })?;
        Ok(Self {
            slot,
            stop,
            worker: Some(worker),
            last: Instant::now(),
            revision: 0,
        })
    }
    pub fn frame(&mut self) -> io::Result<Option<Frame>> {
        let slot = self.slot.lock().unwrap();
        if slot.error {
            return Err(io::Error::other(
                "Selected portal video stream ended or is unsupported",
            ));
        }
        if slot.revision != self.revision {
            self.revision = slot.revision;
            self.last = Instant::now();
        }
        if self.last.elapsed() > Duration::from_secs(5) {
            return Err(io::Error::other("Selected portal video stream timed out"));
        }
        Ok(slot.frame.clone())
    }
}
impl Drop for Capture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(w) = self.worker.take() {
            let _ = w.join();
        }
    }
}
fn run(
    fd: OwnedFd,
    node: u32,
    serial: Option<u64>,
    slot: &Arc<Mutex<Slot>>,
    stop: &AtomicBool,
    cancel: &Cancel,
) -> io::Result<()> {
    pw::init();
    let main = pw::main_loop::MainLoopRc::new(None).map_err(err)?;
    let context = pw::context::ContextRc::new(&main, None).map_err(err)?;
    // Ownership is transferred to PipeWire, including error/disconnect cleanup.
    let core = context.connect_fd_rc(fd, None).map_err(err)?;
    let mut props = properties! { *pw::keys::MEDIA_TYPE=>"Video", *pw::keys::MEDIA_CATEGORY=>"Capture", *pw::keys::MEDIA_ROLE=>"Screen", *pw::keys::NODE_NAME=>"harbor-vnc-portal-capture" };
    if let Some(serial) = serial {
        props.insert(*pw::keys::TARGET_OBJECT, serial.to_string());
    }
    let stream =
        pw::stream::StreamBox::new(&core, "Harbor selected portal display", props).map_err(err)?;
    let frames = slot.clone();
    let failures = slot.clone();
    let _core_listener = core
        .add_listener_local()
        .error(move |_, _, _, _| {
            failures.lock().unwrap().error = true;
        })
        .register();
    let failures = slot.clone();
    let _listener = stream
        .add_local_listener_with_user_data(spa::param::video::VideoInfoRaw::default())
        .state_changed(move |_, _, _, state| {
            if matches!(
                state,
                pw::stream::StreamState::Error(_) | pw::stream::StreamState::Unconnected
            ) {
                failures.lock().unwrap().error = true;
            }
        })
        .param_changed(|_, format, id, param| {
            if id == spa::param::ParamType::Format.as_raw() {
                *format = Default::default();
                if let Some(p) = param {
                    let _ = format.parse(p);
                }
            }
        })
        .process(move |stream, format| {
            // One bounded latest-frame slot. Returning Buffer always requeues it.
            // Drain older buffers without copying so slow network viewers cannot back up capture.
            let Some(mut buffer) = stream.dequeue_buffer() else {
                return;
            };
            for _ in 0..15 {
                if let Some(new) = stream.dequeue_buffer() {
                    buffer = new;
                } else {
                    break;
                }
            }
            use spa::buffer::meta::{
                MetaHeader, MetaHeaderFlags, MetaVideoCrop, MetaVideoTransform,
                MetaVideoTransformValue,
            };
            let transformed = buffer
                .find_meta::<MetaVideoTransform>()
                .is_some_and(|m| m.transform() != MetaVideoTransformValue::NONE);
            let cropped = buffer.find_meta::<MetaVideoCrop>().is_some_and(|m| {
                let r = m.meta_region();
                r.is_valid()
                    && (r.position().x != 0 || r.position().y != 0 || r.size() != format.size())
            });
            let corrupted = buffer.find_meta::<MetaHeader>().is_some_and(|m| {
                m.flags()
                    .intersects(MetaHeaderFlags::CORRUPTED | MetaHeaderFlags::GAP)
            });
            if transformed || cropped || corrupted {
                frames.lock().unwrap().error = true;
                return;
            }
            let planes = buffer.datas_mut();
            let result = (|| {
                if planes.len() != 1 {
                    return Err(io::Error::other("Unsupported video planes"));
                }
                let plane = &mut planes[0];
                if plane.as_raw().chunk.is_null() {
                    return Err(io::Error::other("Missing video chunk"));
                }
                let chunk = plane.chunk();
                if chunk.size() == 0 {
                    return Ok(None);
                }
                if !chunk.flags().is_empty() {
                    return Err(io::Error::other("Corrupted video chunk"));
                }
                let (offset, size, stride) = (chunk.offset(), chunk.size(), chunk.stride());
                // MAP_BUFFERS maps only memory-backed negotiated buffers; DMA-BUF formats
                // are not advertised. Never mmap an arbitrary descriptor ourselves.
                let bytes = plane
                    .data()
                    .ok_or_else(|| io::Error::other("Unmapped video buffer"))?;
                decode(
                    format.format(),
                    format.size().width,
                    format.size().height,
                    bytes,
                    offset,
                    size,
                    stride,
                )
                .map(Some)
            })();
            let mut state = frames.lock().unwrap();
            match result {
                Ok(Some(frame)) => {
                    state.frame = Some(frame);
                    state.revision = state.revision.wrapping_add(1);
                }
                Ok(None) => {}
                Err(_) => state.error = true,
            }
        })
        .register()
        .map_err(err)?;
    let obj = spa::pod::object!(
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
            Choice,
            Enum,
            Id,
            spa::param::video::VideoFormat::BGRx,
            spa::param::video::VideoFormat::BGRx,
            spa::param::video::VideoFormat::RGBx,
            spa::param::video::VideoFormat::BGRA,
            spa::param::video::VideoFormat::RGBA,
            spa::param::video::VideoFormat::RGB,
            spa::param::video::VideoFormat::BGR
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoSize,
            Choice,
            Range,
            Rectangle,
            spa::utils::Rectangle {
                width: 1280,
                height: 720
            },
            spa::utils::Rectangle {
                width: 1,
                height: 1
            },
            spa::utils::Rectangle {
                width: 4096,
                height: 2048
            }
        ),
        spa::pod::property!(
            spa::param::format::FormatProperties::VideoFramerate,
            Choice,
            Range,
            Fraction,
            spa::utils::Fraction { num: 30, denom: 1 },
            spa::utils::Fraction { num: 0, denom: 1 },
            spa::utils::Fraction { num: 60, denom: 1 }
        )
    );
    let bytes = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &spa::pod::Value::Object(obj),
    )
    .map_err(err)?
    .0
    .into_inner();
    use spa::pod::{ChoiceValue, Property, PropertyFlags, Value};
    use spa::utils::{Choice, ChoiceEnum, ChoiceFlags};
    let buffers = spa::pod::Object {
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
                key: spa::sys::SPA_PARAM_BUFFERS_dataType,
                flags: PropertyFlags::empty(),
                value: Value::Choice(ChoiceValue::Int(Choice(
                    ChoiceFlags::empty(),
                    ChoiceEnum::Flags {
                        default: (1 << spa::sys::SPA_DATA_MemPtr) | (1 << spa::sys::SPA_DATA_MemFd),
                        flags: vec![
                            (1 << spa::sys::SPA_DATA_MemPtr) | (1 << spa::sys::SPA_DATA_MemFd),
                        ],
                    },
                ))),
            },
        ],
    };
    let buffers = spa::pod::serialize::PodSerializer::serialize(
        std::io::Cursor::new(Vec::new()),
        &Value::Object(buffers),
    )
    .map_err(err)?
    .0
    .into_inner();
    stream
        .connect(
            spa::utils::Direction::Input,
            if serial.is_some() { None } else { Some(node) },
            pw::stream::StreamFlags::AUTOCONNECT
                | pw::stream::StreamFlags::DONT_RECONNECT
                | pw::stream::StreamFlags::MAP_BUFFERS,
            &mut [
                spa::pod::Pod::from_bytes(&bytes)
                    .ok_or_else(|| io::Error::other("Invalid video format"))?,
                spa::pod::Pod::from_bytes(&buffers)
                    .ok_or_else(|| io::Error::other("Invalid video buffers"))?,
            ],
        )
        .map_err(err)?;
    while !stop.load(Ordering::Acquire) && !cancel() && !slot.lock().unwrap().error {
        if main
            .loop_()
            .iterate(pw::loop_::Timeout::Finite(Duration::from_millis(20)))
            < 0
        {
            return Err(io::Error::other("PipeWire transport failed"));
        }
    }
    Ok(())
}
fn err(_: impl std::fmt::Display) -> io::Error {
    io::Error::other("Could not connect selected portal video stream")
}
