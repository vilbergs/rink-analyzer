# Rink Replay

A desktop app for recreating real hockey plays as keyframed 2D animations on a to-scale, top-down rink. Later it
gets an MCP server, so an AI client can read a play as a timeline of snapshots and give feedback on positioning
and decisions.

![Breakout example at 1.9 s](docs/screenshot.png)

Plan: [Rink Replay — Hockey Play Animator (Plan)](https://claude.ai/artifact/9v9NCszp1ViTAkhPhGwC1m)

## Status

| Milestone | State |
| --- | --- |
| M0 · Spike: GPUI window, IIHF rink drawn to scale, drag a player | Done |
| M1 · Core + playback: `rink-core` model, `sample()`, keyframes, scrub, play/pause | Done |
| M2 · Editor: multi-row timeline, event editing, templates UI, undo, autosave | Not started; a basic inspector and save are in |
| M3 · MCP v1: `rink-mcp` over stdio | Not started |

## Layout

```text
crates/
  rink-core/   # model, rink geometry, interpolation (sample), validation, file format, templates
  rink-app/    # GPUI binary: rink-replay
rinks/         # rink presets (IIHF 60x30, NHL 61x26) as JSON
```

`rink-core` has no UI dependency. `sample(play, t_ms) -> Snapshot` is the one function that turns keyframes into
positions. The renderer uses it now, and the MCP server will use the same function.

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
| Ctrl-S | Save |

## Known gaps

- IIHF line distances (goal line 4.0 m and blue lines 22.86 m from the end boards) come from secondary sources.
  They still need checking against the current IIHF rulebook.
- The timeline shows one row: the selected entity's keyframes, plus the events above it. There's no event
  editing, undo, pan/zoom, overlays or command palette yet.
