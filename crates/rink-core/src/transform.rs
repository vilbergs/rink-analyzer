//! Maps rink meters to screen pixels and back. Drawing and hit-testing both go
//! through one `RinkTransform`, so what you click is what you see.

use glam::{Mat2, Vec2};

use crate::rink::RinkSpec;

/// How far in the view zooms, relative to the fitted view.
pub const MAX_ZOOM: f32 = 8.0;

/// Zoom and pan on top of the fitted rink. A view setting; never saved with the play.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewZoom {
    /// 1.0 is the fitted view.
    pub zoom: f32,
    /// Offset of center ice from where the fit puts it, in pixels.
    pub pan: Vec2,
}

impl Default for ViewZoom {
    fn default() -> Self {
        ViewZoom { zoom: 1.0, pan: Vec2::ZERO }
    }
}

/// The canvas a rink is fitted into and how it sits there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    /// Top-left corner of the canvas, in pixels.
    pub origin: Vec2,
    pub size: Vec2,
    /// Screen space kept free around what is fitted, in pixels.
    pub margin_px: f32,
    /// Rotate the view 180 degrees: home attacks left, or up in portrait.
    pub flipped: bool,
    /// The rink's length runs down the screen, -x at the bottom and +y to the left.
    pub portrait: bool,
    /// The rink x-range, in meters, that fills the canvas at zoom 1, across the
    /// rink's full width. `None` fits the whole rink.
    pub crop: Option<[f32; 2]>,
}

