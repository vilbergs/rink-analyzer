//! `rink`: read plays as facts and write position reviews from the command
//! line. Output is JSON so a Claude Code agent can read it directly.

mod args;

use std::path::{Path, PathBuf};

use anyhow::{anyhow, bail, Context, Result};
use rink_core::facts::{entity_facts, facts, round_half, FactSnapshot, Hypothetical};
use rink_core::review::{load_review, review_path, save_review, Review, Suggestion, DEFAULT_ZONE_RADIUS_M};
use rink_core::session::{self, OpenRequest};
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
                                             samples every MS, plus each event and puck pickup
  rink snapshot <play> <t_ms> [--entity ID]  facts at one moment (all players, or one)
  rink compare <play> --t MS --x X --y Y [--entity ID] [--facing DEG] [--from MS]
                                             dry run: actual vs. hypothetical spot, nothing saved
  rink suggest <play> --t MS --x X --y Y --principle TEXT --reason TEXT
               [--entity ID] [--from MS] [--facing DEG] [--radius M] [--detail TEXT] [--confidence 0..1]
  rink review <play>                         suggestions stored for a play
  rink unsuggest <play> <id>                 remove a suggestion
  rink upgrade <play>                        rewrite an older play file in the current format
  rink open <play> [--at MS] [--select ID]   show the play in the app window (reloads it if already
                                             open; starts the app if no window is running)
  rink current                               what the app window shows: play, playhead, selection

A suggestion is \"stale\" when the play's motion changed after it was written.

<play> is a path to a .rink.json file. The library folder is $RINK_REPLAY_LIBRARY
or the platform data dir. Coordinates are meters from center ice; +x is the home
team's attacking direction and +y is home's left. Positions in facts are rounded
to 0.5 m. --facing takes degrees; play files store facing in radians.";

fn main() {
    match run(std::env::args().skip(1).collect()) {
        Ok((out, Format::Pretty)) => println!("{}", serde_json::to_string_pretty(&out).expect("JSON values serialize")),
        Ok((out, Format::Compact)) => println!("{}", serde_json::to_string(&out).expect("JSON values serialize")),
        Err(e) => {
            eprintln!("error: {e:#}");
            std::process::exit(1);
        }
    }
}

enum Format {
    Pretty,
    Compact,
}

fn run(raw: Vec<String>) -> Result<(Value, Format)> {
    let mut args = Args::parse(raw)?;
    if args.switch("help") {
        println!("{USAGE}");
        std::process::exit(0);
    }
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
        "compare" => compare(&load(&args)?, &args),
        "suggest" => suggest(&mut args),
        "review" => show_review(&args),
        "unsuggest" => unsuggest(&args),
        "upgrade" => upgrade(&args),
        "open" => open(&args),
        "current" => current(),
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            std::process::exit(0);
        }
        other => bail!("unknown command {other:?}\n\n{USAGE}"),
    }?;
    args.finish()?;
    let format = if command == "timeline" && args.switch("compact") { Format::Compact } else { Format::Pretty };
    Ok((result, format))
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

/// The app writes its state file while it runs; check the process is still
/// alive in case it crashed without clearing it.
fn running_app() -> Option<session::AppState> {
    let state = session::read_state()?;
    let alive = std::process::Command::new("kill")
        .args(["-0", &state.pid.to_string()])
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(true);
    alive.then_some(state)
}

