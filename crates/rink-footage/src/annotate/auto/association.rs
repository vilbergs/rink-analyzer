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
    gear: Option<[[f32; 5]; 3]>,
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
        let gear = (bounds.h >= 30.0 && bounds.w >= 12.0)
            .then(|| super::gear(image, bounds).map(|zone| zone.map(|n| f32::from(n) / 20.)));
        Self { bounds, jersey, gear }
    }
}

pub struct Track {
    pub id: u32,
    pub bounds: Bounds,
    pub age_since_match: u32,
    hits: u32,
    jersey: [f32; 3],
    gear: Option<[[f32; 5]; 3]>,
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
        let appearance: f32 = match (self.gear, detection.gear) {
            (Some(previous), Some(current)) => previous
                .iter()
                .zip(current)
                .zip([0.4, 0.4, 0.2])
                .map(|((a, b), weight)| a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum::<f32>() * weight)
                .sum(),
            _ => self.jersey.iter().zip(detection.jersey).map(|(a, b)| (a - b).abs()).sum(),
        };
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
        if let Some(gear) = detection.gear {
            let previous = self.gear.get_or_insert(gear);
            for (zone, sample) in previous.iter_mut().zip(gear) {
                for (old, new) in zone.iter_mut().zip(sample) {
                    *old = *old * 0.8 + new * 0.2;
                }
            }
        }
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
        let mut assigned_detections = vec![false; detections.len()];
        let costs =
            self.active.iter().map(|track| detections.iter().map(|d| track.cost(*d)).collect()).collect::<Vec<_>>();
        for (track, matched) in self.active.iter_mut().zip(assign(&costs)) {
            if let Some(index) = matched {
                track.update(detections[index]);
                assigned_detections[index] = true;
            } else {
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
                    gear: detection.gear,
                    velocity: [0.0; 2],
                });
            }
        }
    }

    pub fn visible(&self) -> impl Iterator<Item = &Track> {
        self.active.iter().filter(|track| track.hits >= 2 && track.age_since_match <= 3)
    }
}

/// Minimum-cost bipartite assignment (Hungarian algorithm). Extra columns let
/// each track miss a frame without consuming another skater's detection.
fn assign(costs: &[Vec<Option<f32>>]) -> Vec<Option<usize>> {
    let tracks = costs.len();
    let detections = costs.first().map_or(0, Vec::len);
    if detections == 0 {
        return vec![None; tracks];
    }
    let columns = detections + tracks;
    let mut track_price = vec![0.; tracks + 1];
    let mut column_price = vec![0.; columns + 1];
    let mut owner = vec![0; columns + 1];
    let mut parent = vec![0; columns + 1];
    for track in 1..=tracks {
        owner[0] = track;
        let mut col = 0;
        let mut distance = vec![f64::INFINITY; columns + 1];
        let mut visited = vec![false; columns + 1];
        loop {
            visited[col] = true;
            let row = owner[col];
            let mut next = 0;
            for j in 1..=columns {
                if visited[j] {
                    continue;
                }
                let cost = if j > detections { 1.31 } else { f64::from(costs[row - 1][j - 1].unwrap_or(100.)) };
                let reduced = cost - track_price[row] - column_price[j];
                if reduced < distance[j] {
                    distance[j] = reduced;
                    parent[j] = col;
                }
                if distance[j] < distance[next] {
                    next = j;
                }
            }
            let delta = distance[next];
            for j in 0..=columns {
                if visited[j] {
                    track_price[owner[j]] += delta;
                    column_price[j] -= delta;
                } else {
                    distance[j] -= delta;
                }
            }
            col = next;
            if owner[col] == 0 {
                break;
            }
        }
        while col != 0 {
            let prev = parent[col];
            owner[col] = owner[prev];
            col = prev;
        }
    }
    let mut matches = vec![None; tracks];
    for (col, &track) in owner.iter().enumerate().take(detections + 1).skip(1) {
        if track != 0 {
            matches[track - 1] = Some(col - 1);
        }
    }
    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detection(x: f32, jersey: [f32; 3]) -> Detection {
        Detection { bounds: Bounds { x, y: 50.0, w: 20.0, h: 40.0 }, jersey, gear: None }
    }

    #[test]
    fn crossing_tracks_share_the_available_detections() {
        let costs = [vec![Some(0.10), Some(0.15)], vec![Some(0.12), None]];
        assert_eq!(assign(&costs), [Some(1), Some(0)]);
        assert_eq!(assign(&[vec![None], vec![None]]), [None, None]);
    }

    #[test]
    fn gear_distinguishes_matching_jerseys_as_players_cross() {
        let skater = |x, helmet| {
            let mut player = detection(x, [0., 1., 0.]);
            let mut profile = [[0.; 5]; 3];
            profile[0][helmet] = 1.;
            profile[1][2] = 1.;
            profile[2][helmet] = 1.;
            player.gear = Some(profile);
            player
        };
        let mut tracks = Tracks::default();
        tracks.advance(vec![skater(10., 2), skater(18., 3)]);
        tracks.advance(vec![skater(16., 2), skater(12., 3)]);
        let positions = tracks.visible().map(|t| (t.id, t.bounds.x)).collect::<Vec<_>>();
        assert_eq!(positions, [(1, 16.), (2, 12.)]);
    }

    #[test]
    fn a_single_detection_cannot_confirm_two_tracks() {
        let mut tracker = Tracks::default();
        tracker.advance(vec![detection(10.0, [1.0, 0.0, 0.0]), detection(29.0, [0.0, 1.0, 0.0])]);
        tracker.advance(vec![detection(19.0, [1.0, 0.0, 0.0])]);
        assert_eq!(tracker.visible().map(|track| track.id).collect::<Vec<_>>(), [1]);
    }
}