impl Viewport {
    /// The whole rink, landscape, home attacking right, with a 24 px margin.
    pub fn new(origin: Vec2, size: Vec2) -> Self {
        Viewport { origin, size, margin_px: 24.0, flipped: false, portrait: false, crop: None }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RinkTransform {
    /// Center ice, in pixels.
    center: Vec2,
    /// Pixels per meter.
    scale: f32,
    /// Rink axes (y up) to screen axes (y down), with orientation and flip.
    /// Its own inverse.
    axes: Mat2,
    /// Center and scale at zoom 1 with no pan.
    fit_center: Vec2,
    fit_scale: f32,
    canvas_center: Vec2,
    half_bounds: Vec2,
    /// Half the rink's extent along the screen axes, in meters.
    half_rink: Vec2,
    margin_px: f32,
}

impl RinkTransform {
    /// Fit `rink`, or its crop, into `viewport`, then apply `view`.
    pub fn fit(rink: &RinkSpec, viewport: &Viewport, view: ViewZoom) -> Self {
        let flip = if viewport.flipped { -1.0 } else { 1.0 };
        let axes = if viewport.portrait {
            Mat2::from_cols(Vec2::new(0.0, -flip), Vec2::new(-flip, 0.0))
        } else {
            Mat2::from_cols(Vec2::new(flip, 0.0), Vec2::new(0.0, -flip))
        };
        let along_screen = |v: Vec2| (axes * v).abs();
        let [x0, x1] = viewport.crop.unwrap_or([-rink.half_length(), rink.half_length()]);
        let fitted = along_screen(Vec2::new((x1 - x0).abs(), rink.width_m));
        let room = viewport.size - Vec2::splat(2.0 * viewport.margin_px);
        let fit_scale = (room / fitted).min_element().max(0.1);
        let half_bounds = viewport.size / 2.0;
        let canvas_center = viewport.origin + half_bounds;
        let fit_center = canvas_center - axes * Vec2::new((x0 + x1) / 2.0, 0.0) * fit_scale;
        let mut t = RinkTransform {
            center: fit_center,
            scale: fit_scale,
            axes,
            fit_center,
            fit_scale,
            canvas_center,
            half_bounds,
            half_rink: along_screen(Vec2::new(rink.half_length(), rink.half_width())),
            margin_px: viewport.margin_px,
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

    /// Keep the boards from coming inside the canvas edges along any axis the
    /// whole rink overflows; along the others the rink stays centered. A crop
    /// at one end of the rink slides to put that end's boards at the edge.
    fn clamped(&self, view: ViewZoom) -> ViewZoom {
        let zoom = view.zoom.clamp(1.0, MAX_ZOOM);
        let half_rink_px = self.half_rink * self.fit_scale * zoom + Vec2::splat(self.margin_px);
        let max = (half_rink_px - self.half_bounds).max(Vec2::ZERO);
        let center = (self.fit_center + view.pan).clamp(self.canvas_center - max, self.canvas_center + max);
        ViewZoom { zoom, pan: center - self.fit_center }
    }

    /// Rink meters (y up) to screen pixels (y down).
    pub fn to_screen(self, p: Vec2) -> Vec2 {
        self.center + self.axes * p * self.scale
    }

    pub fn to_rink(self, p: Vec2) -> Vec2 {
        self.axes * (p - self.center) / self.scale
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fit(flipped: bool, view: ViewZoom) -> RinkTransform {
        let viewport = Viewport { flipped, ..Viewport::new(Vec2::new(10.0, 20.0), Vec2::new(1248.0, 648.0)) };
        RinkTransform::fit(&RinkSpec::iihf(), &viewport, view)
    }

    /// A 390 x 400 phone canvas, portrait, with an 8 px margin.
    fn phone(flipped: bool, crop: Option<[f32; 2]>, view: ViewZoom) -> RinkTransform {
        let viewport = Viewport {
            margin_px: 8.0,
            flipped,
            portrait: true,
            crop,
            ..Viewport::new(Vec2::new(0.0, 100.0), Vec2::new(390.0, 400.0))
        };
        RinkTransform::fit(&RinkSpec::iihf(), &viewport, view)
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

    #[test]
    fn portrait_round_trips() {
        for flipped in [false, true] {
            for crop in [None, Some([-30.0, -8.0])] {
                let view = ViewZoom { zoom: 2.5, pan: Vec2::new(30.0, -40.0) };
                let t = phone(flipped, crop, view);
                let p = Vec2::new(-12.5, 7.25);
                let back = t.to_rink(t.to_screen(p));
                assert!(back.distance(p) < 1e-3, "flipped={flipped} crop={crop:?}: {back}");
            }
        }
    }

    #[test]
    fn portrait_puts_minus_x_at_the_bottom_and_plus_y_left() {
        let t = phone(false, None, ViewZoom::default());
        let own = t.to_screen(Vec2::new(-25.0, 0.0));
        let theirs = t.to_screen(Vec2::new(25.0, 0.0));
        assert!(own.y > theirs.y);
        assert!((own.x - theirs.x).abs() < 1e-3);
        let o = t.to_screen(Vec2::ZERO);
        assert!(t.to_screen(Vec2::new(0.0, 10.0)).x < o.x);

        let f = phone(true, None, ViewZoom::default());
        assert!(f.to_screen(Vec2::new(-25.0, 0.0)).y < f.to_screen(Vec2::new(25.0, 0.0)).y);
        assert!(f.to_screen(Vec2::new(0.0, 10.0)).x > o.x);
    }

    #[test]
    fn crop_fills_the_canvas() {
        let t = phone(false, Some([-30.0, -8.0]), ViewZoom::default());
        let expected = ((390.0 - 16.0) / 30.0_f32).min((400.0 - 16.0) / 22.0);
        assert!((t.scale() - expected).abs() < 1e-3, "{}", t.scale());
        // Width limits the scale, so the strip is shorter than the canvas and
        // slides down until the end boards sit at the bottom margin.
        assert!((t.to_screen(Vec2::new(-30.0, 0.0)).y - (100.0 + 400.0 - 8.0)).abs() < 1e-3);
        assert!(t.to_screen(Vec2::new(-8.0, 0.0)).y > 100.0 + 8.0);
        let side = t.to_screen(Vec2::new(-20.0, 15.0)).x;
        assert!((side - 8.0).abs() < 1e-3, "{side}");
    }

    #[test]
    fn crop_away_from_the_ends_centers_the_strip() {
        let t = phone(false, Some([-10.0, 10.0]), ViewZoom::default());
        let mid = t.to_screen(Vec2::ZERO);
        assert!(mid.distance(Vec2::new(195.0, 300.0)) < 1e-3, "{mid}");
        let s = phone(false, Some([-15.0, -5.0]), ViewZoom::default());
        let mid = s.to_screen(Vec2::new(-10.0, 0.0));
        assert!(mid.distance(Vec2::new(195.0, 300.0)) < 1e-3, "{mid}");
    }

    #[test]
    fn cropped_pan_clamps_against_the_whole_rink() {
        let t = phone(false, Some([-30.0, -8.0]), ViewZoom::default());
        // Dragging up would lift the end boards off the bottom edge.
        let up = t.panned(ViewZoom::default(), Vec2::new(0.0, -500.0));
        let u = phone(false, Some([-30.0, -8.0]), up);
        assert!((u.to_screen(Vec2::new(-30.0, 0.0)).y - 492.0).abs() < 1e-3);
        // Dragging down runs the length of the rink, to the far boards and no further.
        let down = t.panned(ViewZoom::default(), Vec2::new(0.0, 5000.0));
        let d = phone(false, Some([-30.0, -8.0]), down);
        assert!((d.to_screen(Vec2::new(30.0, 0.0)).y - 108.0).abs() < 1e-3);
        // The rink is narrower than the canvas: no sideways pan.
        let side = t.panned(ViewZoom::default(), Vec2::new(80.0, 0.0));
        assert_eq!(phone(false, Some([-30.0, -8.0]), side).to_screen(Vec2::ZERO).x, 195.0);
    }
}
