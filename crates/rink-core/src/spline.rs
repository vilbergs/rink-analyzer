//! Centripetal Catmull-Rom interpolation. Centripetal (alpha = 0.5) avoids the
//! cusps and self-intersecting loops uniform Catmull-Rom makes when keyframes
//! are unevenly spaced, so skater paths curve without anything to tune.

use std::f32::consts::{PI, TAU};

use glam::Vec2;

const ALPHA: f32 = 0.5;
const EPS: f32 = 1e-4;

/// Point on the centripetal Catmull-Rom segment from `p1` to `p2` at `u` in
/// [0, 1], with `p0` and `p3` as the neighbouring control points.
pub fn centripetal_catmull_rom(p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, u: f32) -> Vec2 {
    let knot = |a: Vec2, b: Vec2| a.distance(b).powf(ALPHA).max(EPS);
    let t0 = 0.0;
    let t1 = t0 + knot(p0, p1);
    let t2 = t1 + knot(p1, p2);
    let t3 = t2 + knot(p2, p3);

    if p1.distance_squared(p2) < EPS * EPS {
        return p1;
    }

    let t = t1 + u.clamp(0.0, 1.0) * (t2 - t1);
    let lerp = |a: Vec2, b: Vec2, ta: f32, tb: f32| a * ((tb - t) / (tb - ta)) + b * ((t - ta) / (tb - ta));

    // Barry-Goldman pyramid.
    let a1 = lerp(p0, p1, t0, t1);
    let a2 = lerp(p1, p2, t1, t2);
    let a3 = lerp(p2, p3, t2, t3);
    let b1 = lerp(a1, a2, t0, t2);
    let b2 = lerp(a2, a3, t1, t3);
    lerp(b1, b2, t1, t2)
}

/// Point on a spline through `points` (at least one), segment `i` (from
/// `points[i]` to `points[i + 1]`) at `u`. Ends get mirrored phantom points so
/// the first and last segments still have neighbours.
pub fn sample_segment(points: &[Vec2], i: usize, u: f32) -> Vec2 {
    let n = points.len();
    if n == 1 {
        return points[0];
    }
    let p1 = points[i];
    let p2 = points[i + 1];
    let p0 = if i == 0 { p1 + (p1 - p2) } else { points[i - 1] };
    let p3 = if i + 2 < n { points[i + 2] } else { p2 + (p2 - p1) };
    centripetal_catmull_rom(p0, p1, p2, p3, u)
}

/// Point on the cubic Hermite curve from `p0` (velocity `v0`) to `p1`
/// (velocity `v1`), `dt_s` seconds apart, at `u` in [0, 1]. Used between
/// keyframes that know their velocity, so speed and direction carry across.
pub fn hermite(p0: Vec2, v0: Vec2, p1: Vec2, v1: Vec2, dt_s: f32, u: f32) -> Vec2 {
    let u = u.clamp(0.0, 1.0);
    let (u2, u3) = (u * u, u * u * u);
    let h00 = 2.0 * u3 - 3.0 * u2 + 1.0;
    let h10 = u3 - 2.0 * u2 + u;
    let h01 = -2.0 * u3 + 3.0 * u2;
    let h11 = u3 - u2;
    p0 * h00 + v0 * (h10 * dt_s) + p1 * h01 + v1 * (h11 * dt_s)
}

/// Velocity (per second) along [`hermite`] at `u`.
pub fn hermite_velocity(p0: Vec2, v0: Vec2, p1: Vec2, v1: Vec2, dt_s: f32, u: f32) -> Vec2 {
    if dt_s <= 0.0 {
        return v0;
    }
    let u = u.clamp(0.0, 1.0);
    let u2 = u * u;
    let d00 = 6.0 * u2 - 6.0 * u;
    let d10 = 3.0 * u2 - 4.0 * u + 1.0;
    let d01 = -6.0 * u2 + 6.0 * u;
    let d11 = 3.0 * u2 - 2.0 * u;
    (p0 * d00 + p1 * d01) / dt_s + v0 * d10 + v1 * d11
}

/// Wrap an angle into (-PI, PI].
pub fn wrap_angle(a: f32) -> f32 {
    let mut a = a % TAU;
    if a <= -PI {
        a += TAU;
    } else if a > PI {
        a -= TAU;
    }
    a
}

/// Interpolate between two headings along the shorter arc.
pub fn lerp_angle(a: f32, b: f32, u: f32) -> f32 {
    wrap_angle(a + wrap_angle(b - a) * u)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hits_endpoints() {
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(5.0, 2.0), Vec2::new(10.0, 0.0)];
        assert!(sample_segment(&pts, 0, 0.0).distance(pts[0]) < 1e-4);
        assert!(sample_segment(&pts, 0, 1.0).distance(pts[1]) < 1e-4);
        assert!(sample_segment(&pts, 1, 1.0).distance(pts[2]) < 1e-4);
    }

    #[test]
    fn straight_line_stays_straight() {
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)];
        let mid = sample_segment(&pts, 0, 0.5);
        assert!(mid.y.abs() < 1e-4);
        assert!((mid.x - 5.0).abs() < 1e-3);
    }

    #[test]
    fn duplicate_points_do_not_nan() {
        let pts = [Vec2::new(1.0, 1.0), Vec2::new(1.0, 1.0), Vec2::new(4.0, 1.0)];
        for i in 0..2 {
            for s in 0..=10 {
                let p = sample_segment(&pts, i, s as f32 / 10.0);
                assert!(p.is_finite(), "segment {i} u {s}: {p}");
            }
        }
    }

    #[test]
    fn uneven_spacing_does_not_loop() {
        // A tight cluster then a long jump: uniform Catmull-Rom loops here.
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(0.2, 0.0), Vec2::new(0.3, 0.1), Vec2::new(20.0, 0.0)];
        let mut prev_x = f32::MIN;
        for s in 0..=50 {
            let p = sample_segment(&pts, 2, s as f32 / 50.0);
            assert!(p.x >= prev_x - 0.05, "x went backwards at step {s}");
            prev_x = p.x;
        }
    }

    #[test]
    fn hermite_matches_its_ends() {
        let (p0, v0, p1, v1) = (Vec2::new(0.0, 0.0), Vec2::new(6.0, 0.0), Vec2::new(5.0, 3.0), Vec2::new(0.0, 6.0));
        assert!(hermite(p0, v0, p1, v1, 1.0, 0.0).distance(p0) < 1e-5);
        assert!(hermite(p0, v0, p1, v1, 1.0, 1.0).distance(p1) < 1e-5);
        assert!(hermite_velocity(p0, v0, p1, v1, 1.0, 0.0).distance(v0) < 1e-4);
        assert!(hermite_velocity(p0, v0, p1, v1, 1.0, 1.0).distance(v1) < 1e-4);
    }

    #[test]
    fn angle_takes_short_way() {
        let a = 170f32.to_radians();
        let b = -170f32.to_radians();
        let mid = lerp_angle(a, b, 0.5);
        assert!((mid.abs() - PI).abs() < 1e-4);
    }
}
