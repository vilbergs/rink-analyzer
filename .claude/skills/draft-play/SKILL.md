---
name: draft-play
description: Draft a new Rink Replay play (.rink.json) from the user's description of a hockey situation, or change an existing one, and show it in the app window. Use when the user describes a play, shift or scenario and wants it on the rink ("I'm LW, our D rimmed it and their D pinched..."), or asks to change, load or show a play or an ideal version of one.
---

# Draft a play from a description

You turn a described situation into a play file the user opens in Rink Replay and fixes by
dragging. A rough draft that gets the shape right beats a precise one that took an hour. The user
retraces the details; your job is the starting point.

## Running the CLI

From the repo root, use `rink` if it is on PATH, otherwise `cargo run -q -p rink-cli --`. Every
command prints JSON.

## Steps

1. **Pin down the situation.** You need:
   - the zone;
   - who has the puck and where;
   - the user's role;
   - the 3–5 players who matter;
   - what happened, beat by beat;
   - the moment the user wants judged.

   Ask for at most two things that are missing and would change the shape. Assume the rest and
   list your assumptions in your reply.
2. **Start from a template.** Use the closest one; it has the full roster, and the home LW is
   marked as `me`.

   ```sh
   rink new breakout --out <library>/<slug>.rink.json   # faceoff | dzone | breakout
   ```

   The library folder comes from `rink list`. Never overwrite an existing file; choose a new name.
3. **Edit the JSON** (format below):
   - title and notes;
   - `is_me` on the user's role;
   - keyframes, events and a `decision_point` with the user's question as its note;
   - `duration_ms`.

   Keyframe only the players who matter, at each beat (every 1–2 s). Everyone else gets one
   keyframe at a sensible spot so the shape is right.
4. **Validate and sanity-check.**
   - Run `rink check <play>` and fix every issue it reports. It also flags any skater moving
     faster than 8 m/s between keyframes.
   - Run `rink timeline <play> --compact --interval 1000`. It also samples at every event and
     puck pickup.
   - Then check that the story is right, not just the structure:
     - the puck carrier at each beat (possession comes only from events);
     - `me`'s distance and side relative to the key opponent at the moment in question.

     If the user says "I was too slow to close", `me` must actually be late there.
5. **Show it in the app.** Run `rink open <play> --at <decision point ms> --select <me's id>`.
   The open window switches to the play, or the app starts if no window is running. Do this once,
   when the file is finished and `rink check` passes, not after every edit.
6. **Reply to the user.**
   - Give the path, a one-line summary of each beat, and your assumptions.
   - Say it's in the app window, and ask them to fix positions by dragging, especially `me` and
     the key opponent around the decision point. The app autosaves their edits.
   - If they also asked a positioning question ("where should I have been?"), offer to run the
     `review-positioning` skill now. If they want the answer straight away, run it, and say that
     the answer rests on the drafted positions.

## Changing or loading an existing play

- **Which play.** "This play", "the one I have open": run `rink current`, which gives the path,
  playhead and selection in the app window, and whether it has unsaved edits. Otherwise use the
  path they give, or `rink list`.
- **Unsaved edits.** The app autosaves about a second after the user's last edit. If
  `rink current` says `"dirty": true`, they're mid-edit: wait two seconds and run it again. If it
  stays dirty, ask them to pause editing (or press Cmd-S) before you change the file, so you build
  on their version.
- **A variant, such as "the ideal version".** Copy the play to a new file with a new `id` and a
  title that says what it is, and change the copy. Edit the original in place only when they ask.
- **Edit, check, then show.** Make all your changes, run `rink check`, then run `rink open <play>`
  once. On the play that's already open, this reloads it in place and keeps the playhead and
  selection. Pass `--at` and `--select` to point them at what changed.
- The window never drops the user's work. If you change a file while they have unsaved edits in
  it, the app doesn't overwrite your change: it puts their edits in a copy next to it named
  `<play>.your-edits-<time>.rink.json` and asks them which version to keep. Avoid that by
  checking `rink current` first, as above.
- **Just show a play.** "Open the breakout": `rink open <play>`, with no edits.

## Coordinates

- Meters from center ice. x runs along the rink (−30 to 30 on the IIHF 60 × 30 m rink), and the
  home team attacks +x. y runs across (−15 to 15), and +y is home's left.
- **The user's team is home**, so their defensive zone is x < 0.
  - In their D-zone, their left wing's wall is around y = +12 to +14.
  - The left boards are y = +15, the right boards y = −15.
- **Left and right are from each team's own view.** Home's left (LW, LD) is +y. Away attacks −x,
  so away's left is −y.
  - So in home's D-zone, the opponent on home's left point (+y) is away's **RD**, and the
    opponent in home's right corner (−y) is usually away's **LW**.
  - When the user says "their LD", check that this matches the side they describe. If it
    doesn't, keep the side they describe, use the role that fits that side, and mention it.
- Directions the user gives ("right point", "left corner") are from their team's view unless
  they say otherwise.
