//! `rink`: read plays as facts and write position reviews from the command
//! line. Output is JSON so a Claude Code agent can read it directly.

mod args;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use rink_core::facts::{entity_facts, facts, FactSnapshot};
use rink_core::review::{load_review, review_path, save_review, Review, Suggestion, DEFAULT_ZONE_RADIUS_M};
use rink_core::templates::{build, Template};
use rink_core::{io, sample, validate, EntityId, Play, RinkSpec, Vec2};
use serde_json::{json, Value};

use crate::args::Args;

const USAGE: &str = "\
rink: read Rink Replay plays and write position reviews (JSON output)

USAGE:
  rink list                                  plays in the library folder
  rink new <faceoff|dzone|breakout> [--out FILE]
  rink check <play>                          validation issues
  rink summary <play>                        roster, events, decision points, notes
  rink timeline <play> [--interval MS] [--from MS] [--to MS] [--compact]
  rink snapshot <play> <t_ms>                every player's facts at one moment
  rink suggest <play> --t MS --x X --y Y --principle TEXT --reason TEXT
               [--entity ID] [--from MS] [--radius M] [--detail TEXT] [--confidence 0..1]
  rink review <play>                         suggestions stored for a play
  rink unsuggest <play> <id>                 remove a suggestion

<play> is a path to a .rink.json file. The library folder is $RINK_REPLAY_LIBRARY
or the platform data dir. Coordinates are meters from center ice; +x is the home
team's attacking direction and +y is home's left.";

fn main() {
    match run(std::env::args().skip(1).collect()) {
        Ok(out) => println!("{}", serde_json::to_string_pretty(&out).expect("JSON values serialize")),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
    }
}

fn run(raw: Vec<String>) -> Result<Value> {
    let mut args = Args::parse(raw)?;
    let Some(command) = args.positional(0) else {
        println!("{USAGE}");
        std::process::exit(0);
    };
    let result = match command.as_str() {
        "list" => list(),
        "new" => new_play(&mut args),
        "check" => check(&load(&args)?),
        "summary" => {
            let path = play_path(&args)?;
            summary(&io::load(&path)?, &path)
        }
        "timeline" => timeline(&load(&args)?, &mut args),
        "snapshot" => snapshot(&load(&args)?, &args),
        "suggest" => suggest(&mut args),
        "review" => show_review(&args),
        "unsuggest" => unsuggest(&args),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            std::process::exit(0);
        }
        other => bail!("unknown command {other:?}\n\n{USAGE}"),
    }?;
    args.finish()?;
    Ok(result)
}

fn play_path(args: &Args) -> Result<PathBuf> {
    args.positional(1).map(PathBuf::from).ok_or_else(|| anyhow!("missing <play> path"))
}

fn load(args: &Args) -> Result<Play> {
    let path = play_path(args)?;
    Ok(io::load(&path)?)
}

fn list() -> Result<Value> {
    let dir = io::library_dir();
    let plays: Vec<Value> = io::list_play_files(&dir)
        .into_iter()
        .map(|path| match io::load(&path) {
            Ok(p) => json!({
                "path": path,
                "title": p.title,
                "duration_ms": p.duration_ms,
                "rink": p.rink.name,
                "decision_points": p.events.iter().filter(|e| e.kind == rink_core::EventKind::DecisionPoint).count(),
                "has_review": review_path(&path).exists(),
            }),
            Err(e) => json!({ "path": path, "error": e.to_string() }),
        })
        .collect();
    Ok(json!({ "library": dir, "plays": plays }))
}

fn new_play(args: &mut Args) -> Result<Value> {
    let template = match args.positional(1).as_deref() {
        Some("faceoff") => Template::Faceoff,
        Some("dzone") => Template::DZoneSet,
        Some("breakout") => Template::Breakout,
        other => bail!("unknown template {other:?}; use faceoff, dzone or breakout"),
    };
    let play = build(template, RinkSpec::iihf());
    let path = match args.flag("out") {
        Some(p) => PathBuf::from(p),
        None => {
            let dir = io::library_dir();
            std::fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
            dir.join(io::file_name(&play))
        }
    };
    io::save(&play, &path)?;
    Ok(json!({ "path": path, "id": play.id }))
}

fn check(play: &Play) -> Result<Value> {
    let issues: Vec<String> = validate(play).iter().map(ToString::to_string).collect();
    Ok(json!({ "ok": issues.is_empty(), "issues": issues }))
}

fn coordinate_guide(play: &Play) -> String {
    format!(
        "Meters from center ice. x runs along the length ({:.1} to {:.1}); +x is the direction the home team \
         attacks. y runs across ({:.1} to {:.1}); +y is home's left. Goal lines at x = ±{:.2}, blue lines at \
         x = ±{:.2}. Positions were entered by hand: judge shape and spacing, not centimeters.",
        -play.rink.half_length(),
        play.rink.half_length(),
        -play.rink.half_width(),
        play.rink.half_width(),
        play.rink.goal_line_x(),
        play.rink.blue_line_x(),
    )
}

fn summary(play: &Play, path: &Path) -> Result<Value> {
    let roster: Vec<Value> = play
        .entities
        .iter()
        .map(|e| {
            json!({
                "id": e.id,
                "kind": e.kind,
                "team": e.team,
                "role": e.role,
                "number": e.number,
                "is_me": e.is_me,
                "keyframes": play.track(&e.id).len(),
            })
        })
        .collect();
    Ok(json!({
        "path": path,
        "title": play.title,
        "duration_ms": play.duration_ms,
        "rink": play.rink.name,
        "coordinates": coordinate_guide(play),
        "me": play.me().map(|e| &e.id),
        "teams": play.teams,
        "roster": roster,
        "events": play.events,
        "decision_points": play.events.iter().filter(|e| e.kind == rink_core::EventKind::DecisionPoint).collect::<Vec<_>>(),
        "notes": play.notes,
        "review": review_path(path),
    }))
}

