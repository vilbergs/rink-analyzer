use image::RgbImage;

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Bounds {
    pub fn is_valid(self) -> bool {
        [self.x, self.y, self.w, self.h].iter().all(|value| value.is_finite()) && self.w >= 5.0 && self.h >= 12.0
    }

    fn center(self) -> [f32; 2] {
        [self.x + self.w * 0.5, self.y + self.h * 0.5]
    }

    fn shift(self, [x, y]: [f32; 2], steps: f32) -> Self {
        Self { x: self.x + steps * x, y: self.y + steps * y, ..self }
    }

    fn overlap(self, other: Self) -> f32 {
        let width = (self.x + self.w).min(other.x + other.w) - self.x.max(other.x);
        let height = (self.y + self.h).min(other.y + other.h) - self.y.max(other.y);
        width.max(0.0) * height.max(0.0)
    }

    #[cfg(all(feature = "auto-tracking", target_os = "macos"))]
    pub fn duplicates(self, other: Self) -> bool {
        let overlap = self.overlap(other);
        self.iou(other) >= 0.45 || overlap >= 0.75 * (self.w * self.h).min(other.w * other.h)
    }

    fn iou(self, other: Self) -> f32 {
        let overlap = self.overlap(other);
        overlap / (self.w * self.h + other.w * other.h - overlap)
    }
}

#[derive(Clone, Copy)]
pub struct Detection {
    pub bounds: Bounds,
    jersey: [f32; 3],
}

impl Detection {
    pub fn new(image: &RgbImage, bounds: Bounds) -> Self {
        let mut jersey = [0.0; 3];
        for row in 0..4 {
            for col in 0..4 {
                let x = bounds.x + bounds.w * (0.32 + col as f32 * 0.12);
                let y = bounds.y + bounds.h * (0.27 + row as f32 * 0.075);
                let pixel = image.get_pixel(
                    x.clamp(0.0, (image.width() - 1) as f32) as u32,
                    y.clamp(0.0, (image.height() - 1) as f32) as u32,
                );
                let [r, g, b] = pixel.0.map(i32::from);
                let bin = if r - g > 24 && r - b > 24 {
                    0
                } else if r.max(g).max(b) < 105 {
                    1
                } else {
                    2
                };
                jersey[bin] += 1.0 / 16.0;
            }
        }
        Self { bounds, jersey }
    }
}

pub struct Track {
    pub id: u32,
    pub bounds: Bounds,
    pub age_since_match: u32,
    hits: u32,
    jersey: [f32; 3],
    velocity: [f32; 2],
}

impl Track {
    fn cost(&self, detection: Detection) -> Option<f32> {
        let predicted = self.bounds.shift(self.velocity, (self.age_since_match + 1).min(4) as f32);
        let [x, y] = predicted.center();
        let [nx, ny] = detection.bounds.center();
        let scale = predicted.h.max(detection.bounds.h);
        let distance = (x - nx).hypot(y - ny) / scale;
        let size = (predicted.h / detection.bounds.h).ln().abs();
        let appearance: f32 = self.jersey.iter().zip(detection.jersey).map(|(a, b)| (a - b).abs()).sum();
        let cost = distance
            + 0.30 * (1.0 - predicted.iou(detection.bounds))
            + 0.42 * appearance
            + 0.28 * size
            + 0.05 * self.age_since_match as f32;
        (distance < 0.85 && size < 0.85 && cost < 1.30).then_some(cost)
    }

    fn update(&mut self, detection: Detection) {
        let [x, y] = self.bounds.center();
        let [nx, ny] = detection.bounds.center();
        let elapsed = (self.age_since_match + 1) as f32;
        for (velocity, delta) in self.velocity.iter_mut().zip([nx - x, ny - y]) {
            *velocity = *velocity * 0.45 + delta / elapsed * 0.55;
        }
        for (old, new) in self.jersey.iter_mut().zip(detection.jersey) {
            *old = *old * 0.80 + new * 0.20;
        }
        self.age_since_match = 0;
        self.hits += 1;
        self.bounds = detection.bounds;
    }

    pub fn draw_bounds(&self) -> Bounds {
        self.bounds.shift(self.velocity, self.age_since_match as f32)
    }
}

#[derive(Default)]
pub struct Tracks {
    next_id: u32,
    active: Vec<Track>,
}

impl Tracks {
    pub fn advance(&mut self, mut detections: Vec<Detection>) {
        detections.sort_by(|a, b| a.bounds.x.total_cmp(&b.bounds.x));
        let mut candidates = Vec::new();
        for (track_index, track) in self.active.iter().enumerate() {
            for (detection_index, detection) in detections.iter().enumerate() {
                if let Some(cost) = track.cost(*detection) {
                    candidates.push((cost, track_index, detection_index));
                }
            }
        }
        candidates.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut assigned_tracks = vec![false; self.active.len()];
        let mut assigned_detections = vec![false; detections.len()];
        for (_, track, detection) in candidates {
            if !assigned_tracks[track] && !assigned_detections[detection] {
                self.active[track].update(detections[detection]);
                assigned_tracks[track] = true;
                assigned_detections[detection] = true;
            }
        }
        for (track, matched) in self.active.iter_mut().zip(assigned_tracks) {
            if !matched {
                track.age_since_match += 1;
            }
        }
        self.active.retain(|track| track.age_since_match <= if track.hits >= 2 { 10 } else { 2 });
        for (detection, matched) in detections.into_iter().zip(assigned_detections) {
            if !matched {
                self.next_id += 1;
                self.active.push(Track {
                    id: self.next_id,
                    bounds: detection.bounds,
                    age_since_match: 0,
                    hits: 1,
                    jersey: detection.jersey,
                    velocity: [0.0; 2],
                });
            }
        }
    }

    pub fn visible(&self) -> impl Iterator<Item = &Track> {
        self.active.iter().filter(|track| track.hits >= 2 && track.age_since_match <= 3)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detection(x: f32, jersey: [f32; 3]) -> Detection {
        Detection { bounds: Bounds { x, y: 50.0, w: 20.0, h: 40.0 }, jersey }
    }

    #[test]
    fn a_single_detection_cannot_confirm_two_tracks() {
        let mut tracker = Tracks::default();
        tracker.advance(vec![detection(10.0, [1.0, 0.0, 0.0]), detection(29.0, [0.0, 1.0, 0.0])]);
        tracker.advance(vec![detection(19.0, [1.0, 0.0, 0.0])]);
        assert_eq!(tracker.visible().map(|track| track.id).collect::<Vec<_>>(), [1]);
    }
}
