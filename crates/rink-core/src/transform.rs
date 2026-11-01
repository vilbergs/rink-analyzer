//! Maps rink meters to screen pixels and back. Drawing and hit-testing both go
//! through one `RinkTransform`, so what you click is what you see.

use glam::Vec2;

use crate::rink::RinkSpec;

/// Screen space kept free around the boards, in pixels.
const MARGIN_PX: f32 = 24.0;
/// How far in the view zooms, relative to the whole rink fitting.
pub const MAX_ZOOM: f32 = 8.0;

/// Zoom and pan on top of the fitted rink. A view setting; never saved with the play.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewZoom {
    /// 1.0 fits the whole rink.
    pub zoom: f32,
    /// Offset of center ice from the canvas center, in pixels.
    pub pan: Vec2,
}

impl Default for ViewZoom {
    fn default() -> Self {
        ViewZoom { zoom: 1.0, pan: Vec2::ZERO }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RinkTransform {
    center: Vec2,
    /// Pixels per meter.
    scale: f32,
    /// +1 shows home attacking right; -1 rotates the view 180 degrees.
    flip: f32,
    /// Center and scale with the whole rink in view, before zoom and pan.
    fit_center: Vec2,
    fit_scale: f32,
    half_bounds: Vec2,
    half_rink: Vec2,
}

impl RinkTransform {
    /// Fit `rink` into the canvas at `origin` with `size`, both in pixels.
    pub fn fit(origin: Vec2, size: Vec2, rink: &RinkSpec, flipped: bool, view: ViewZoom) -> Self {
        let w = size.x - 2.0 * MARGIN_PX;
        let h = size.y - 2.0 * MARGIN_PX;
        let fit_scale = (w / rink.length_m).min(h / rink.width_m).max(0.1);
        let half_bounds = size / 2.0;
        let fit_center = origin + half_bounds;
        let mut t = RinkTransform {
            center: fit_center,
            scale: fit_scale,
            flip: if flipped { -1.0 } else { 1.0 },
            fit_center,
            fit_scale,
            half_bounds,
            half_rink: Vec2::new(rink.length_m / 2.0, rink.width_m / 2.0),
        };
        let view = t.clamped(view);
        t.scale = fit_scale * view.zoom;
        t.center = fit_center + view.pan;
        t
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// `view` zoomed by `factor`, keeping the rink point under `at` (pixels) where it is.
    pub fn zoomed(&self, view: ViewZoom, at: Vec2, factor: f32) -> ViewZoom {
        // Work from `view`, not this transform's own center and scale: several
        // gesture steps can land before the next paint.
        let view = self.clamped(view);
        let zoom = (view.zoom * factor).clamp(1.0, MAX_ZOOM);
        let center = at + (self.fit_center + view.pan - at) * (zoom / view.zoom);
        self.clamped(ViewZoom { zoom, pan: center - self.fit_center })
    }

    /// `view` moved by `delta` pixels.
    pub fn panned(&self, view: ViewZoom, delta: Vec2) -> ViewZoom {
        self.clamped(ViewZoom { zoom: view.zoom, pan: view.pan + delta })
    }

    /// Keep the boards from coming inside the canvas edges once the rink is
    /// bigger than the canvas; at zoom 1 the rink stays centered.
    fn clamped(&self, view: ViewZoom) -> ViewZoom {
        let zoom = view.zoom.clamp(1.0, MAX_ZOOM);
        let half_rink_px = self.half_rink * self.fit_scale * zoom + Vec2::splat(MARGIN_PX);
        let max = (half_rink_px - self.half_bounds).max(Vec2::ZERO);
        ViewZoom { zoom, pan: view.pan.clamp(-max, max) }
    }

    /// Rink meters (y up) to screen pixels (y down).
    pub fn to_screen(self, p: Vec2) -> Vec2 {
        Vec2::new(self.center.x + p.x * self.scale * self.flip, self.center.y - p.y * self.scale * self.flip)
    }

    pub fn to_rink(self, p: Vec2) -> Vec2 {
        let x = (p.x - self.center.x) / (self.scale * self.flip);
        let y = -(p.y - self.center.y) / (self.scale * self.flip);
        Vec2::new(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(flipped: bool, view: ViewZoom) -> RinkTransform {
        RinkTransform::fit(Vec2::new(10.0, 20.0), Vec2::new(1248.0, 648.0), &RinkSpec::iihf(), flipped, view)
    }

    #[test]
    fn round_trips() {
        for flipped in [false, true] {
            let t = fit(flipped, ViewZoom::default());
            let p = Vec2::new(-12.5, 7.25);
            let back = t.to_rink(t.to_screen(p));
            assert!(back.distance(p) < 1e-3, "flipped={flipped}: {back}");
        }
    }

    #[test]
    fn home_attacks_right_and_plus_y_is_up() {
        let t = fit(false, ViewZoom::default());
        let o = t.to_screen(Vec2::ZERO);
        assert!(t.to_screen(Vec2::new(10.0, 0.0)).x > o.x);
        assert!(t.to_screen(Vec2::new(0.0, 10.0)).y < o.y);
        let f = fit(true, ViewZoom::default());
        assert!(f.to_screen(Vec2::new(10.0, 0.0)).x < o.x);
    }

    #[test]
    fn fits_rink_inside_bounds() {
        let t = fit(false, ViewZoom::default());
        // 1200 x 600 usable pixels for a 60 x 30 m rink = 20 px per meter.
        assert!((t.scale() - 20.0).abs() < 1e-3);
    }

    #[test]
    fn zoom_keeps_the_point_under_the_cursor() {
        for flipped in [false, true] {
            let t = fit(flipped, ViewZoom::default());
            let at = t.to_screen(Vec2::new(20.0, -7.0));
            let mut view = t.zoomed(ViewZoom::default(), at, 2.0);
            // A second step before the next paint, from the same transform.
            view = t.zoomed(view, at, 1.5);
            let z = fit(flipped, view);
            assert!((z.scale() - 60.0).abs() < 1e-3);
            assert!(z.to_rink(at).distance(Vec2::new(20.0, -7.0)) < 1e-3, "flipped={flipped}");
        }
    }

    #[test]
    fn zoom_and_pan_stay_in_range() {
        let t = fit(false, ViewZoom::default());
        let at = Vec2::new(300.0, 300.0);
        assert_eq!(t.zoomed(ViewZoom::default(), at, 0.5), ViewZoom::default());
        assert_eq!(t.zoomed(ViewZoom::default(), at, 100.0).zoom, MAX_ZOOM);
        assert_eq!(t.panned(ViewZoom::default(), Vec2::new(50.0, 50.0)).pan, Vec2::ZERO);

        // Zoomed 2x, the rink plus margins is 2448 px wide in a 1248 px canvas: 600 px each way.
        let far = t.panned(ViewZoom { zoom: 2.0, pan: Vec2::ZERO }, Vec2::new(5000.0, 0.0));
        assert!((far.pan.x - 600.0).abs() < 1e-3);
    }
}
