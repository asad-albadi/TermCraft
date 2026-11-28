//! Ordered private-pipe output. A slow GUI must not stall RDP input or decoding.
use crate::wire::{self, Event};
use ironrdp_pdu::geometry::InclusiveRectangle;
use ironrdp_session::image::DecodedImage;
use std::{
    io::Write,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender, TrySendError},
    },
    time::Duration,
};
use zeroize::Zeroizing;

#[derive(Default)]
pub enum Damage {
    #[default]
    Clean,
    Full,
    Region(InclusiveRectangle),
}

impl Damage {
    pub fn update(&mut self, rect: InclusiveRectangle, width: u16, height: u16) {
        // IronRDP may report clipped drawing operations. Keep only the part
        // present in the decoded framebuffer.
        if rect.left >= width
            || rect.top >= height
            || rect.left > rect.right
            || rect.top > rect.bottom
        {
            return;
        }
        let rect = InclusiveRectangle {
            right: rect.right.min(width - 1),
            bottom: rect.bottom.min(height - 1),
            ..rect
        };
        match self {
            Self::Clean => *self = Self::Region(rect),
            Self::Full => {}
            Self::Region(prior) => {
                prior.left = prior.left.min(rect.left);
                prior.top = prior.top.min(rect.top);
                prior.right = prior.right.max(rect.right);
                prior.bottom = prior.bottom.max(rect.bottom);
            }
        }
    }

    fn packet(&self, image: &DecodedImage, revision: u64) -> Result<Packet, String> {
        let (width, height) = (image.width(), image.height());
        wire::dimensions(width, height)?;
        let entire = InclusiveRectangle {
            left: 0,
            top: 0,
            right: width - 1,
            bottom: height - 1,
        };
        let rect = match self {
            Self::Clean => return Err("No desktop pixels to publish".into()),
            Self::Full => &entire,
            Self::Region(rect) => rect,
        };
        let full = matches!(self, Self::Full);
        let w = rect.right - rect.left + 1;
        let h = rect.bottom - rect.top + 1;
        let row = usize::from(w) * 4;
        let bytes = wire::patch_bounds(width, height, rect.left, rect.top, w, h)?;
        let mut data = Zeroizing::new(Vec::with_capacity((if full { 12 } else { 20 }) + bytes));
        data.extend_from_slice(&width.to_be_bytes());
        data.extend_from_slice(&height.to_be_bytes());
        data.extend_from_slice(&revision.to_be_bytes());
        if !full {
            for field in [rect.left, rect.top, w, h] {
                data.extend_from_slice(&field.to_be_bytes());
            }
        }
        if full {
            data.extend_from_slice(image.data());
        } else {
            for y in rect.top..=rect.bottom {
                let offset = usize::from(y) * image.stride() + usize::from(rect.left) * 4;
                data.extend_from_slice(&image.data()[offset..offset + row]);
            }
        }
        Ok(Packet {
            kind: if full { 2 } else { 3 },
            data,
            finished: None,
        })
    }
}

struct Packet {
    kind: u8,
    data: Zeroizing<Vec<u8>>,
    finished: Option<SyncSender<()>>,
}

pub struct Output {
    sender: SyncSender<Packet>,
    frame_busy: Arc<AtomicBool>,
}

impl Output {
    pub fn new(mut writer: impl Write + Send + 'static) -> std::io::Result<Self> {
        let (sender, receiver) = mpsc::sync_channel::<Packet>(32);
        let frame_busy = Arc::new(AtomicBool::new(false));
        let busy = frame_busy.clone();
        std::thread::Builder::new()
            .name("rdp-parent-output".into())
            .spawn(move || {
                while let Ok(mut packet) = receiver.recv() {
                    let header = match packet.kind {
                        2 => Some(12),
                        3 => Some(20),
                        _ => None,
                    };
                    if let Some(header) = header {
                        for pixel in packet.data[header..].chunks_exact_mut(4) {
                            pixel[3] = 255;
                        }
                    }
                    if wire::write_packet(&mut writer, packet.kind, &packet.data).is_err() {
                        busy.store(false, Ordering::Release);
                        break;
                    }
                    let frame = packet.kind != 1;
                    if let Some(ack) = packet.finished {
                        let _ = ack.send(());
                    }
                    drop(packet.data);
                    if frame {
                        busy.store(false, Ordering::Release);
                    }
                }
            })?;
        Ok(Self { sender, frame_busy })
    }

    fn control(&self, event: Event, finished: Option<SyncSender<()>>) -> Result<(), String> {
        let data = Zeroizing::new(
            serde_json::to_vec(&event).map_err(|_| "Desktop control encoding failed")?,
        );
        if data.len() > wire::MAX_CONTROL {
            return Err("Desktop control output exceeds its bound".into());
        }
        self.sender
            .try_send(Packet {
                kind: 1,
                data,
                finished,
            })
            .map_err(|error| match error {
                TrySendError::Full(_) => {
                    "Desktop parent did not drain its bounded control queue".into()
                }
                TrySendError::Disconnected(_) => "Desktop parent output pipe closed".into(),
            })
    }

