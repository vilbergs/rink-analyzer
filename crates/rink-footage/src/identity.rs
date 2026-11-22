//! Appearance ranking shared by hosted review and the frozen-data evaluator.
use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const MAX_DISTANCE: f32 = 0.35;

pub type Gear = [[u8; 5]; 3];

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct FrameKey {
    pub clip: Uuid,
    pub run: Uuid,
    pub source_ms: u64,
    pub track_id: u32,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct View {
    #[serde(flatten)]
    pub key: FrameKey,
    pub size: [u32; 2],
    pub xyxy: [f32; 4],
    pub gear: Option<Gear>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Example {
    pub player: Uuid,
    pub view: View,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Candidate {
    pub player: Uuid,
    pub distance: f32,
    pub reference: View,
}

pub fn gear_distance(left: Gear, right: Gear) -> f32 {
    [0.4, 0.4, 0.2]
        .into_iter()
        .enumerate()
        .map(|(zone, weight)| {
            weight * left[zone].iter().zip(right[zone]).map(|(a, b)| a.abs_diff(b) as f32).sum::<f32>() / 40.0
        })
        .sum()
}

pub fn rank(query: View, examples: &[Example], rejected: &[Uuid]) -> Vec<Candidate> {
    let Some(gear) = query.gear else { return vec![] };
    let mut players: BTreeMap<Uuid, Vec<(View, f32)>> = BTreeMap::new();
    for example in examples.iter().rev() {
        let view = example.view;
        if rejected.contains(&example.player) || view.key == query.key {
            continue;
        }
        let Some(sample) = view.gear else { continue };
        let views = players.entry(example.player).or_default();
        // Retain distinct reviewed views and cap the sample count per player.
        if views.len() >= 8
            || views.iter().any(|(other, _)| {
                other.key.clip == view.key.clip
                    && other.key.run == view.key.run
                    && other.key.source_ms.abs_diff(view.key.source_ms) < 500
            })
        {
            continue;
        }
        let mut distance = gear_distance(gear, sample);
        if near_continuous_view(query, view) {
            distance *= 0.85;
        }
        views.push((view, distance));
    }
    let mut ranking = players
        .into_iter()
        .map(|(player, mut views)| {
            views.sort_by(|a, b| a.1.total_cmp(&b.1));
            let (reference, distance) = views[0];
            Candidate { player, distance, reference }
        })
        .collect::<Vec<_>>();
    ranking.sort_by(|a, b| a.distance.total_cmp(&b.distance).then(a.player.cmp(&b.player)));
    ranking
}

fn near_continuous_view(a: View, b: View) -> bool {
    let [l, t, r, bottom] = a.xyxy;
    let [bl, bt, br, bb] = b.xyxy;
    a.key.clip == b.key.clip
        && a.key.run == b.key.run
        && a.key.track_id == b.key.track_id
        && a.key.source_ms.abs_diff(b.key.source_ms) <= 250
        && r.min(br) > l.max(bl)
        && bottom.min(bb) > t.max(bt)
}

pub fn clear_choice(ranking: &[Candidate]) -> Option<Uuid> {
    let best = ranking.first()?;
    (best.distance <= MAX_DISTANCE && ranking.get(1).is_none_or(|second| second.distance - best.distance >= 0.08))
        .then_some(best.player)
}

// The legacy Suggest match action compares one seed with each track in the
// clip. For roster evaluation, run that same matcher once per confirmed seed
// and invert the results so both matchers answer the same review question.
pub fn baseline(query: View, clip: &[View], examples: &[Example]) -> Option<Uuid> {
    let mut seeds = BTreeMap::new();
    for example in examples.iter().filter(|e| e.view.key.clip != query.key.clip) {
        if let Some(gear) = example.view.gear {
            seeds.entry(example.player).or_insert(gear);
        }
    }
    let tracks = baseline_tracks(clip);
    let mut choices = seeds
        .into_iter()
        .filter_map(|(player, seed)| {
            let (track, distance) = baseline_match(&tracks, seed)?;
            (track == query.key.track_id).then_some((player, distance))
        })
        .collect::<Vec<_>>();
    choices.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.cmp(&b.0)));
    let &(player, distance) = choices.first()?;
    choices.get(1).is_none_or(|second| second.1 - distance >= 0.08).then_some(player)
}

fn baseline_tracks(clip: &[View]) -> BTreeMap<u32, Vec<(u64, Gear)>> {
    let mut tracks: BTreeMap<u32, Vec<(u64, Gear)>> = BTreeMap::new();
    for view in clip {
        let Some(gear) = view.gear else { continue };
        let samples = tracks.entry(view.key.track_id).or_default();
        if samples.last().is_none_or(|(time, _)| view.key.source_ms.saturating_sub(*time) >= 250) {
            samples.push((view.key.source_ms, gear));
        }
    }
    tracks
}

fn baseline_match(tracks: &BTreeMap<u32, Vec<(u64, Gear)>>, seed: Gear) -> Option<(u32, f32)> {
    let mut scores = tracks
        .iter()
        .filter(|(_, samples)| samples.len() >= 3)
        .map(|(&id, samples)| {
            let mut best = [f32::INFINITY; 3];
            for (_, gear) in samples {
                let distance = gear_distance(seed, *gear);
                if distance < best[2] {
                    best[2] = distance;
                    best.sort_by(f32::total_cmp);
                }
            }
            (id, best.iter().sum::<f32>() / 3.0)
        })
        .collect::<Vec<_>>();
    scores.sort_by(|a, b| a.1.total_cmp(&b.1));
    let &(id, distance) = scores.first()?;
    (distance <= MAX_DISTANCE && scores.get(1).is_none_or(|second| second.1 - distance >= 0.08))
        .then_some((id, distance))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_confirmed_view_resolves_a_later_clip_and_undo_removes_it() {
        let player = Uuid::from_u128(1);
        let opponent = Uuid::from_u128(2);
        let make_view = |clip, color: usize, track_id| {
            let mut zone = [0; 5];
            zone[color] = 20;
            View {
                key: FrameKey { clip: Uuid::from_u128(clip), run: Uuid::from_u128(10 + clip), source_ms: 0, track_id },
                size: [640, 480],
                xyxy: [0.0, 0.0, 40.0, 60.0],
                gear: Some([zone; 3]),
            }
        };
        let query = make_view(3, 0, 4);
        let mut examples =
            vec![Example { player, view: make_view(1, 1, 1) }, Example { player: opponent, view: make_view(1, 2, 2) }];
        assert_eq!(clear_choice(&rank(query, &examples, &[])), None);
        examples.push(Example { player, view: make_view(2, 0, 7) });
        assert_eq!(clear_choice(&rank(query, &examples, &[])), Some(player));
        assert_eq!(clear_choice(&rank(query, &examples, &[player])), None);
        assert!(rank(View { gear: None, ..query }, &examples, &[]).is_empty());
        examples.pop();
        assert_eq!(clear_choice(&rank(query, &examples, &[])), None);
    }
}
