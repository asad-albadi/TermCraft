//! Pure, bounded ScreenCaptureKit SDR BGRA conversion and coordinate mapping.
use super::{Frame, MAX_PIXELS};
use std::io;
pub(super) fn bgra(width: usize, height: usize, stride: usize, bytes: &[u8]) -> io::Result<Frame> {
    let pixels = width
        .checked_mul(height)
        .filter(|n| *n > 0 && *n <= MAX_PIXELS)
        .ok_or_else(|| io::Error::other("Selected display exceeds VNC limit"))?;
    if width > u16::MAX as usize
        || height > u16::MAX as usize
        || stride < width * 4
        || stride > MAX_PIXELS * 4
        || bytes.len() != stride.checked_mul(height).unwrap_or(0)
    {
        return Err(io::Error::other("Invalid ScreenCaptureKit frame layout"));
    }
    let mut rgba = Vec::with_capacity(pixels * 4);
    for row in bytes.chunks_exact(stride) {
        for p in row[..width * 4].chunks_exact(4) {
            rgba.extend_from_slice(&[p[2], p[1], p[0], 255]);
        }
    }
    Ok(Frame {
        width: width as u16,
        height: height as u16,
        rgba,
    })
}
pub(super) fn position(x: u16, y: u16, w: u16, h: u16, bounds: [f64; 4]) -> io::Result<(f64, f64)> {
    if w == 0
        || h == 0
        || x >= w
        || y >= h
        || !bounds.iter().all(|v| v.is_finite())
        || bounds[2] <= 0.0
        || bounds[3] <= 0.0
    {
        return Err(io::Error::other("Pointer is outside selected display"));
    }
    Ok((
        bounds[0] + x as f64 * bounds[2] / w as f64,
        bounds[1] + y as f64 * bounds[3] / h as f64,
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn padded_bgra_is_bounded_and_converted() {
        let f = bgra(1, 2, 8, &[1, 2, 3, 0, 9, 9, 9, 9, 4, 5, 6, 0, 9, 9, 9, 9]).unwrap();
        assert_eq!(f.rgba, [3, 2, 1, 255, 6, 5, 4, 255]);
        for (w, h, s, b) in [
            (0, 1, 4, 4),
            (1, 1, 3, 3),
            (1, 2, 8, 8),
            (usize::MAX, 2, 4, 4),
        ] {
            assert!(bgra(w, h, s, &vec![0; b]).is_err());
        }
    }
    #[test]
    fn retina_selected_monitor_origin_and_edges() {
        assert_eq!(
            position(100, 50, 400, 200, [-1920., 0., 200., 100.]).unwrap(),
            (-1870., 25.)
        );
        assert!(position(400, 0, 400, 200, [0., 0., 200., 100.]).is_err());
    }
}