fn open(args: &Args) -> Result<Value> {
    let path = play_path(args)?;
    let path = std::fs::canonicalize(&path).with_context(|| format!("opening {}", path.display()))?;
    let play = io::load(&path)?;
    let at_ms: Option<u32> = args.parsed("at")?;
    let select = args.flag("select");
    if let Some(id) = &select {
        if play.entity(&EntityId::from(id.as_str())).is_none() {
            bail!("no entity {id:?} in {}", path.display());
        }
    }
    session::write_request(&OpenRequest {
        requested_at_ms: session::now_ms(),
        path: path.clone(),
        at_ms: at_ms.map(|t| t.min(play.duration_ms)),
        select: select.clone(),
    })?;

    let window = if running_app().is_some() {
        "updated"
    } else {
        // The app picks up the request on launch, including --at and --select.
        let app = std::env::current_exe()?.with_file_name("rink-replay");
        // Detached from our stdio, or the caller's shell waits for the window to close.
        let launched = std::process::Command::new(&app)
            .arg(&path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
        match launched {
            Ok(_) => "launched",
            Err(_) => {
                return Ok(json!({
                    "path": path,
                    "window": "not_running",
                    "hint": format!("Start the app: cargo run -p rink-app --release -- {}", path.display()),
                }))
            }
        }
    };
    Ok(json!({ "path": path, "window": window, "at_ms": at_ms, "select": select }))
}

fn current() -> Result<Value> {
    Ok(match running_app() {
        Some(state) => json!({ "running": true, "state": state }),
        None => json!({ "running": false }),
    })
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

/// Round to centimeters so JSON doesn't show f32 noise like 7.1399993.
fn cm(v: f32) -> f64 {
    (v as f64 * 100.0).round() / 100.0
}

fn coordinate_guide(play: &Play) -> Value {
    let r = &play.rink;
    let end_dot = r.end_dots()[3];
    let neutral_dot = r.neutral_dots()[3];
    let slot_x = r.goal_line_x() - 7.0;
    json!({
        "axes": format!(
            "Meters from center ice. x runs along the length ({:.1} to {:.1}); +x is the direction home attacks, \
             so home's defensive zone is x < -{:.2}. y runs across ({:.1} to {:.1}); +y is home's left. \
             Corners are rounded with a {:.1} m radius.",
            -r.half_length(), r.half_length(), r.blue_line_x(), -r.half_width(), r.half_width(), r.corner_radius_m
        ),
        "landmarks": {
            "goal_lines_x": [-cm(r.goal_line_x()), cm(r.goal_line_x())],
            "blue_lines_x": [-cm(r.blue_line_x()), cm(r.blue_line_x())],
            "end_zone_dots": format!("(±{:.1}, ±{:.1}); circles radius {:.1}; hash marks on the circle at y = ±{:.1}", end_dot.x, end_dot.y, r.faceoff_circle_radius_m, end_dot.y + r.faceoff_circle_radius_m),
            "neutral_zone_dots": format!("(±{:.1}, ±{:.1})", neutral_dot.x, neutral_dot.y),
            "nets": format!("goal mouth at x = ±{:.1}, y within ±{:.2}; goalie usually about 1 m out", r.goal_line_x(), r.goal_width_m / 2.0),
            "slot": format!("about (±{slot_x:.0}, 0)"),
            "points": format!("about (±{:.0}, ±{:.0}), just inside the blue line near the boards", r.blue_line_x() + 2.0, r.half_width() - 3.5),
            "behind_net": format!("about (±{:.0}, ±2)", r.goal_line_x() + 2.0),
        },
        "zones": "Each player's zone is from their own team's view: an away player in home's defensive zone is in its offensive zone.",
        "precision": "Positions were entered by hand. Judge shape and spacing, not centimeters.",
    })
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

/// Full facts for `me` and the carrier; everyone else as `id: [x, y]`.
fn compact(play: &Play, f: FactSnapshot) -> Value {
    let me = play.me().map(|e| e.id.clone());
    let mut focus = Vec::new();
    let mut others = serde_json::Map::new();
    for e in f.entities {
        if Some(&e.id) == me.as_ref() || Some(&e.id) == f.puck.carrier.as_ref() {
            focus.push(serde_json::to_value(e).expect("facts serialize"));
        } else {
            others.insert(e.id.to_string(), json!(e.pos));
        }
    }
    json!({ "t_ms": f.t_ms, "puck": f.puck, "focus": focus, "others": others })
}

fn timeline(play: &Play, args: &mut Args) -> Result<Value> {
    let interval: u32 = args.parsed("interval")?.unwrap_or(1000);
    let from: u32 = args.parsed("from")?.unwrap_or(0);
    let to: u32 = args.parsed::<u32>("to")?.unwrap_or(play.duration_ms).min(play.duration_ms);
    let compact_mode = args.switch("compact");
    if interval == 0 {
        bail!("--interval must be positive");
    }

    // Regular samples plus every event and every puck pickup at its exact time.
    let mut times: Vec<u32> = (from..=to).step_by(interval as usize).collect();
    times.extend(play.events.iter().map(|e| e.t_ms).filter(|t| (from..=to).contains(t)));
    times.extend(rink_core::sample::possessions(play).iter().map(|p| p.start_ms).filter(|t| (from..=to).contains(t)));
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
    Ok(json!({ "interval_ms": interval, "snapshots": snapshots }))
}

fn snapshot(play: &Play, args: &Args) -> Result<Value> {
    let t: u32 = args.positional(2).ok_or_else(|| anyhow!("missing <t_ms>"))?.parse().context("<t_ms>")?;
    let snap = sample(play, t.min(play.duration_ms));
    match args.flag("entity") {
        Some(id) => {
            let id = EntityId(id);
            let f = entity_facts(play, &snap, &id, None).ok_or_else(|| anyhow!("{id} isn't on the ice at {t} ms"))?;
            Ok(json!({ "t_ms": snap.t_ms, "puck": facts(play, &snap).puck, "entity": f }))
        }
        None => Ok(serde_json::to_value(facts(play, &snap))?),
    }
}

/// The player a command is about: `--entity`, or whoever is marked as me.
fn target_entity(play: &Play, args: &Args) -> Result<EntityId> {
    match args.flag("entity") {
        Some(id) => Ok(EntityId(id)),
        None => play.me().map(|e| e.id.clone()).ok_or_else(|| anyhow!("no --entity and no player marked as me")),
    }
}

fn hypothetical(args: &Args) -> Result<Hypothetical> {
    let x: f32 = args.parsed("x")?.ok_or_else(|| anyhow!("--x is required"))?;
    let y: f32 = args.parsed("y")?.ok_or_else(|| anyhow!("--y is required"))?;
    let facing = args.parsed::<f32>("facing")?.map(f32::to_radians);
    Ok(Hypothetical { pos: Vec2::new(x, y), facing })
}

/// Facts at `t_ms` where the player was and at the hypothetical spot.
fn comparison(play: &Play, entity: &EntityId, t_ms: u32, h: Hypothetical) -> Value {
    let snap = sample(play, t_ms);
    json!({
        "miss_m": rink_core::sample::position_at(play.track(entity), t_ms).map(|p| round_half(p.distance(h.pos))),
        "actual": entity_facts(play, &snap, entity, None),
        "suggested": entity_facts(play, &snap, entity, Some(h)),
    })
}

fn compare(play: &Play, args: &Args) -> Result<Value> {
    let t_ms: u32 = args.parsed("t")?.ok_or_else(|| anyhow!("--t is required"))?;
    let entity = target_entity(play, args)?;
    let h = hypothetical(args)?;
    let branch_from_ms = args.parsed("from")?.unwrap_or(t_ms.saturating_sub(1500));
    let probe = Suggestion {
        id: String::new(),
        entity: entity.clone(),
        t_ms,
        branch_from_ms,
        target: h.pos,
        facing: h.facing,
        zone_radius_m: DEFAULT_ZONE_RADIUS_M,
        principle: String::new(),
        reason: String::new(),
        detail: String::new(),
        confidence: None,
        play_hash: String::new(),
    };
    let mut out = comparison(play, &entity, t_ms, h);
    out["valid"] = json!(probe.check(play).err().map(|e| e.to_string()).unwrap_or_else(|| "ok".into()));
    Ok(out)
}

fn suggest(args: &mut Args) -> Result<Value> {
    let path = play_path(args)?;
    let play = io::load(&path)?;
    let t_ms: u32 = args.parsed("t")?.ok_or_else(|| anyhow!("--t is required"))?;
    let entity = target_entity(&play, args)?;
    let h = hypothetical(args)?;
    let suggestion = Suggestion {
        id: String::new(),
        entity: entity.clone(),
        t_ms,
        branch_from_ms: args.parsed("from")?.unwrap_or(t_ms.saturating_sub(1500)),
        target: h.pos,
        facing: h.facing,
        zone_radius_m: args.parsed("radius")?.unwrap_or(DEFAULT_ZONE_RADIUS_M),
        principle: args.flag("principle").ok_or_else(|| anyhow!("--principle is required"))?,
        reason: args.flag("reason").ok_or_else(|| anyhow!("--reason is required"))?,
        detail: args.flag("detail").unwrap_or_default(),
        confidence: args.parsed("confidence")?,
        play_hash: String::new(),
    };

    let mut review = load_review(&path)?.unwrap_or_else(|| Review::new(&play));
    let id = review.add(&play, suggestion.clone()).map_err(|e| anyhow!("suggestion rejected: {e}"))?;
    let saved = save_review(&path, &review)?;

    // Show what the move changes, everyone else held where they were.
    let mut out = comparison(&play, &entity, t_ms, h);
    out["id"] = json!(id);
    out["review"] = json!(saved);
    Ok(out)
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
            v["miss_m"] = json!(s.miss_distance(&play).map(round_half));
            v["valid"] = json!(s.check(&play).err().map(|e| e.to_string()).unwrap_or_else(|| "ok".into()));
            v["stale"] = json!(s.is_stale(&play));
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

fn upgrade(args: &Args) -> Result<Value> {
    let path = play_path(args)?;
    let before = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let old_version = serde_json::from_str::<Value>(&before)?.get("schema_version").and_then(Value::as_u64);
    let play = io::load(&path)?;
    io::save(&play, &path)?;
    Ok(json!({ "path": path, "from_version": old_version, "to_version": play.schema_version }))
}
