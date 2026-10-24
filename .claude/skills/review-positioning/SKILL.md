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

1. **Find the play.** Use the path the user gave. Otherwise run `rink list` and pick the one they
   mean (ask if it's ambiguous).
2. **Read the play.** Run `rink summary <play>`. Note who `me` is, the decision points and their
   notes, and the play notes. Read the coordinates guide in the output once.
3. **Read the team's system.** Look for `system.md` in the library folder (`rink list` prints the
   folder). Positioning depends on the system: a box-plus-one and a low-zone collapse put the
   weak-side winger in different places. With no profile, use common principles, say so in your
   reply, and ask about the system when a call depends on it.
4. **Get the shape of the play.** Run `rink timeline <play> --compact --interval 500`. Then run
   `rink snapshot <play> <t_ms>` at each decision point, each event, and any moment where `me`
   looks badly placed.
5. **Judge `me`'s positioning** at those moments. For each moment, answer: where was the puck,
   who had it, what was `me`'s job, and where does the system want `me`? Use the facts: zone,
   strong or weak side, above or below the puck, gap to the nearest opponent, and open pass lanes
   from the carrier. Focus on `me`. Comment on teammates only when the user asks, or when their
   spot explains `me`'s options.
6. **Write a suggestion for each real issue.** Keep to at most 3–4, most important first.

   ```sh
   rink suggest <play> --t <ms> --x <x> --y <y> \
     --from <ms when the better route starts> --radius <m> --confidence <0..1> \
     --principle "<the principle, a few words>" \
     --reason "<one or two sentences: why, and what it changes>"
   ```

   - `--t`: the moment the feedback is about.
   - `--from`: when the player could first have started moving there. That's usually the last
     change of possession or the previous event. It defaults to 1.5 s before `--t`.
   - `--radius`: how sure you are of the exact spot. Use 1–1.5 m for a clear spot, 2–3 m for
     "somewhere in this area".
   - `--entity`: only for someone other than `me`.
7. **Check each suggestion's output.** It compares `actual` and `suggested` facts at `--t`, with
   everyone else held still.
   - If the move changes nothing concrete (no lane opens, no better gap, same side, same relation
     to the puck), remove it with `rink unsuggest <play> <id>` and reconsider.
   - If the CLI rejects a suggestion as unreachable, branch earlier or pick a closer spot. Don't
     drop the reachability rule; a suggestion you couldn't have skated to isn't useful.
8. **Reply to the user.** List each suggestion with its time, the principle, and why, using
   numbers from the comparison ("the lane from your D opens; their RW is 7 m away instead of
   4.5"). Mention what they did well in a line. Tell them the ghosts are already in the app if
   the play is open: N and Shift-N jump between suggestions, G toggles ghosts.

## Ground rules

- Positions are entered by hand and are approximate. Judge shape and spacing, not centimeters.
  Don't suggest moves under about 1.5 m unless the small move clearly changes a lane or a side.
- Don't edit the `.rink.json` play file. Feedback goes only through `rink suggest`.
- If a review already exists (`rink review <play>`), build on it: remove suggestions you disagree
  with rather than stacking near-duplicates.
- Don't invent positions for players who have no keyframes; if a key player is missing, say so.
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
