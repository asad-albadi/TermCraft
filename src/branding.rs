//! Shared brand artwork for the window icon and the workspace texture.
pub fn logo(size: u32) -> image::RgbaImage {
    image::load_from_memory(include_bytes!("../assets/termcraft-logo.png"))
        .expect("bundled TermCraft logo must be a valid PNG")
        .resize_exact(size, size, image::imageops::FilterType::Lanczos3)
        .into_rgba8()
}