    pub fn event(&self, event: Event) -> Result<(), String> {
        self.control(event, None)
    }

    pub fn finish(&self, message: String) {
        let (tx, rx) = mpsc::sync_channel(1);
        if self
            .control(Event::Disconnected { message }, Some(tx))
            .is_ok()
        {
            // The helper process can exit even if the parent stopped reading.
            let _ = rx.recv_timeout(Duration::from_secs(2));
        }
    }

    pub fn publish(
        &self,
        image: &DecodedImage,
        damage: &mut Damage,
        revision: &mut u64,
    ) -> Result<(), String> {
        if matches!(damage, Damage::Clean) || self.frame_busy.load(Ordering::Acquire) {
            return Ok(());
        }
        let next = revision
            .checked_add(1)
            .ok_or("Desktop frame revisions exhausted")?;
        let packet = damage.packet(image, next)?;
        self.frame_busy.store(true, Ordering::Release);
        match self.sender.try_send(packet) {
            Ok(()) => {
                *revision = next;
                *damage = Damage::Clean;
                Ok(())
            }
            Err(TrySendError::Full(_)) => {
                self.frame_busy.store(false, Ordering::Release);
                Ok(())
            }
            Err(TrySendError::Disconnected(_)) => Err("Desktop parent frame pipe closed".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ironrdp_graphics::image_processing::PixelFormat;
    use std::{io, sync::Mutex, time::Instant};

    struct PausedWriter {
        started: SyncSender<()>,
        resume: Option<mpsc::Receiver<()>>,
        bytes: Arc<Mutex<Vec<u8>>>,
    }
    impl Write for PausedWriter {
        fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
            if let Some(resume) = self.resume.take() {
                self.started.send(()).unwrap();
                resume.recv_timeout(Duration::from_secs(5)).unwrap();
            }
            self.bytes.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn paused_frame_preserves_control_order_and_accumulated_clipped_damage() {
        let (started, wait_started) = mpsc::sync_channel(1);
        let (resume, wait_resume) = mpsc::sync_channel(1);
        let bytes = Arc::new(Mutex::new(Vec::new()));
        let output = Output::new(PausedWriter {
            started,
            resume: Some(wait_resume),
            bytes: bytes.clone(),
        })
        .unwrap();
        let image = DecodedImage::new(PixelFormat::RgbA32, 8, 8);
        let mut revision = 0;
        let mut damage = Damage::Full;
        output.publish(&image, &mut damage, &mut revision).unwrap();
        wait_started.recv_timeout(Duration::from_secs(2)).unwrap();
        output.event(Event::ClipboardReady).unwrap();
        for (left, top, right, bottom) in
            [(5, 3, 6, 4), (2, 1, 3, 2), (7, 5, 99, 6), (12, 1, 15, 3)]
        {
            damage.update(
                InclusiveRectangle {
                    left,
                    top,
                    right,
                    bottom,
                },
                8,
                8,
            );
            output.publish(&image, &mut damage, &mut revision).unwrap();
        }
        assert_eq!(revision, 1);
        resume.send(()).unwrap();
        let end = Instant::now() + Duration::from_secs(2);
        while revision == 1 && Instant::now() < end {
            output.publish(&image, &mut damage, &mut revision).unwrap();
            std::thread::yield_now();
        }
        assert_eq!(revision, 2);
        output.finish("done".into());
        let bytes = bytes.lock().unwrap();
        let mut cursor = bytes.as_slice();
        let (kind, full) = wire::read_packet(&mut cursor).unwrap();
        assert_eq!((kind, full.len()), (2, 12 + 8 * 8 * 4));
        assert!(full[12..].chunks_exact(4).all(|p| p[3] == 255));
        let (kind, event) = wire::read_packet(&mut cursor).unwrap();
        assert_eq!(kind, 1);
        assert!(matches!(
            serde_json::from_slice(&event),
            Ok(Event::ClipboardReady)
        ));
        let (kind, patch) = wire::read_packet(&mut cursor).unwrap();
        assert_eq!((kind, patch.len()), (3, 20 + 6 * 6 * 4));
        assert_eq!(
            &patch[..20],
            &[0, 8, 0, 8, 0, 0, 0, 0, 0, 0, 0, 2, 0, 2, 0, 1, 0, 6, 0, 6]
        );
        assert!(patch[20..].chunks_exact(4).all(|p| p[3] == 255));
        let (kind, event) = wire::read_packet(&mut cursor).unwrap();
        assert_eq!(kind, 1);
        assert!(matches!(
            serde_json::from_slice(&event),
            Ok(Event::Disconnected { .. })
        ));
        assert!(cursor.is_empty());
    }
}

#[cfg(test)]
#[path = "graphics_tests.rs"]
mod graphics_tests;