fn compact(play: &Play, f: FactSnapshot) -> Value {
    let me = play.me().map(|e| e.id.clone());
    let entities: Vec<Value> = f
        .entities
        .into_iter()
        .map(|e| {
            if Some(&e.id) == me.as_ref() || Some(&e.id) == f.puck.carrier.as_ref() {
                serde_json::to_value(e).expect("facts serialize")
            } else {
                json!({ "id": e.id, "pos": e.pos })
            }
        })
        .collect();
    json!({ "t_ms": f.t_ms, "puck": f.puck, "entities": entities })
}

fn timeline(play: &Play, args: &mut Args) -> Result<Value> {
    let interval: u32 = args.parsed("interval")?.unwrap_or(1000);
    let from: u32 = args.parsed("from")?.unwrap_or(0);
    let to: u32 = args.parsed::<u32>("to")?.unwrap_or(play.duration_ms).min(play.duration_ms);
    let compact_mode = args.switch("compact");
    if interval == 0 {
        bail!("--interval must be positive");
    }

    // Regular samples plus every event's exact time.
    let mut times: Vec<u32> = (from..=to).step_by(interval as usize).collect();
    times.extend(play.events.iter().map(|e| e.t_ms).filter(|t| (from..=to).contains(t)));
    times.sort_unstable();
    times.dedup();

    let mut prev = None;
    let snapshots: Vec<Value> = times
        .iter()
        .map(|&t| {
            let f = facts(play, &sample(play, t));
            let mut v = if compact_mode { compact(play, f) } else { serde_json::to_value(f).expect("facts serialize") };
            let events: Vec<&rink_core::Event> =
                play.events.iter().filter(|e| e.t_ms <= t && prev.is_none_or(|p| e.t_ms > p)).collect();
            if !events.is_empty() {
                v["events_since_last"] = serde_json::to_value(events).expect("events serialize");
            }
            prev = Some(t);
            v
        })
        .collect();
    Ok(json!({ "coordinates": coordinate_guide(play), "interval_ms": interval, "snapshots": snapshots }))
}

fn snapshot(play: &Play, args: &Args) -> Result<Value> {
    let t: u32 = args.positional(2).ok_or_else(|| anyhow!("missing <t_ms>"))?.parse().context("<t_ms>")?;
    Ok(serde_json::to_value(facts(play, &sample(play, t.min(play.duration_ms))))?)
}

fn suggest(args: &mut Args) -> Result<Value> {
    let path = play_path(args)?;
    let play = io::load(&path)?;
    let t_ms: u32 = args.parsed("t")?.ok_or_else(|| anyhow!("--t is required"))?;
    let x: f32 = args.parsed("x")?.ok_or_else(|| anyhow!("--x is required"))?;
    let y: f32 = args.parsed("y")?.ok_or_else(|| anyhow!("--y is required"))?;
    let entity = match args.flag("entity") {
        Some(id) => EntityId(id),
        None => play.me().map(|e| e.id.clone()).ok_or_else(|| anyhow!("no --entity and no player marked as me"))?,
    };
    let suggestion = Suggestion {
        id: String::new(),
        entity: entity.clone(),
        t_ms,
        branch_from_ms: args.parsed("from")?.unwrap_or(t_ms.saturating_sub(1500)),
        target: Vec2::new(x, y),
        zone_radius_m: args.parsed("radius")?.unwrap_or(DEFAULT_ZONE_RADIUS_M),
        principle: args.flag("principle").ok_or_else(|| anyhow!("--principle is required"))?,
        reason: args.flag("reason").ok_or_else(|| anyhow!("--reason is required"))?,
        detail: args.flag("detail").unwrap_or_default(),
        confidence: args.parsed("confidence")?,
    };

    let mut review = load_review(&path)?.unwrap_or_else(|| Review::new(&play));
    let id = review.add(&play, suggestion.clone()).map_err(|e| anyhow!("suggestion rejected: {e}"))?;
    let saved = save_review(&path, &review)?;

    // Show what the move changes, everyone else held where they were.
    let snap = sample(&play, t_ms);
    Ok(json!({
        "id": id,
        "review": saved,
        "miss_m": suggestion.miss_distance(&play).map(rink_core::facts::round_half),
        "actual": entity_facts(&play, &snap, &entity, None),
        "suggested": entity_facts(&play, &snap, &entity, Some(suggestion.target)),
    }))
}

fn show_review(args: &Args) -> Result<Value> {
    let path = play_path(args)?;
    let play = io::load(&path)?;
    let review = load_review(&path)?.unwrap_or_else(|| Review::new(&play));
    let items: Vec<Value> = review
        .suggestions
        .iter()
        .map(|s| {
            let mut v = serde_json::to_value(s).expect("suggestions serialize");
            v["miss_m"] = json!(s.miss_distance(&play).map(rink_core::facts::round_half));
            v["valid"] = json!(s.check(&play).err().map(|e| e.to_string()).unwrap_or_else(|| "ok".into()));
            v
        })
        .collect();
    Ok(json!({ "review": review_path(&path), "suggestions": items }))
}

fn unsuggest(args: &Args) -> Result<Value> {
    let path = play_path(args)?;
    let id = args.positional(2).ok_or_else(|| anyhow!("missing suggestion <id>"))?;
    let Some(mut review) = load_review(&path)? else { bail!("{} has no review yet", path.display()) };
    if !review.remove(&id) {
        bail!("no suggestion {id}");
    }
    save_review(&path, &review)?;
    Ok(json!({ "removed": id, "remaining": review.suggestions.len() }))
}
