//! Complete, bounded RGBA snapshots; intermediate updates are coalesced.
#[derive(Clone)]
pub struct Frame {
    pub width: u16,
    pub height: u16,
    pub revision: u64,
    pub pixels: Vec<u8>,
}
