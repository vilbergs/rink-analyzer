---
name: draft-play
description: Draft a new Rink Replay play (.rink.json) from the user's description of a hockey situation, ready to tweak in the app and review. Use when the user describes a play, shift or scenario and wants it on the rink ("I'm LW, our D rimmed it and their D pinched...").
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
     - the puck carrier at each beat;
     - `me`'s distance and side relative to the key opponent at the moment in question.

     If the user says "I was too slow to close", `me` must actually be late there.
5. **Reply to the user.**
   - Give the path, a one-line summary of each beat, and your assumptions.
   - Tell them to open it with `cargo run -q -p rink-app -- <play>` and fix positions by
     dragging, especially `me` and the key opponent around the decision point.
   - If they also asked a positioning question ("where should I have been?"), offer to run the
     `review-positioning` skill now. If they want the answer straight away, run it, and say that
     the answer rests on the drafted positions.

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

## File format (`schema_version` 1)

```jsonc
{
  "schema_version": 1, "id": "<keep from template>", "title": "...", "rink": { /* keep */ },
  "duration_ms": 6000,
  "teams": [ { "side": "home", "name": "Home", "color": "#1f5fbf", "attacks_positive_x": true },
             { "side": "away", "name": "Away", "color": "#c8322d", "attacks_positive_x": false } ],
  "entities": [ { "id": "home_LW", "kind": "skater", "team": "home", "number": 17, "role": "LW",
                  "label": "", "is_me": true },           // kind: skater | goalie | puck
                /* ... home_C, home_RW, home_LD, home_RD, home_G, away_*, and "puck" */ ],
  "tracks": {   // entity id -> keyframes sorted by t_ms; a player with no keyframes isn't on the ice
    "home_LD": [ { "t_ms": 0, "pos": [-27.5, 3.0], "has_puck": true },
                 { "t_ms": 1500, "pos": [-27.0, 6.0], "has_puck": true } ],
    "home_LW": [ { "t_ms": 0, "pos": [-18.0, 11.0] },
                 { "t_ms": 2000, "pos": [-19.0, 12.0], "facing": 3.14, "has_puck": true } ]
  },
  "events": [   // sorted by t_ms; actors[0] is the one acting (passer, shooter)
    { "t_ms": 1500, "kind": "pass", "actors": ["home_LD", "home_LW"], "note": "" },
    { "t_ms": 3000, "kind": "decision_point", "actors": ["home_LW"],
      "note": "Chip it past the pinching D or carry it out?" }
  ],
  "notes": "What happened, in the user's words."
}
```

Event kinds: `pass`, `shot`, `puck_recovery`, `turnover`, `zone_entry`, `zone_exit`, `hit`,
`faceoff`, `whistle`, `decision_point`. A shot or pass can carry `"to": [x, y]` for where the puck
went.

## Puck possession

Possession follows these rules; get them right or the puck will wander:

- Only `pass`, `shot` and `turnover` change possession. All other events, including
  `puck_recovery`, are labels on the timeline for the reviewer. A recovery is really a `has_puck`
  keyframe, plus a `puck_recovery` event for the label.

- A keyframe with `"has_puck": true` means the player has the puck at that moment.
- Possession lasts until one of these happens:
  - the same player has a keyframe without `has_puck`;
  - that player is `actors[0]` of a `pass`, `shot` or `turnover`;
  - someone else has a `has_puck` keyframe.
- So **every keyframe of the carrier while they hold the puck needs `"has_puck": true`**.
- For a pass: the passer's last keyframe before the pass has the puck; the `pass` event is at the
  release time; the receiver's first keyframe at or after the catch has `has_puck`. The puck
  travels between them on its own.
- Never give two players `has_puck` at the same `t_ms`.

**A loose puck** (a rebound, a dump-in, a puck along the boards) gets keyframes on the `"puck"`
entity itself.

- When nobody has the puck, it moves in a straight line through these points, in this order:
  - the release point (the last carrier's stick);
  - any puck keyframes;
  - the next pickup.
- **A rebound, for example:**
  - a `shot` at 500 ms;
  - puck keyframes at the goalie's pad (800 ms) and in the corner (1900 ms);
  - a `has_puck` keyframe on whoever wins the race (2300 ms).
- **`"to"` on a shot or pass** is used only when nothing follows: no puck keyframes and no
  pickup. The puck then travels there at 25 m/s and stays. Puck keyframes and pickups take
  priority over `"to"`.
