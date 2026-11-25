use super::{DesktopEvent, Frame, bytes, wire};
use tokio::io::AsyncRead;

pub(super) const HEXTILE: i32 = 5;
pub(super) const CURSOR: i32 = -239;

#[derive(Clone, Copy)]
pub(super) struct Rect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

impl Rect {
    pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
        Self {
            x: x.into(),
            y: y.into(),
            width: width.into(),
            height: height.into(),
        }
    }

    fn rows(
        self,
        frame_width: u16,
    ) -> impl DoubleEndedIterator<Item = std::ops::Range<usize>> + ExactSizeIterator {
        (self.y..self.y + self.height).map(move |y| {
            let offset = (y * usize::from(frame_width) + self.x) * 4;
            offset..offset + self.width * 4
        })
    }

    fn fill(self, frame: &mut Frame, color: [u8; 4]) {
        for row in self.rows(frame.width) {
            for pixel in frame.pixels[row].chunks_exact_mut(4) {
                pixel.copy_from_slice(&color);
            }
        }
    }
}

fn rgba(bgrx: &[u8]) -> [u8; 4] {
    [bgrx[2], bgrx[1], bgrx[0], 255]
}

pub(super) async fn raw(
    reader: &mut (impl AsyncRead + Unpin),
    frame: &mut Frame,
    rect: Rect,
) -> Result<(), String> {
    let data = bytes(reader, rect.width * rect.height * 4).await?;
    for (row, data) in rect
        .rows(frame.width)
        .zip(data.chunks_exact(rect.width * 4))
    {
        for (dst, src) in frame.pixels[row]
            .chunks_exact_mut(4)
            .zip(data.chunks_exact(4))
        {
            dst.copy_from_slice(&rgba(src));
        }
    }
    Ok(())
}

pub(super) async fn copyrect(
    reader: &mut (impl AsyncRead + Unpin),
    frame: &mut Frame,
    rect: Rect,
) -> Result<(), String> {
    let src = bytes(reader, 4).await?;
    let sx = usize::from(u16::from_be_bytes(src[..2].try_into().unwrap()));
    let sy = usize::from(u16::from_be_bytes(src[2..].try_into().unwrap()));
    if sx + rect.width > frame.width.into() || sy + rect.height > frame.height.into() {
        return Err("VNC CopyRect source is outside framebuffer".into());
    }
    let source = Rect {
        x: sx,
        y: sy,
        ..rect
    };
    let rows = source.rows(frame.width).zip(rect.rows(frame.width));
    // Reverse row order for a downward overlapping copy. copy_within
    // handles horizontal overlap within each row.
    if rect.y > sy {
        for (src, dst) in rows.rev() {
            frame.pixels.copy_within(src, dst.start);
        }
    } else {
        for (src, dst) in rows {
            frame.pixels.copy_within(src, dst.start);
        }
    }
    Ok(())
}

pub(super) async fn hextile(
    reader: &mut (impl AsyncRead + Unpin),
    frame: &mut Frame,
    rect: Rect,
) -> Result<(), String> {
    const RAW: u8 = 1;
    const BACKGROUND: u8 = 2;
    const FOREGROUND: u8 = 4;
    const SUBRECTS: u8 = 8;
    const COLOURED: u8 = 16;
    let mut background = None;
    let mut foreground = None;
    for y in (0..rect.height).step_by(16) {
        for x in (0..rect.width).step_by(16) {
            let tile = Rect {
                x: rect.x + x,
                y: rect.y + y,
                width: (rect.width - x).min(16),
                height: (rect.height - y).min(16),
            };
            let flags = bytes(reader, 1).await?[0];
            if flags & RAW != 0 {
                raw(reader, frame, tile).await?;
                background = None;
                foreground = None;
                continue;
            }
            let coloured = flags & COLOURED != 0;
            if flags & !31 != 0 || (coloured && flags & FOREGROUND != 0) {
                return Err("Invalid VNC Hextile flags".into());
            }
            if flags & BACKGROUND != 0 {
                background = Some(rgba(&bytes(reader, 4).await?));
            }
            tile.fill(frame, background.ok_or("Missing VNC Hextile background")?);
            if flags & FOREGROUND != 0 {
                foreground = Some(rgba(&bytes(reader, 4).await?));
            }
            if coloured {
                foreground = None;
            }
            if flags & SUBRECTS == 0 {
                continue;
            }
            let count = usize::from(bytes(reader, 1).await?[0]);
            let stride = if coloured { 6 } else { 2 };
            let data = bytes(reader, count * stride).await?;
            for sub in data.chunks_exact(stride) {
                let color = if coloured {
                    rgba(&sub[..4])
                } else {
                    foreground.ok_or("Missing VNC Hextile foreground")?
                };
                let position = sub[stride - 2];
                let dimensions = sub[stride - 1];
                let x = usize::from(position >> 4);
                let y = usize::from(position & 15);
                let width = usize::from(dimensions >> 4) + 1;
                let height = usize::from(dimensions & 15) + 1;
                if x + width > tile.width || y + height > tile.height {
                    return Err("VNC Hextile subrectangle is outside tile".into());
                }
                Rect {
                    x: tile.x + x,
                    y: tile.y + y,
                    width,
                    height,
                }
                .fill(frame, color);
            }
        }
    }
    Ok(())
}

pub(super) async fn cursor(
    reader: &mut (impl AsyncRead + Unpin),
    (x, y): (u16, u16),
    (width, height): (u16, u16),
) -> Result<DesktopEvent, String> {
    if width == 0 || height == 0 {
        return Ok(DesktopEvent::CursorShape {
            width: 0,
            height: 0,
            hotspot: (0, 0),
            pixels: Vec::new(),
        });
    }
    if width > 512 || height > 512 || x >= width || y >= height {
        return Err("Invalid VNC cursor dimensions or hotspot".into());
    }
    let length = wire::dimensions(width, height)?;
    let mut pixels = bytes(reader, length).await?;
    let stride = usize::from(width).div_ceil(8);
    let mask = bytes(reader, stride * usize::from(height)).await?;
    for (row, mask) in pixels
        .chunks_exact_mut(usize::from(width) * 4)
        .zip(mask.chunks_exact(stride))
    {
        for (col, pixel) in row.chunks_exact_mut(4).enumerate() {
            pixel.swap(0, 2);
            pixel[3] = if mask[col / 8] & (128 >> (col % 8)) == 0 {
                0
            } else {
                255
            };
        }
    }
    Ok(DesktopEvent::CursorShape {
        width,
        height,
        hotspot: (x, y),
        pixels,
    })
}
