//! Maps rink meters to screen pixels and back. Drawing and hit-testing both go
//! through one `RinkTransform`, so what you click is what you see.

use gpui::{point, px, Bounds, Pixels, Point};
use rink_core::{RinkSpec, Vec2};

/// Screen space kept free around the boards, in pixels.
const MARGIN_PX: f32 = 24.0;
/// How far in the view zooms, relative to the whole rink fitting.
pub const MAX_ZOOM: f32 = 8.0;

/// Orbit angles in radians. The camera always looks at center ice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera3d {
    pub yaw: f32,
    pub elevation: f32,
}

impl Default for Camera3d {
    fn default() -> Self {
        Self { yaw: -20.0_f32.to_radians(), elevation: 50.0_f32.to_radians() }
    }
}

impl Camera3d {
    pub fn orbited(self, delta: Vec2) -> Self {
        Self {
            yaw: (self.yaw + delta.x * 0.006).rem_euclid(std::f32::consts::TAU),
            elevation: (self.elevation + delta.y * 0.006).clamp(25.0_f32.to_radians(), 80.0_f32.to_radians()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Perspective {
    camera: Camera3d,
    distance: f32,
}

impl Perspective {
    fn depth(self, p: Vec2, height: f32) -> f32 {
        let p = p.rotate(Vec2::from_angle(self.camera.yaw));
        self.distance + p.y * self.camera.elevation.cos() - height * self.camera.elevation.sin()
    }

    fn project(self, p: Vec2, height: f32) -> Vec2 {
        let p = p.rotate(Vec2::from_angle(self.camera.yaw));
        let (sin, cos) = self.camera.elevation.sin_cos();
        let perspective = self.distance / (self.distance + p.y * cos - height * sin);
        Vec2::new(p.x, -(p.y * sin + height * cos)) * perspective
    }

    fn unproject(self, p: Vec2) -> Option<Vec2> {
        let (sin, cos) = self.camera.elevation.sin_cos();
        let divisor = self.distance * sin + p.y * cos;
        // Rays at or above the horizon never meet the ice in front of the camera.
        if divisor <= 0.001 {
            return None;
        }
        let y = -p.y * self.distance / divisor;
        let x = p.x * (self.distance + y * cos) / self.distance;
        let rink = Vec2::new(x, y).rotate(Vec2::from_angle(-self.camera.yaw));
        rink.is_finite().then_some(rink)
    }
}

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
    perspective: Option<Perspective>,
}

impl RinkTransform {
    pub fn fit(bounds: Bounds<Pixels>, rink: &RinkSpec, flipped: bool, view: ViewZoom) -> Self {
        let w = f32::from(bounds.size.width) - 2.0 * MARGIN_PX;
        let h = f32::from(bounds.size.height) - 2.0 * MARGIN_PX;
        let fit_scale = (w / rink.length_m).min(h / rink.width_m).max(0.1);
        let half_bounds = Vec2::new(f32::from(bounds.size.width) / 2.0, f32::from(bounds.size.height) / 2.0);
        let fit_center = Vec2::new(f32::from(bounds.origin.x), f32::from(bounds.origin.y)) + half_bounds;
        let mut t = RinkTransform {
            center: fit_center,
            scale: fit_scale,
            flip: if flipped { -1.0 } else { 1.0 },
            fit_center,
            fit_scale,
            half_bounds,
            half_rink: Vec2::new(rink.length_m / 2.0, rink.width_m / 2.0),
            perspective: None,
        };
        let view = t.clamped(view);
        t.scale = fit_scale * view.zoom;
        t.center = fit_center + view.pan;
        t
    }

    /// Fit the rink and its raised geometry using a perspective camera.
    pub fn fit_3d(bounds: Bounds<Pixels>, rink: &RinkSpec, flipped: bool, view: ViewZoom, camera: Camera3d) -> Self {
        let mut t = Self::fit(bounds, rink, flipped, ViewZoom::default());
        let perspective = Perspective { camera, distance: rink.length_m.hypot(rink.width_m) * 1.5 };
        let mut extent = Vec2::ZERO;
        for x in [-rink.half_length(), rink.half_length()] {
            for y in [-rink.half_width(), rink.half_width()] {
                for height in [-0.8, 3.0] {
                    extent = extent.max(perspective.project(Vec2::new(x, y), height).abs());
                }
            }
        }
        t.half_rink = extent;
        let usable = (t.half_bounds - Vec2::splat(MARGIN_PX)).max(Vec2::splat(1.0));
        t.fit_scale = (usable / extent).min_element().max(0.1);
        t.perspective = Some(perspective);
        let view = t.clamped(view);
        t.scale = t.fit_scale * view.zoom;
        t.center = t.fit_center + view.pan;
        t
    }

    pub fn is_3d(&self) -> bool {
        self.perspective.is_some()
    }

    /// Distance along the camera's viewing direction, for sorting geometry back to front.
    pub fn depth(&self, p: Vec2, height: f32) -> f32 {
        self.perspective.map_or(0.0, |camera| camera.depth(p * self.flip, height))
    }

    pub fn scale_at(&self, p: Vec2, height: f32) -> f32 {
        self.scale * self.perspective.map_or(1.0, |camera| camera.distance / camera.depth(p * self.flip, height))
    }

    pub fn to_screen_at(self, p: Vec2, height: f32) -> Point<Pixels> {
        let v = match self.perspective {
            Some(camera) => camera.project(p * self.flip, height),
            None => Vec2::new(p.x, -p.y) * self.flip,
        };
        let screen = self.center + v * self.scale;
        point(px(screen.x), px(screen.y))
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// `view` zoomed by `factor`, keeping the rink point under `at` where it is.
    pub fn zoomed(&self, view: ViewZoom, at: Point<Pixels>, factor: f32) -> ViewZoom {
        // Work from `view`, not this transform's own center and scale: several
        // gesture steps can land before the next paint.
        let view = self.clamped(view);
        let zoom = (view.zoom * factor).clamp(1.0, MAX_ZOOM);
        let at = Vec2::new(f32::from(at.x), f32::from(at.y));
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

    /// Rink meters (y up) to window pixels (y down).
    pub fn to_screen(self, p: Vec2) -> Point<Pixels> {
        let p = self.to_screen_vec(p);
        point(px(p.x), px(p.y))
    }

    pub fn to_screen_vec(self, p: Vec2) -> Vec2 {
        let s = self.to_screen_at(p, 0.0);
        Vec2::new(f32::from(s.x), f32::from(s.y))
    }

    pub fn to_rink(self, p: Point<Pixels>) -> Option<Vec2> {
        let screen = (Vec2::new(f32::from(p.x), f32::from(p.y)) - self.center) / self.scale;
        match self.perspective {
            Some(camera) => camera.unproject(screen).map(|p| p * self.flip),
            None => Some(Vec2::new(screen.x, -screen.y) * self.flip),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    fn bounds() -> Bounds<Pixels> {
        Bounds { origin: point(px(10.0), px(20.0)), size: size(px(1248.0), px(648.0)) }
    }

    fn fit(flipped: bool, view: ViewZoom) -> RinkTransform {
        RinkTransform::fit(bounds(), &RinkSpec::iihf(), flipped, view)
    }

    #[test]
    fn round_trips() {
        for flipped in [false, true] {
            let t = fit(flipped, ViewZoom::default());
            let p = Vec2::new(-12.5, 7.25);
            let back = t.to_rink(t.to_screen(p)).unwrap();
            assert!(back.distance(p) < 1e-3, "flipped={flipped}: {back}");
        }
    }

    #[test]
    fn home_attacks_right_and_plus_y_is_up() {
        let t = fit(false, ViewZoom::default());
        let o = t.to_screen_vec(Vec2::ZERO);
        assert!(t.to_screen_vec(Vec2::new(10.0, 0.0)).x > o.x);
        assert!(t.to_screen_vec(Vec2::new(0.0, 10.0)).y < o.y);
        let f = fit(true, ViewZoom::default());
        assert!(f.to_screen_vec(Vec2::new(10.0, 0.0)).x < o.x);
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
            assert!(z.to_rink(at).unwrap().distance(Vec2::new(20.0, -7.0)) < 1e-3, "flipped={flipped}");
        }
    }

    #[test]
    fn zoom_and_pan_stay_in_range() {
        let t = fit(false, ViewZoom::default());
        let at = point(px(300.0), px(300.0));
        assert_eq!(t.zoomed(ViewZoom::default(), at, 0.5), ViewZoom::default());
        assert_eq!(t.zoomed(ViewZoom::default(), at, 100.0).zoom, MAX_ZOOM);
        assert_eq!(t.panned(ViewZoom::default(), Vec2::new(50.0, 50.0)).pan, Vec2::ZERO);

        // Zoomed 2x, the rink plus margins is 2448 px wide in a 1248 px canvas: 600 px each way.
        let far = t.panned(ViewZoom { zoom: 2.0, pan: Vec2::ZERO }, Vec2::new(5000.0, 0.0));
        assert!((far.pan.x - 600.0).abs() < 1e-3);
    }

    #[test]
    fn perspective_round_trips_and_fits_at_every_camera_angle() {
        let rink = RinkSpec::iihf();
        for flipped in [false, true] {
            for yaw in [0.0_f32, 45.0, 90.0, 180.0, 275.0] {
                for elevation in [25.0_f32, 50.0, 80.0] {
                    let camera = Camera3d { yaw: yaw.to_radians(), elevation: elevation.to_radians() };
                    let t = RinkTransform::fit_3d(bounds(), &rink, flipped, ViewZoom::default(), camera);
                    for p in crate::paint::boards_outline(&rink).into_iter().chain([Vec2::ZERO, Vec2::new(10.0, -7.0)])
                    {
                        assert!(t.to_rink(t.to_screen(p)).unwrap().distance(p) < 0.001);
                        for height in [-0.65, 0.0, 2.5] {
                            let at = t.to_screen_at(p, height);
                            assert!(bounds().contains(&at), "camera={camera:?} point={p} height={height}: {at:?}");
                            assert!(t.depth(p, height) > 0.0);
                        }
                    }
                    let at = t.to_screen(Vec2::new(8.0, -4.0));
                    // At 6x both projected axes exceed the viewport, so pan clamping
                    // does not need to recenter a narrow end-on view.
                    let view = t.zoomed(ViewZoom::default(), at, 6.0);
                    let zoomed = RinkTransform::fit_3d(bounds(), &rink, flipped, view, camera);
                    assert!(zoomed.to_rink(at).unwrap().distance(Vec2::new(8.0, -4.0)) < 0.001);
                }
            }
        }
    }

    #[test]
    fn perspective_has_depth_and_rejects_rays_above_the_horizon() {
        let camera = Camera3d { yaw: 0.0, elevation: 30.0_f32.to_radians() };
        let t = RinkTransform::fit_3d(bounds(), &RinkSpec::iihf(), false, ViewZoom::default(), camera);
        assert!(t.scale_at(Vec2::new(0.0, -10.0), 0.0) > t.scale_at(Vec2::new(0.0, 10.0), 0.0));
        assert!(t.to_screen_at(Vec2::ZERO, 2.0).y < t.to_screen(Vec2::ZERO).y);
        assert!(t.to_rink(point(px(600.0), px(-10000.0))).is_none());
        let orbit = camera.orbited(Vec2::new(1_000_000.0, -1_000_000.0));
        assert!(orbit.yaw >= 0.0 && orbit.yaw < std::f32::consts::TAU);
        assert_eq!(orbit.elevation, 25.0_f32.to_radians());
    }
}
