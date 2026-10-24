# Rink Replay

A desktop app for recreating real hockey plays as keyframed 2D animations on a to-scale, top-down rink. Your
Claude Code agent then reviews your positioning ("at that moment you should have been there"). The suggestion
appears on the rink as a ghost skating the better route.

![Breakout example at 3.0 s with two position suggestions](docs/screenshot.png)

Plan: [Rink Replay — Hockey Play Animator (Plan)](https://claude.ai/artifact/9v9NCszp1ViTAkhPhGwC1m)

## Status

| Milestone | State |
| --- | --- |
| M0 · Spike: GPUI window, IIHF rink drawn to scale, drag a player | Done |
| M1 · Core + playback: `rink-core` model, `sample()`, keyframes, scrub, play/pause | Done |
| M2 · Editor: multi-row timeline, event editing, templates UI, undo, autosave | Not started; a basic inspector and save are in |
| Position feedback: facts, `rink` CLI, Claude Code skill, ghosts in the app | Done (replaces the planned MCP server for now) |

## Layout

```text
crates/
  rink-core/   # model, rink geometry, interpolation (sample), validation, file format, templates
  rink-app/    # GPUI binary: rink-replay
  rink-cli/    # `rink`: JSON facts, timelines and position reviews for agents
rinks/         # rink presets (IIHF 60x30, NHL 61x26) as JSON
examples/      # breakout play with an example review
.claude/skills/   # Claude Code skills: review-positioning, draft-play, team-system
```

`rink-core` has no UI dependency. `sample(play, t_ms) -> Snapshot` is the one function that turns keyframes into
positions. The renderer and the `rink` CLI both use it, so the app and Claude can't disagree about where anyone
was.

## Model in brief

- Rink coordinates are meters with the origin at center ice. +x is home's attacking direction and +y is home's
  left. Time is in milliseconds.
- Skater paths use centripetal Catmull-Rom splines through the keyframes. Facing turns along the shortest arc
  between keyframes that set it; otherwise a skater faces their direction of travel.
- Possession: a keyframe with `has_puck` starts possession. It lasts until that entity has a keyframe without
  `has_puck`, makes a pass, shot or turnover, or someone else picks the puck up. A loose puck moves in a straight
  line from its release point to the next pickup, passing through any puck keyframes on the way.
- Each play is saved as one `.rink.json` file with a `schema_version`. Snapshots are always derived and never
  saved.

## Position feedback

1. **Retrace the play.** Retrace it in the app, mark the decision points, and save it with Ctrl-S.
2. **Ask Claude Code.** In this repo, ask: "review my positioning in <play>". The `review-positioning` skill:
   - reads the play through the `rink` CLI, as computed facts such as zone, strong or weak side, above or below
     the puck, gaps and open pass lanes;
   - judges your positioning at the key moments;
   - writes suggestions to `<play>.review.json` with `rink suggest`.
3. **Every suggestion is checked before it's stored.** The spot must be on the ice and reachable from where you
   really were when the routes split, at 8 m/s or less. The command also prints the facts for where you were next
   to the facts for where you should have been, so you can see what the move changes.
4. **The app picks up the review while the play is open.** Each suggestion shows as a shaded zone at the target,
   a ghost skating the suggested route, an arrow from your real position, and a green bar on the timeline. N and
   Shift-N jump between suggestions, and G toggles ghosts.

Three Claude Code skills in `.claude/skills/` drive this loop:

| Skill | Ask Claude Code | Writes |
| --- | --- | --- |
| `review-positioning` | "review my positioning in <play>" | `<play>.review.json` through `rink suggest` |
| `draft-play` | "draft a play: D-zone breakout, I'm LW strong side, our D rims it and their D pinches" | a new `.rink.json` |
| `team-system` | "set up my team's system" | `system.md` in the library folder, read by reviews |

`rink compare` is a dry run: it shows the facts for any spot without saving anything. Use it to try a spot before
writing a suggestion.

```sh
cargo run -q -p rink-cli -- summary examples/breakout.rink.json
cargo run -q -p rink-cli -- snapshot examples/breakout.rink.json 3000
cargo run -q -p rink-app -- examples/breakout.rink.json      # shows the example review
```

## Build and run

```sh
cargo run -p rink-app --release                       # opens the breakout example
cargo run -p rink-app --release -- --template faceoff  # faceoff | dzone | breakout
cargo run -p rink-app --release -- path/to/play.rink.json
cargo test --workspace
```

Ctrl-S saves to the open file. A new play is saved to the library folder: `$RINK_REPLAY_LIBRARY` if set,
otherwise `plays/` under the platform data directory.

On Linux, GPUI needs these packages (Debian/Ubuntu names): `libxkbcommon-dev libxkbcommon-x11-dev
libwayland-dev libvulkan-dev libx11-xcb-dev libxcb1-dev libfontconfig-dev libfreetype-dev`, plus a Vulkan
driver.

`gpui` is pinned to `=0.2.2` from crates.io instead of a Zed git revision. `Cargo.lock` pins `libc` to 0.2.188
because gpui's `xattr` 0.2 dependency fails to compile against newer `libc` releases.

## Controls

| Input | Action |
| --- | --- |
| Drag | Move; creates a keyframe at the playhead, snapped to 100 ms |
| Shift-drag | Turn facing |
| Alt-click | Toggle puck possession |
| Space | Play / pause |
| ← → | Step 100 ms |
| Shift ← → | Jump to the previous or next keyframe of the selected entity |
| Delete | Remove the selected entity's keyframe at the playhead |
| ] | Cycle playback speed (0.25×, 0.5×, 1×, 2×) |
| F | Flip ends |
| N / Shift-N | Next / previous position suggestion |
| G | Show / hide ghosts |
| Ctrl-S | Save |

## Known gaps

- IIHF line distances (goal line 4.0 m and blue lines 22.86 m from the end boards) come from secondary sources.
  They still need checking against the current IIHF rulebook.
- You can't answer a suggestion in the app yet, for example by dragging the ghost and asking "why not here?".
  Ask in Claude Code instead.
- The timeline shows one row: the selected entity's keyframes, plus the events above it. There's no event
  editing, undo, pan/zoom, overlays or command palette yet.
