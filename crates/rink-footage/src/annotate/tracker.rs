use image::RgbImage;

const SAMPLE_RADIUS: i32 = 12;
const SIDE: usize = 9;
const PIXELS: usize = SIDE * SIDE;
const SEARCH_RADIUS: i32 = 30;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Point(pub i32, pub i32);

impl Point {
    pub fn distance(self, other: Self) -> f32 {
        let dx = (self.0 - other.0) as f32;
        let dy = (self.1 - other.1) as f32;
        dx.hypot(dy)
    }
}

type Patch = [[u8; 3]; PIXELS];

fn fits(image: &RgbImage, Point(x, y): Point) -> bool {
    let limit = SAMPLE_RADIUS;
    x >= limit && y >= limit && x + limit < image.width() as i32 && y + limit < image.height() as i32
}

fn patch(image: &RgbImage, Point(x, y): Point) -> Patch {
    let mut patch = [[0; 3]; PIXELS];
    for (index, pixel) in patch.iter_mut().enumerate() {
        let dx = (index % SIDE) as i32 * 3 - SAMPLE_RADIUS;
        let dy = (index / SIDE) as i32 * 3 - SAMPLE_RADIUS;
        *pixel = image.get_pixel((x + dx) as u32, (y + dy) as u32).0;
    }
    patch
}

fn appearance_error(left: &Patch, right: &Patch) -> f32 {
    let total: u32 =
        left.iter().zip(right).map(|(a, b)| a.iter().zip(b).map(|(a, b)| u32::from(a.abs_diff(*b))).sum::<u32>()).sum();
    total as f32 / (PIXELS * 3) as f32
}

#[derive(Clone, Copy)]
pub struct Prediction {
    pub center: Point,
    pub weak_match: bool,
}

pub struct Tracker {
    seed: Patch,
    recent: Patch,
    center: Point,
    velocity: Point,
}

impl Tracker {
    pub fn seed(image: &RgbImage, center: Point) -> Option<Self> {
        if !fits(image, center) {
            return None;
        }
        let seed = patch(image, center);
        Some(Self { seed, recent: seed, center, velocity: Point(0, 0) })
    }

    pub fn advance(&mut self, image: &RgbImage) -> Prediction {
        let predicted = Point(self.center.0 + self.velocity.0, self.center.1 + self.velocity.1);
        let mut best = (self.center, f32::INFINITY, f32::INFINITY);
        let consider = |candidate: Point, best: &mut (Point, f32, f32)| {
            if !fits(image, candidate) {
                return;
            }
            let sample = patch(image, candidate);
            let difference =
                appearance_error(&self.recent, &sample) * 0.8 + appearance_error(&self.seed, &sample) * 0.2;
            let cost = difference + 0.15 * candidate.distance(predicted);
            if cost < best.1 {
                *best = (candidate, cost, difference);
            }
        };
        for dy in (-SEARCH_RADIUS..=SEARCH_RADIUS).step_by(3) {
            for dx in (-SEARCH_RADIUS..=SEARCH_RADIUS).step_by(3) {
                consider(Point(predicted.0 + dx, predicted.1 + dy), &mut best);
            }
        }
        let coarse = best.0;
        for dy in -2..=2 {
            for dx in -2..=2 {
                consider(Point(coarse.0 + dx, coarse.1 + dy), &mut best);
            }
        }
        let weak_match = best.2 > 24.0;
        if weak_match {
            self.velocity = Point(self.velocity.0 / 2, self.velocity.1 / 2);
        } else {
            let center = best.0;
            self.velocity = Point(
                (self.velocity.0 + center.0 - self.center.0) / 2,
                (self.velocity.1 + center.1 - self.center.1) / 2,
            );
            self.center = center;
            let observed = patch(image, center);
            for (stored, new) in self.recent.iter_mut().zip(observed) {
                for (channel, observation) in stored.iter_mut().zip(new) {
                    *channel = ((3 * u16::from(*channel) + u16::from(observation)) / 4) as u8;
                }
            }
        }
        Prediction { center: self.center, weak_match }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgb;

    #[test]
    fn follows_a_shifted_patch_without_another_seed() {
        let make_image = |center: Point| {
            RgbImage::from_fn(96, 96, |x, y| {
                let dx = x as i32 - center.0;
                let dy = y as i32 - center.1;
                if dx.abs() <= 12 && dy.abs() <= 12 {
                    Rgb([(dx + 12) as u8 * 8, (dy + 12) as u8 * 8, 50])
                } else {
                    Rgb([240; 3])
                }
            })
        };
        let initial = Point(33, 41);
        let target = Point(43, 36);
        let mut tracker = Tracker::seed(&make_image(initial), initial).unwrap();
        let prediction = tracker.advance(&make_image(target));
        assert!(!prediction.weak_match);
        assert!(prediction.center.distance(target) <= 1.0);
    }
}
