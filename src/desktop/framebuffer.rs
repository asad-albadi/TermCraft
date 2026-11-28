//! Complete, bounded RGBA snapshots; intermediate updates are coalesced.
#[derive(Clone)]
pub struct Frame {
    pub width: u16,
    pub height: u16,
    pub revision: u64,
    pub pixels: Vec<u8>,
}

pub use super::wire::damage::DamageRect;
use super::wire::damage::patch_bounds;
pub struct FramePatch {
    pub width: u16,
    pub height: u16,
    pub revision: u64,
    pub rect: DamageRect,
    pub pixels: Vec<u8>,
}
impl FramePatch {
    pub fn apply(&self, frame: &mut Frame) -> Result<(), String> {
        let len = patch_bounds(
            self.width,
            self.height,
            self.rect.x,
            self.rect.y,
            self.rect.width,
            self.rect.height,
        )?;
        if (self.width, self.height) != (frame.width, frame.height)
            || self.revision <= frame.revision
            || self.pixels.len() != len
            || frame.pixels.len() != usize::from(frame.width) * usize::from(frame.height) * 4
        {
            return Err("Desktop patch has no matching framebuffer or revision".into());
        }
        let stride = usize::from(frame.width) * 4;
        let row = usize::from(self.rect.width) * 4;
        for y in 0..usize::from(self.rect.height) {
            let start = (usize::from(self.rect.y) + y) * stride + usize::from(self.rect.x) * 4;
            frame.pixels[start..start + row].copy_from_slice(&self.pixels[y * row..(y + 1) * row]);
        }
        frame.revision = self.revision;
        Ok(())
    }
}
pub enum FrameUpdate {
    Full(Frame),
    Patch(FramePatch),
}
#[derive(Default)]
pub struct FrameStore {
    frame: Option<Frame>,
    dirty: Option<DamageRect>,
    full: bool,
}
impl FrameStore {
    pub fn snapshot(&self) -> Option<Frame> {
        self.frame.clone()
    }
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn full(&mut self, frame: Frame) -> Result<(), String> {
        let len = patch_bounds(frame.width, frame.height, 0, 0, frame.width, frame.height)?;
        if frame.pixels.len() != len || frame.revision == 0 {
            return Err("Invalid desktop full frame".into());
        }
        self.frame = Some(frame);
        self.full = true;
        self.dirty = None;
        Ok(())
    }
    pub fn patch(&mut self, patch: FramePatch) -> Result<(), String> {
        let frame = self
            .frame
            .as_mut()
            .ok_or("Desktop damage arrived before its full framebuffer")?;
        if frame.revision.checked_add(1) != Some(patch.revision) {
            return Err("Desktop patch revision is not contiguous".into());
        }
        patch.apply(frame)?;
        self.dirty = Some(self.dirty.map_or(patch.rect, |old| old.union(patch.rect)));
        Ok(())
    }
    pub fn take(&mut self) -> Option<FrameUpdate> {
        let frame = self.frame.as_ref()?;
        if self.full {
            self.full = false;
            self.dirty = None;
            return Some(FrameUpdate::Full(frame.clone()));
        }
        let rect = self.dirty.take()?;
        let row = usize::from(rect.width) * 4;
        let mut pixels = Vec::with_capacity(row * usize::from(rect.height));
        for y in usize::from(rect.y)..usize::from(rect.y + rect.height) {
            let start = (y * usize::from(frame.width) + usize::from(rect.x)) * 4;
            pixels.extend_from_slice(&frame.pixels[start..start + row]);
        }
        Some(FrameUpdate::Patch(FramePatch {
            width: frame.width,
            height: frame.height,
            revision: frame.revision,
            rect,
            pixels,
        }))
    }
}
#[cfg(test)]
mod damage_tests {
    use super::*;
    #[test]
    fn patches_coalesce_without_losing_pixels_and_take_only_dirty_region() {
        let mut store = FrameStore::default();
        store
            .full(Frame {
                width: 100,
                height: 80,
                revision: 1,
                pixels: vec![0; 100 * 80 * 4],
            })
            .unwrap();
        assert!(matches!(store.take(), Some(FrameUpdate::Full(_))));
        for (revision, x) in [(2, 10), (3, 12)] {
            store
                .patch(FramePatch {
                    width: 100,
                    height: 80,
                    revision,
                    rect: DamageRect {
                        x,
                        y: 20,
                        width: 1,
                        height: 1,
                    },
                    pixels: vec![x as u8, 0, 0, 255],
                })
                .unwrap();
        }
        let Some(FrameUpdate::Patch(p)) = store.take() else {
            panic!("expected cropped update")
        };
        assert_eq!(
            p.rect,
            DamageRect {
                x: 10,
                y: 20,
                width: 3,
                height: 1
            }
        );
        assert_eq!(p.revision, 3);
        assert_eq!(p.pixels.len(), 12);
        assert_eq!(p.pixels[0], 10);
        assert_eq!(p.pixels[8], 12);
        assert!(store.take().is_none());
    }
    #[test]
    fn patches_reject_missing_base_geometry_revision_gaps_and_bad_lengths() {
        let patch = || FramePatch {
            width: 4,
            height: 3,
            revision: 2,
            rect: DamageRect {
                x: 3,
                y: 2,
                width: 1,
                height: 1,
            },
            pixels: vec![0; 4],
        };
        let mut store = FrameStore::default();
        assert!(store.patch(patch()).is_err());
        store
            .full(Frame {
                width: 4,
                height: 3,
                revision: 1,
                pixels: vec![0; 48],
            })
            .unwrap();
        let mut p = patch();
        p.rect.width = 2;
        assert!(store.patch(p).is_err());
        let mut p = patch();
        p.revision = 3;
        assert!(store.patch(p).is_err());
        let mut p = patch();
        p.pixels.clear();
        assert!(store.patch(p).is_err());
        assert!(store.patch(patch()).is_ok());
    }
}
