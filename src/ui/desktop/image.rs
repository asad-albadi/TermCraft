//! Convert remote pixels in this crate, so review dependency build settings do
//! not put an unoptimized per-pixel conversion on the rendering path.
use eframe::egui::{Color32, ColorImage};

pub(super) fn from_rgba(size: [usize; 2], rgba: &[u8], opaque: bool) -> ColorImage {
    assert_eq!(
        size[0].checked_mul(size[1]).and_then(|n| n.checked_mul(4)),
        Some(rgba.len()),
        "desktop image dimensions must match its RGBA data"
    );
    // The RDP helper guarantees opaque frame pixels. Keep ordinary alpha
    // conversion for VNC and cursor images, which may contain transparency.
    let pixels = if opaque {
        rgba.chunks_exact(4)
            .map(|p| Color32::from_rgb(p[0], p[1], p[2]))
            .collect()
    } else {
        rgba.chunks_exact(4)
            .map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3]))
            .collect()
    };
    ColorImage::new(size, pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn remote_pixel_conversion_preserves_opaque_and_transparent_colors() {
        let translucent = [
            255, 10, 20, 0, 9, 100, 240, 1, 50, 130, 80, 127, 10, 20, 30, 255,
        ];
        assert_eq!(
            from_rgba([2, 2], &translucent, false),
            ColorImage::from_rgba_unmultiplied([2, 2], &translucent)
        );
        let opaque = [3, 99, 200, 255].repeat(20);
        assert_eq!(
            from_rgba([5, 4], &opaque, true),
            ColorImage::from_rgba_unmultiplied([5, 4], &opaque)
        );
    }
    #[test]
    #[should_panic(expected = "desktop image dimensions must match")]
    fn remote_pixel_conversion_rejects_truncated_data() {
        from_rgba([2, 2], &[0; 15], true);
    }
}
