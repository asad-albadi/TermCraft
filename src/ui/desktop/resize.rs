//! Pane resize scheduling. Separate from negotiated framebuffer/input coordinates.
use std::time::{Duration, Instant};
#[derive(Default)]
pub(super) struct PaneResize {
    candidate: Option<(u16, u16)>,
    changed: Option<Instant>,
    sent: Option<((u16, u16), Instant)>,
}
impl PaneResize {
    pub fn observe(
        &mut self,
        width: f32,
        height: f32,
        density: f32,
        now: Instant,
    ) -> Option<(u16, u16)> {
        if !width.is_finite()
            || !height.is_finite()
            || !density.is_finite()
            || width < 1.
            || height < 1.
            || density <= 0.
        {
            return None;
        }
        let mut w = (width * density).round().clamp(200., 8192.);
        let mut h = (height * density).round().clamp(200., 8192.);
        let budget = (16. * 1024. * 1024. / (w * h)).sqrt().min(1.);
        w = (w * budget).floor();
        h = (h * budget).floor();
        let size = (((w as u16) & !1).max(200), (h as u16).max(200));
        if self.candidate != Some(size) {
            self.candidate = Some(size);
            self.changed = Some(now);
        }
        if now.saturating_duration_since(self.changed?) < Duration::from_millis(150)
            || self.sent.is_some_and(|(last, when)| {
                last == size || now.saturating_duration_since(when) < Duration::from_millis(500)
            })
        {
            return None;
        }
        Some(size)
    }
    pub fn sent(&mut self, dimensions: (u16, u16), now: Instant) {
        self.sent = Some((dimensions, now));
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pane_resize_debounces_drag_and_sends_only_latest_stable_size() {
        let start = Instant::now();
        let mut sizing = PaneResize::default();
        assert_eq!(sizing.observe(800., 600., 1., start), None);
        assert_eq!(
            sizing.observe(1000., 700., 1., start + Duration::from_millis(100)),
            None
        );
        let ready = start + Duration::from_millis(260);
        assert_eq!(sizing.observe(1000., 700., 1., ready), Some((1000, 700)));
        sizing.sent((1000, 700), ready);
        assert_eq!(
            sizing.observe(1000., 700., 1., ready + Duration::from_secs(1)),
            None
        );
        assert_eq!(
            sizing.observe(1100., 700., 1., ready + Duration::from_millis(1)),
            None
        );
        assert_eq!(
            sizing.observe(1100., 700., 1., ready + Duration::from_millis(160)),
            None
        );
        assert_eq!(
            sizing.observe(1100., 700., 1., ready + Duration::from_millis(501)),
            Some((1100, 700))
        );
    }
    #[test]
    fn pane_resize_uses_physical_pixels_even_width_and_bounded_frame_budget() {
        let now = Instant::now();
        let mut sizing = PaneResize::default();
        sizing.observe(801., 601., 1.5, now);
        assert_eq!(
            sizing.observe(801., 601., 1.5, now + Duration::from_secs(1)),
            Some((1202, 902))
        );
        sizing.observe(10000., 10000., 2., now + Duration::from_secs(2));
        let (w, h) = sizing
            .observe(10000., 10000., 2., now + Duration::from_secs(3))
            .unwrap();
        assert_eq!(w % 2, 0);
        assert!(u32::from(w) * u32::from(h) <= 16 * 1024 * 1024);
        assert_eq!(sizing.observe(f32::NAN, 600., 1., now), None);
        assert_eq!(sizing.observe(0., 0., 1., now), None);
    }
}
