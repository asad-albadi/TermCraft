use super::read_regular;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use std::{io::Cursor, path::Path};

const INPUT_LIMIT: u64 = 32 * 1024 * 1024;
pub const MAX_PIXELS: u64 = 16_000_000;
const MAX_DIMENSION: u32 = 8192;

#[derive(Debug)]
pub struct ImagePreview {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl ImagePreview {
    pub fn open(path: &Path, max_pixels: u64) -> Result<Self, String> {
        let encoded = read_regular(path, INPUT_LIMIT)?;
        let mut reader = ImageReader::new(Cursor::new(encoded))
            .with_guessed_format()
            .map_err(|error| error.to_string())?;
        if !matches!(reader.format(), Some(ImageFormat::Png | ImageFormat::Jpeg)) {
            return Err("Select a PNG or JPEG image".into());
        }

        let mut limits = Limits::default();
        limits.max_image_width = Some(MAX_DIMENSION);
        limits.max_image_height = Some(MAX_DIMENSION);
        limits.max_alloc = Some(128 * 1024 * 1024);
        reader.limits(limits);
        let decoder = reader.into_decoder().map_err(|error| error.to_string())?;
        let (width, height) = decoder.dimensions();
        if u64::from(width) * u64::from(height) > max_pixels.min(MAX_PIXELS) {
            return Err("Image exceeds the preview pixel limit".into());
        }
        let rgba = DynamicImage::from_decoder(decoder)
            .map_err(|error| error.to_string())?
            .into_rgba8()
            .into_raw();
        Ok(Self {
            width,
            height,
            rgba,
        })
    }
}
