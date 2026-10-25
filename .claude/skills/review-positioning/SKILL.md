---
name: review-positioning
description: Review a Rink Replay play and give position feedback ("at that moment you should have been there") that shows up as ghosts in the app. Use when the user asks for feedback, a review, or coaching on a play, their positioning, or a decision point.
---

# Review positioning in a Rink Replay play

You are reviewing a hockey play the user retraced in Rink Replay. Your output is a few position
suggestions, written with the `rink` CLI. The app watches for them and draws each one on the rink:
a shaded zone, a ghost skating the better route, and an arrow from where the player really was.

## Running the CLI

From the repo root, use `rink` if it is on PATH, otherwise `cargo run -q -p rink-cli --`. Every
command prints JSON. `rink help` lists them all.

## Steps

1. **Find the play.** Use the path the user gave. For "my play", "this one" or no path, run
   `rink current`, which names the play in the app window. Otherwise run `rink list` and pick
   the one they mean (ask if it's ambiguous). If `rink current` shows `"dirty": true`, ask them
   to save first (Ctrl-S): the CLI reads the file, not their unsaved edits. If the notes start with "Example", the play is an unchanged
   template; say so and check that this is the play they meant.
2. **Read the play.** Run `rink summary <play>`. Note:
   - who `me` is;
   - the events and decision points, with their notes;
   - the play notes;
   - `coordinates`: the axes and the landmarks (dots, hash marks, slot, points). Use the landmarks
     to place spots.
3. **Read the team's system.** Look for `system.md` in the library folder (`rink list` prints the
   folder). Positioning depends on the system: a box-plus-one and a low-zone collapse put the
   weak-side winger in different places.
   - With no profile, use the common principles below, and say so in your reply.
   - When a call depends on the system, ask about it, or offer the `team-system` skill.
4. **Get the shape of the play.**
   - Run `rink timeline <play> --compact --interval 500`. Each snapshot has full facts for `me`
     and the carrier (`focus`) and positions for everyone else (`others`).
   - Then run `rink snapshot <play> <t_ms>` (all players) or
     `rink snapshot <play> <t_ms> --entity <id>` (one player) at each decision point, each event,
     and any moment where `me` looks badly placed.
5. **Judge `me`'s positioning** at those moments. For each moment, answer: where was the puck,
   who had it, what was `me`'s job, and where does the system want `me`? Use the facts:
   - zone (from each player's own team's view);
   - strong or weak side;
   - above or below the puck;
   - gap to the nearest opponent;
   - open pass lanes. These are from the carrier, or from the passer for a moment after a pass.

   Focus on `me`. Comment on teammates only when the user asks, or when their spot explains
   `me`'s options.
6. **Try spots before writing them.** `rink compare` is a dry run. It saves nothing and shows
   `actual` and `suggested` facts side by side, plus whether the spot is valid:

   ```sh
   rink compare <play> --t <ms> --x <x> --y <y> [--facing <deg>] [--from <ms>]
   ```

   - Everyone else stays still. If `me` has the puck, the puck moves with them.
   - Add `--facing` (degrees; 0 = +x, 90 = +y) when the facing is part of the point, such as
     "back to the boards" or "open to the middle". Otherwise the suggested spot has no facing.
   - Keep a spot only if it changes something concrete: a lane opens, the gap grows, the side or
     the relation to the puck changes. If nothing changes, the issue may not be where you
     thought.
7. **Write the keepers.** One good suggestion is a fine review. So is none, if their positioning
   was sound. At most 3–4, most important first.

   ```sh
   rink suggest <play> --t <ms> --x <x> --y <y> \
     --from <ms when the better route starts> [--facing <deg>] --radius <m> --confidence <0..1> \
     --principle "<the principle, a few words>" \
     --reason "<one or two sentences: why, and what it changes, with numbers>" \
     [--detail "<optional longer explanation>"]
   ```

   - `--t`: the moment the feedback is about.
   - `--from`: when the player could first have started moving there. That's usually the last
     change of possession or the previous event. It defaults to 1.5 s before `--t`.
   - `--radius`: how sure you are of the exact spot. Use 1–1.5 m for a clear spot, 2–3 m for
     "somewhere in this area".
   - `--entity`: only for someone other than `me`.
   - The app shows the principle and the reason when the user reaches that moment, and `--detail`
     under them.
   - If the CLI rejects a spot as unreachable, branch earlier or pick a closer spot. Don't drop
     the reachability rule; a suggestion you couldn't have skated to isn't useful.
8. **Puck decisions** ("chip it, carry it or bump it?") are about the choice, not the position.
   Answer them in your reply, using the facts at that moment: pressure on the carrier, open
   lanes, and where support was. Add a ghost only when position is part of the answer: a better
   carry route (the puck moves with the carrier), or where `me` should have been to make the
   better option possible.
9. **Reply to the user.**
   - List each suggestion with its time, the principle, and why, using numbers from the
     comparison ("the lane from your D opens; their RW is 7 m away instead of 4.5").
   - Answer the decision points.
   - Mention what they did well in a line.
   - Before replying, run `rink open <play> --at <first suggestion's t_ms>`. The window loads the
     play if it isn't showing it, picks up the review and jumps to the first suggestion.
   - Tell them the ghosts are in the app window: N and Shift-N jump between suggestions, and G
     toggles ghosts.

## Ground rules

- Positions are entered by hand and are approximate. Judge shape and spacing, not centimeters.
  Don't suggest moves under about 1.5 m unless the small move clearly changes a lane or a side.
- Don't edit the `.rink.json` play file. Feedback goes only through `rink suggest`.
- If a review already exists (`rink review <play>`), build on it: remove suggestions you disagree
  with (`rink unsuggest <play> <id>`) rather than stacking near-duplicates. Ids are never reused.
- A suggestion with `"stale": true` was written before the play's motion was edited. Re-check
  each one at its moment. Remove it and write a fresh one if the spot or the reason no longer
  fits.
- If a decision point or pass looks wrong (the wrong carrier, a pass from someone without the
  puck), run `rink check <play>`. Possession comes from events, so a missing `puck_recovery` is a
  common cause. Tell the user rather than reviewing around it.
- The puck usually has no keyframes of its own; its position comes from the carrier and the
  events. That's normal. Don't invent positions for players who have no keyframes; if a key
  player is missing, say so.
- Common principles when there's no system profile:
  - **D-zone:** weak-side forwards stay above the puck and between their point man and the slot;
    the strong-side winger takes the wall and the point; the center supports low.
  - **Breakout:** wingers get to the wall at the hash marks and are stopped, back to the boards,
    when the D gets the puck; the center swings low through the middle as an outlet.
  - **Neutral zone and regroups:** keep width with a forward in each lane; give the carrier an
    option ahead of the puck and one behind it.
  - **Offensive zone:** have a man in the slot or at the net front, one below the puck, one above
    it, and a D at each point. The weak-side D sits inside, ready to step to the middle.
  - **Without the puck:** stay on the defensive side of your check, with stick in the lane.
