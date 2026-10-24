//! Maps rink meters to screen pixels and back. Drawing and hit-testing both go
//! through one `RinkTransform`, so what you click is what you see.

use gpui::{point, px, Bounds, Pixels, Point};
use rink_core::{RinkSpec, Vec2};

/// Screen space kept free around the boards, in pixels.
const MARGIN_PX: f32 = 24.0;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RinkTransform {
    center: Vec2,
    /// Pixels per meter.
    scale: f32,
    /// +1 shows home attacking right; -1 rotates the view 180 degrees.
    flip: f32,
}

impl RinkTransform {
    pub fn fit(bounds: Bounds<Pixels>, rink: &RinkSpec, flipped: bool) -> Self {
        let w = f32::from(bounds.size.width) - 2.0 * MARGIN_PX;
        let h = f32::from(bounds.size.height) - 2.0 * MARGIN_PX;
        let scale = (w / rink.length_m).min(h / rink.width_m).max(0.1);
        let center = Vec2::new(
            f32::from(bounds.origin.x) + f32::from(bounds.size.width) / 2.0,
            f32::from(bounds.origin.y) + f32::from(bounds.size.height) / 2.0,
        );
        RinkTransform { center, scale, flip: if flipped { -1.0 } else { 1.0 } }
    }

    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Rink meters (y up) to window pixels (y down).
    pub fn to_screen(self, p: Vec2) -> Point<Pixels> {
        let s = self.to_screen_vec(p);
        point(px(s.x), px(s.y))
    }

    pub fn to_screen_vec(self, p: Vec2) -> Vec2 {
        Vec2::new(self.center.x + p.x * self.scale * self.flip, self.center.y - p.y * self.scale * self.flip)
    }

    pub fn to_rink(self, p: Point<Pixels>) -> Vec2 {
        let x = (f32::from(p.x) - self.center.x) / (self.scale * self.flip);
        let y = -(f32::from(p.y) - self.center.y) / (self.scale * self.flip);
        Vec2::new(x, y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::size;

    fn bounds() -> Bounds<Pixels> {
        Bounds { origin: point(px(10.0), px(20.0)), size: size(px(1248.0), px(648.0)) }
    }

    #[test]
    fn round_trips() {
        for flipped in [false, true] {
            let t = RinkTransform::fit(bounds(), &RinkSpec::iihf(), flipped);
            let p = Vec2::new(-12.5, 7.25);
            let back = t.to_rink(t.to_screen(p));
            assert!(back.distance(p) < 1e-3, "flipped={flipped}: {back}");
        }
    }

    #[test]
    fn home_attacks_right_and_plus_y_is_up() {
        let t = RinkTransform::fit(bounds(), &RinkSpec::iihf(), false);
        let o = t.to_screen_vec(Vec2::ZERO);
        assert!(t.to_screen_vec(Vec2::new(10.0, 0.0)).x > o.x);
        assert!(t.to_screen_vec(Vec2::new(0.0, 10.0)).y < o.y);
        let f = RinkTransform::fit(bounds(), &RinkSpec::iihf(), true);
        assert!(f.to_screen_vec(Vec2::new(10.0, 0.0)).x < o.x);
    }

    #[test]
    fn fits_rink_inside_bounds() {
        let t = RinkTransform::fit(bounds(), &RinkSpec::iihf(), false);
        // 1200 x 600 usable pixels for a 60 x 30 m rink = 20 px per meter.
        assert!((t.scale() - 20.0).abs() < 1e-3);
    }
}
