use std::io;
const MAX_PIXELS: usize = 8_388_608;
#[derive(Clone, Debug)]
pub struct Frame {
    pub width: u16,
    pub height: u16,
    pub rgba: Vec<u8>,
}
type Cancel = std::sync::Arc<dyn Fn() -> bool + Send + Sync>;
pub mod capture;
fn decode(
    format: ::pipewire::spa::param::video::VideoFormat,
    width: u32,
    height: u32,
    bytes: &[u8],
    offset: u32,
    size: u32,
    stride: i32,
) -> io::Result<Frame> {
    use ::pipewire::spa::param::video::VideoFormat as F;
    let bad = || io::Error::other("Unsupported or out-of-bounds portal video frame");
    let w = usize::try_from(width).map_err(|_| bad())?;
    let h = usize::try_from(height).map_err(|_| bad())?;
    if w == 0
        || h == 0
        || width > u16::MAX as u32
        || height > u16::MAX as u32
        || w.checked_mul(h).filter(|p| *p <= MAX_PIXELS).is_none()
        || ![F::RGBx, F::RGBA, F::BGRx, F::BGRA, F::RGB, F::BGR].contains(&format)
    {
        return Err(bad());
    }
    let bpp = if format == F::RGB || format == F::BGR {
        3
    } else {
        4
    };
    let row = w.checked_mul(bpp).ok_or_else(bad)?;
    let stride = usize::try_from(stride).map_err(|_| bad())?;
    let offset = offset as usize;
    let size = size as usize;
    let needed = (h - 1)
        .checked_mul(stride)
        .and_then(|n| n.checked_add(row))
        .ok_or_else(bad)?;
    if stride < row
        || needed > size
        || offset
            .checked_add(size)
            .filter(|n| *n <= bytes.len())
            .is_none()
    {
        return Err(bad());
    }
    let mut rgba = vec![0; w * h * 4];
    for y in 0..h {
        for (src, dst) in bytes[offset + y * stride..offset + y * stride + row]
            .chunks_exact(bpp)
            .zip(rgba[y * w * 4..(y + 1) * w * 4].chunks_exact_mut(4))
        {
            if format == F::BGRx || format == F::BGRA || format == F::BGR {
                dst.copy_from_slice(&[src[2], src[1], src[0], 255]);
            } else {
                dst.copy_from_slice(&[src[0], src[1], src[2], 255]);
            }
        }
    }
    Ok(Frame {
        width: width as u16,
        height: height as u16,
        rgba,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    use ::pipewire::spa::param::video::VideoFormat as F;
    #[test]
    fn mapped_rows_respect_chunk_offset_padding_and_channel_order() {
        let data = [
            99, 99, 3, 2, 1, 0, 6, 5, 4, 0, 88, 88, 88, 88, 9, 8, 7, 0, 12, 11, 10, 0,
        ];
        let f = decode(F::BGRx, 2, 2, &data, 2, 20, 12).unwrap();
        assert_eq!(
            f.rgba,
            vec![1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255, 10, 11, 12, 255]
        );
        assert_eq!((f.width, f.height), (2, 2));
    }
    #[test]
    fn invalid_mapping_format_and_frame_bounds_are_rejected() {
        for (fmt, w, h, off, size, stride) in [
            (F::I420, 1, 1, 0, 4, 4),
            (F::RGBA, 0, 1, 0, 4, 4),
            (F::RGBA, 65536, 1, 0, 4, 4),
            (F::RGBA, 4096, 4096, 0, 4, 4),
            (F::RGBA, 2, 1, 0, 4, 4),
            (F::RGBA, 1, 2, 0, 8, -4),
            (F::RGBA, 1, 1, 5, 4, 4),
            (F::RGBA, 1, 1, 0, 3, 4),
        ] {
            assert!(decode(fmt, w, h, &[0; 8], off, size, stride).is_err());
        }
    }
}
