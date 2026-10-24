//! Upgrades of older play files, applied to the raw JSON before it is parsed,
//! so old files keep loading as the model changes.

use serde_json::{json, Value};

/// Bring a raw play document up to [`crate::model::SCHEMA_VERSION`].
pub fn migrate(value: &mut Value) {
    let version = value.get("schema_version").and_then(Value::as_u64).unwrap_or(1);
    if version < 2 {
        v1_to_v2(value);
    }
}

fn str_field<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn t_ms(v: &Value) -> u64 {
    v.get("t_ms").and_then(Value::as_u64).unwrap_or(0)
}

fn actor(ev: &Value, i: usize) -> Option<&str> {
    ev.get("actors")?.get(i)?.as_str()
}

struct V1Possession {
    entity: String,
    start: u64,
    end: Option<u64>,
}

/// v1 kept possession as `has_puck` on keyframes; v2 derives it from events.
/// Rebuild the v1 possession intervals, then add the events that produce the
/// same intervals under v2 rules:
/// - a pass to the new holder gets `arrive_ms` set to when they had it;
/// - any other pickup becomes a `puck_recovery`;
/// - a possession that ended without a pass, shot, turnover or someone else
///   picking up becomes a `turnover`.
fn v1_to_v2(value: &mut Value) {
    let goalies: Vec<String> = value["entities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| str_field(e, "kind") == "goalie")
        .map(|e| str_field(e, "id").to_string())
        .collect();
    let pucks: Vec<String> = value["entities"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|e| str_field(e, "kind") == "puck")
        .map(|e| str_field(e, "id").to_string())
        .collect();
    let mut events: Vec<Value> = value["events"].as_array().cloned().unwrap_or_default();
    let releases = |events: &[Value], who: &str, t: u64| {
        events.iter().any(|ev| {
            matches!(str_field(ev, "kind"), "pass" | "shot" | "turnover") && t_ms(ev) == t && actor(ev, 0) == Some(who)
        })
    };

    // v1 rule: has_puck starts possession; it ends at that entity's next
    // keyframe without it, its own pass/shot/turnover, or another pickup.
    let mut ps: Vec<V1Possession> = Vec::new();
    if let Some(tracks) = value["tracks"].as_object() {
        for (id, keys) in tracks.iter().filter(|(id, _)| !pucks.contains(id)) {
            let mut start = None;
            for k in keys.as_array().into_iter().flatten() {
                let has = k.get("has_puck").and_then(Value::as_bool).unwrap_or(false);
                match (start, has) {
                    (None, true) => start = Some(t_ms(k)),
                    (Some(s), false) => {
                        ps.push(V1Possession { entity: id.clone(), start: s, end: Some(t_ms(k)) });
                        start = None;
                    }
                    _ => {}
                }
            }
            if let Some(s) = start {
                ps.push(V1Possession { entity: id.clone(), start: s, end: None });
            }
        }
    }
    ps.sort_by_key(|p| p.start);
    for p in &mut ps {
        let release = events
            .iter()
            .filter(|ev| matches!(str_field(ev, "kind"), "pass" | "shot" | "turnover"))
            .filter(|ev| actor(ev, 0) == Some(p.entity.as_str()))
            .map(t_ms)
            .filter(|&t| t >= p.start && p.end.is_none_or(|e| t < e))
            .min();
        if release.is_some() {
            p.end = release;
        }
    }
    let starts: Vec<(u64, String)> = ps.iter().map(|p| (p.start, p.entity.clone())).collect();
    for p in &mut ps {
        let taken = starts
            .iter()
            .filter(|(s, id)| *id != p.entity && *s > p.start && p.end.is_none_or(|e| *s < e))
            .map(|(s, _)| *s)
            .min();
        if taken.is_some() {
            p.end = taken;
        }
    }
    ps.retain(|p| p.end.is_none_or(|e| e > p.start));

    let mut added = Vec::new();
    for (i, p) in ps.iter().enumerate() {
        let prev_start = if i > 0 { ps[i - 1].start } else { 0 };
        let pass_in = events
            .iter()
            .enumerate()
            .filter(|(_, ev)| str_field(ev, "kind") == "pass" && actor(ev, 1) == Some(p.entity.as_str()))
            .filter(|(_, ev)| t_ms(ev) <= p.start && (i == 0 || t_ms(ev) >= prev_start))
            .max_by_key(|(_, ev)| t_ms(ev))
            .map(|(idx, _)| idx);
        let already_gained = |events: &[Value]| {
            events.iter().any(|ev| {
                t_ms(ev) == p.start
                    && match str_field(ev, "kind") {
                        "puck_recovery" | "faceoff" => actor(ev, 0) == Some(p.entity.as_str()),
                        "turnover" => actor(ev, 1) == Some(p.entity.as_str()),
                        _ => false,
                    }
            })
        };
        match pass_in {
            Some(idx) => {
                if events[idx].get("arrive_ms").is_none() {
                    events[idx]["arrive_ms"] = json!(p.start);
                }
            }
            None if already_gained(&events) => {}
            None => added.push(json!({ "t_ms": p.start, "kind": "puck_recovery", "actors": [p.entity], "note": "" })),
        }
        if let Some(e) = p.end {
            let handed_on = starts.iter().any(|(s, id)| *s == e && *id != p.entity);
            if !handed_on && !releases(&events, &p.entity, e) {
                added.push(json!({
                    "t_ms": e,
                    "kind": "turnover",
                    "actors": [p.entity],
                    "note": "Lost the puck (converted from an older file).",
                }));
            }
        }
    }
    // In v1 a recovery without a has_puck keyframe was only a label. Keep a
    // goalie's as a save, so a rebound doesn't stick to the goalie's stick.
    for ev in events.iter_mut().filter(|ev| str_field(ev, "kind") == "puck_recovery") {
        let who = actor(ev, 0).unwrap_or_default().to_string();
        let gained = ps.iter().any(|p| p.entity == who && p.start == t_ms(ev));
        if !gained && goalies.contains(&who) {
            ev["kind"] = json!("save");
        }
    }
    events.extend(added);
    events.sort_by_key(t_ms);
    value["events"] = Value::Array(events);

    if let Some(tracks) = value["tracks"].as_object_mut() {
        for k in tracks.values_mut().filter_map(Value::as_array_mut).flatten() {
            if let Some(obj) = k.as_object_mut() {
                obj.remove("has_puck");
            }
        }
    }
    value["schema_version"] = json!(2);
}

#[cfg(test)]
mod tests {
    use crate::io::from_json;
    use crate::model::{EventKind, Role, TeamSide};
    use crate::rink::RinkSpec;
    use crate::sample::{possessions, sample};
    use crate::templates::{build, entity_id, Template};
    use crate::validate::validate;

    const BREAKOUT_V1: &str = include_str!("../fixtures/breakout-v1.rink.json");

    #[test]
    fn v1_breakout_keeps_its_possession() {
        let play = from_json(BREAKOUT_V1).unwrap();
        assert_eq!(play.schema_version, crate::model::SCHEMA_VERSION);
        assert_eq!(validate(&play), vec![]);
        let pass = play.events.iter().find(|e| e.kind == EventKind::Pass).unwrap();
        assert_eq!(pass.arrive_ms, Some(2000));

        let ld = entity_id(TeamSide::Home, Role::LD);
        let lw = entity_id(TeamSide::Home, Role::LW);
        assert_eq!(sample(&play, 1000).puck.carrier, Some(ld));
        assert_eq!(sample(&play, 1800).puck.carrier, None);
        assert_eq!(sample(&play, 2000).puck.carrier, Some(lw.clone()));
        assert_eq!(sample(&play, 5500).puck.carrier, Some(lw));

        // Same possession as the current template.
        let template = build(Template::Breakout, RinkSpec::iihf());
        assert_eq!(possessions(&play), possessions(&template));
        assert!(!crate::io::to_json(&play).contains("has_puck"));
    }

    #[test]
    fn v1_goalie_recovery_label_becomes_a_save() {
        let mut v: serde_json::Value = serde_json::from_str(BREAKOUT_V1).unwrap();
        let goalie = entity_id(TeamSide::Home, Role::G);
        v["events"].as_array_mut().unwrap().push(serde_json::json!({
            "t_ms": 500, "kind": "puck_recovery", "actors": [goalie], "note": ""
        }));
        let play = from_json(&v.to_string()).unwrap();
        assert!(play.events.iter().any(|e| e.kind == EventKind::Save && e.t_ms == 500));
        assert_eq!(sample(&play, 1000).puck.carrier, Some(entity_id(TeamSide::Home, Role::LD)));
    }

    #[test]
    fn v1_flag_pickup_and_drop_become_events() {
        let mut v: serde_json::Value = serde_json::from_str(BREAKOUT_V1).unwrap();
        v["events"] = serde_json::json!([]);
        // LD holds 0..1500 by flags alone; at 1500 the flag goes false.
        let play = from_json(&v.to_string()).unwrap();
        let kinds: Vec<(u32, EventKind)> = play.events.iter().map(|e| (e.t_ms, e.kind)).collect();
        assert!(kinds.contains(&(0, EventKind::PuckRecovery)));
        assert!(kinds.contains(&(1500, EventKind::Turnover)));
        assert!(kinds.contains(&(2000, EventKind::PuckRecovery)));
        assert_eq!(sample(&play, 1000).puck.carrier, Some(entity_id(TeamSide::Home, Role::LD)));
        assert_eq!(sample(&play, 1700).puck.carrier, None);
    }
}
