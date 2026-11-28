//! Bounded pixel rectangles shared by the private RDP wire and frame mailbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DamageRect {
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}
impl DamageRect {
    pub fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        let right = self
            .x
            .saturating_add(self.width)
            .max(other.x.saturating_add(other.width));
        let bottom = self
            .y
            .saturating_add(self.height)
            .max(other.y.saturating_add(other.height));
        Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        }
    }
}
pub fn patch_bounds(cw: u16, ch: u16, x: u16, y: u16, w: u16, h: u16) -> Result<usize, String> {
    if cw == 0
        || ch == 0
        || cw > 8192
        || ch > 8192
        || u32::from(cw) * u32::from(ch) > 16 * 1024 * 1024
        || w == 0
        || h == 0
        || u32::from(x) + u32::from(w) > u32::from(cw)
        || u32::from(y) + u32::from(h) > u32::from(ch)
    {
        return Err("Invalid desktop damage rectangle".into());
    }
    Ok(usize::from(w) * usize::from(h) * 4)
}