- Landmarks, as (x, y) in meters:

  | Landmark | Position |
  | --- | --- |
  | Goal lines | x = ±26 |
  | Blue lines | x = ±7.14 |
  | Red line | x = 0 |
  | End-zone faceoff dots | (±20, ±7), circles of radius 4.5 m |
  | Neutral-zone dots | (±5.6, ±7) |
  | Home net | x = −26 to −27.1 |
  | Home goalie | about (−25, 0) |
  | Slot | about (−19, 0) for home's D-zone |
  | Points | about (−9, ±12) in home's D-zone |
  | Hash marks on the end-zone circles | y = ±11.5, at x = ±20 |
  | Corner arc centers | (±21.5, ±6.5), radius 8.5 m |
  | Behind the home net | about (−28, ±2) |

- The corners are rounded with an 8.5 m radius, so (±29, ±14) is off the ice. `rink check`
  catches this.
- In the file, facing is in **radians**, with 0 = +x and π/2 = +y. Leave it out to face the
  direction of travel. (The CLI's `--facing` takes degrees.)
- `rink` facts round positions to 0.5 m, so a spot you entered as (−9.3, −10.2) reads as
  (−9.5, −10.0).

## File format (`schema_version` 3)

```jsonc
{
  "schema_version": 3, "id": "<keep from template>", "title": "...", "rink": { /* keep */ },
  "duration_ms": 6000,
  "teams": [ { "side": "home", "name": "Home", "color": "#1f5fbf", "attacks_positive_x": true },
             { "side": "away", "name": "Away", "color": "#c8322d", "attacks_positive_x": false } ],
  "entities": [ { "id": "home_LW", "kind": "skater", "team": "home", "number": 17, "role": "LW",
                  "label": "", "is_me": true },           // kind: skater | goalie | puck
                /* ... home_C, home_RW, home_LD, home_RD, home_G, away_*, and "puck" */ ],
  "tracks": {   // entity id -> keyframes sorted by t_ms; a player with no keyframes isn't on the ice
    "home_LD": [ { "t_ms": 0, "pos": [-27.5, 3.0] },
                 { "t_ms": 1500, "pos": [-27.0, 6.0] } ],
    "home_LW": [ { "t_ms": 0, "pos": [-18.0, 11.0] },
                 { "t_ms": 2000, "pos": [-19.0, 12.0], "facing": 3.14 } ]
  },
  "events": [   // sorted by t_ms; actors[0] is the one acting (passer, shooter)
    { "t_ms": 0, "kind": "puck_recovery", "actors": ["home_LD"], "note": "" },
    { "t_ms": 1500, "kind": "pass", "actors": ["home_LD", "home_LW"], "arrive_ms": 2000, "note": "" },
    { "t_ms": 3000, "kind": "decision_point", "actors": ["home_LW"],
      "note": "Chip it past the pinching D or carry it out?" }
  ],
  "notes": "What happened, in the user's words."
}
```

Plays the user built in the app may also have `"orders"` (entity id → `{ "t_ms", "to": [[x, y], ...],
"pace", "backward" }`) and keyframes with `"vel": [vx, vy]` (m/s). The app made those keyframes by
skating each order, and the curve between them follows the stored velocities. Keep both when you edit
other parts of the play. If you change a player's movement after an order's moment, remove that
player's orders from that moment on, and the `vel` of the keyframes you move. Don't write orders
yourself; write keyframes.

Event kinds: `pass`, `shot`, `save`, `puck_recovery`, `turnover`, `zone_entry`, `zone_exit`, `hit`,
`faceoff`, `whistle`, `decision_point`. A shot or pass can carry `"to": [x, y]` for where the puck
went.

## Puck possession

Possession comes **only from events**. Keyframes say where players are, never who has the puck.

| Event | What happens to the puck |
| --- | --- |
| `puck_recovery` | `actors[0]` gets it: a loose-puck win, a battle won, the start of a play |
| `pass` | `actors[0]` releases it at `t_ms`; `actors[1]` gets it on arrival |
| `shot`, `whistle` | whoever has it loses it, and it's loose |
| `turnover` | whoever has it loses it; `actors[1]`, if given, gets it at once (a steal or interception) |
| `faceoff` | `actors[0]`, if given, comes out with it |
| `save` | nothing: the goalie stops it without holding it, and it stays loose |
| everything else | nothing; these are labels for the reviewer |

- **Every play where someone has the puck needs a starting event**, usually a `puck_recovery` at
  the first moment they have it. Without one, nobody has the puck.
- **A pass arrives** at `arrive_ms` if you set it. Otherwise the time is worked out from the
  distance at 15 m/s. Set `arrive_ms` when the timing matters, such as a pass the receiver had to
  wait for.
- **An interception:** if anyone gains the puck (a `puck_recovery` or a `turnover` to them) while
  a pass is travelling, the pass never arrives.
- `rink check` reports a pass or shot by someone who doesn't have the puck at that moment, and
  says who does.

**A loose puck** (a rebound, a dump-in, a puck along the boards) gets keyframes on the `"puck"`
entity itself.

- When nobody has the puck, it moves in a straight line through these points, in this order:
  - the release point (the last carrier's stick);
  - any puck keyframes;
  - the next pickup.
- **A rebound, for example:**
  - a `shot` at 500 ms;
  - a `save` by the goalie at 800 ms (not a `puck_recovery`, which would hand him the puck);
  - puck keyframes at the goalie's pad (800 ms) and in the corner (1900 ms);
  - a `puck_recovery` by whoever wins the race (2300 ms).
- **`"to"` on a shot or pass** is used only when nothing follows: no puck keyframes and no
  pickup. The puck then travels there at 25 m/s and stays. Puck keyframes and pickups take
  priority over `"to"`.
