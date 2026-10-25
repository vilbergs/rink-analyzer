# Rink Replay manual

How to retrace a shift, get it reviewed, and work quickly with the app and Claude Code together.
For the model, file format and build details, see the [README](../README.md).

- [The loop in one minute](#the-loop-in-one-minute)
- [Setup](#setup)
- [The window](#the-window)
- [Opening, creating and saving plays](#opening-creating-and-saving-plays)
- [Retracing a play](#retracing-a-play)
- [Playback and navigation](#playback-and-navigation)
- [Position feedback and ghosts](#position-feedback-and-ghosts)
- [Working with Claude Code](#working-with-claude-code)
- [What to do in the app, what to ask Claude for](#what-to-do-in-the-app-what-to-ask-claude-for)
- [Keyboard and mouse reference](#keyboard-and-mouse-reference)
- [Files and folders](#files-and-folders)
- [Troubleshooting](#troubleshooting)

## The loop in one minute

1. **Describe the shift to Claude Code.** For example: "draft a play: O zone, I'm LW, our D rims it up my
   wall and their D pinches". Claude writes a rough play and shows it in your app window.
2. **Fix it in the app.** Drag players to where they really were, at the moments that matter. Alt-click to
   say who had the puck. Then press Cmd-S.
3. **Ask for a review.** For example: "review my positioning in this play". Suggestions appear on the rink
   as green ghosts. Press N to step through them.
4. **Ask follow-ups.** For example: "why not stay higher at 2.2 s?" or "show me the ideal version". Claude
   updates the files and reloads the window.

Keep one app window open the whole time. Claude drives it. You don't need to reopen anything.

## Setup

Build once from the repo root:

```sh
cargo build --release -p rink-app -p rink-cli
```

Put `rink` on your PATH so you and Claude can call it from anywhere. An alias to the release build works
well, because `rink open` starts the app binary that sits next to it:

```sh
alias rink="$HOME/sites/vilbergs/rink-analyzer/target/release/rink"
```

Start the window with `rink open <play>`, or with `cargo run -p rink-app --release`. With no
arguments, the app opens the breakout example.

After pulling or rebuilding, quit the window (Cmd-Q) and open it again. A running window keeps the old
code.

## The window

```text
┌ Title ▾ · file · Saved ──────────── status · Undo Redo · Ghosts Flip Fit · Save ┐
│                                                     │ Play notes                │
│                    Rink                             │ Selection                 │
│        (drag players, Alt-click for the puck)       │ Position feedback         │
│                                                     │ Events                    │
│                                                     │ Warnings                  │
├─ ▶ 2.2s / 3.6s  1× ─────────────────── legend · ? Shortcuts ────────────────────┤
│ Events    ◆        ◆   (blue: events, orange: decision points)                  │
│ Feedback  ━━━━━━━━━◆   (green: where the better route splits off → the moment)  │
│ #17 LW    ◆   ▬▬▬▬ ◆   (the selected player's keyframes; grey bar: has the puck) │
└─────────────────────────────────────────────────────────────────────────────────┘
```

- **Toolbar.** Click the play's title to open another play (same as Cmd-O). Next to it are the file name
  and "Saved" or "Unsaved changes". The last status message ("Undid move of #17 LW", "Opened …") sits on
  the right. Drag an empty part of the toolbar to move the window.
- **Shortcut sheet.** Press ? (or Cmd-/), click "? Shortcuts" at the right of the timeline bar, or choose
  Help → Keyboard Shortcuts. Every shortcut is listed in one dialog; Esc or a click outside closes it.
- **Menu bar** (macOS). File, Edit, View and Playback menus hold the same commands as the keys, and
  show each key next to its command.
- **Rink.** It's drawn to scale (IIHF by default). Your player has an orange ring, and the selected one
  has a blue halo. The short line on each player shows where they face. The puck sits on the carrier's
  stick side.
- **Inspector** (the right-hand panel):
  - **Play notes:** the play's description. Long notes fold; click More.
  - **Selection:** the selected player's exact position (meters from center ice), speed, facing, and
    keyframe count. "One here" means there's a keyframe at the playhead.
  - **Position feedback:** one card per suggestion. The card whose ghost is on the ice right now is
    expanded. Click a card to jump to its moment.
  - **Events:** passes, recoveries, decision points and so on, with their notes. Click a row to jump
    there and select the player. Click ✕ to delete the event; undo brings it back.
  - **Warnings:** problems `rink check` would also report, for example a pass with no receiver.
- **Timeline.** It has a transport bar (play, time, speed) and three lanes. Click or drag in empty space
  to scrub. Hover a marker to see what it is. Drag a ◆ to retime it (see below).

## Opening, creating and saving plays

- **Cmd-O**, or a click on the title, opens the play picker. It lists every play in your library, newest
  first, with its length, number of decision points, and badges:
  - *Open*: the play in the window.
  - *Reviewed*: it has a review file.
  - *Your edits copy*: a backup made before a reload; see below.

  Use ↑ ↓ and Enter, or click a play. Esc or a click outside closes the picker.
- **New play.** The bottom of the picker has three templates: Breakout, D-zone set and Faceoff. Each one
  has the full 5v5 roster, with home LW (#17) marked as you. A new play is unsaved until you press Cmd-S.
  Then it goes into the library under a name made from its title. To get a better title and file name,
  ask Claude to draft the play instead.
- **Save** with Cmd-S (Ctrl-S works too). Claude and the `rink` CLI read the file, not your window, so
  save before you ask for a review.
- **Your edits are never thrown away.** When you open another play with unsaved edits, they're saved to
  the play you were on first. When Claude reloads the play you're editing, your edits go into
  `<play>.your-edits-<time>.rink.json` next to it. These copies show up in the picker. Delete them once
  you've taken what you need.

## Retracing a play

A play is a set of **keyframes**: where each player is at a given moment. Between keyframes, players
skate smooth curves. You don't need a keyframe every 100 ms; one at each change of direction or
speed is enough.

A good order of work:

1. **Set the start (0.0 s).** Press Home, then drag everyone to where they were when the play starts.
   Players who matter get a keyframe; leave the rest on their template spots.
2. **Step to the next moment that matters.** Press → (100 ms) or click the timeline, then drag the
   players who moved. Dragging always creates or updates a keyframe at the playhead, snapped to 100 ms.
   Only the dragged player gets the keyframe.
3. **Turn players.** Shift-drag a player to set their facing. Without a set facing, a player faces the
   way they're skating.
4. **Say who has the puck.** At the moment someone gets it, Alt-click them. The app adds the event that
   fits:
   - a **pass** if a teammate had it;
   - a **turnover** if an opponent had it;
   - a **puck recovery** if it was loose.

   Alt-click the same player again at the same moment to take it back. A pass flies at a typical speed.
   The receiver gets it on arrival unless someone intercepts it first.
5. **Play it back** (Space) at 0.5× (press ] to change speed) and fix what looks wrong.
6. **Save** (Cmd-S).

Fixing things:

- **Wrong time:** select the player, then drag their ◆ in the bottom timeline lane left or right. The
  keyframe keeps its position and can't pass the keyframes next to it. Events (top lane) can be dragged
  the same way, and a pass keeps its flight time.
- **Wrong keyframe:** go to it with Shift-← / Shift-→ (that jumps between the selected player's
  keyframes) and press Delete.
- **Wrong puck event:** click ✕ on it in the inspector's Events list.
- **Anything else:** Cmd-Z undoes it. A whole drag is one undo step. Shift-Cmd-Z redoes.

Coordinates, if you want to read the numbers: meters from center ice. +x is toward the end your team
attacks and +y is your team's left. F flips the view end to end without changing the play.

## Playback and navigation

| Do | Key |
| --- | --- |
| Play / pause | Space (at the end, Space starts over) |
| Step 100 ms | ← → |
| Previous / next keyframe of the selected player | Shift-← / Shift-→ |
| Start / end | Home / End (or Cmd-← / Cmd-→) |
| Jump to an event | Click it in the inspector, or click its ◆ in the timeline |
| Jump to a suggestion | N / Shift-N, or click its card or its ◆ |
| Playback speed | ] cycles 0.25×, 0.5×, 1×, 2× |
| Zoom | Pinch, mouse wheel, or Cmd-scroll, around the pointer |
| Pan while zoomed | Two-finger scroll |
| Fit the whole rink | 0 or the Fit rink button |
| Flip ends | F |

## Position feedback and ghosts

A review (from Claude, see below) is stored next to the play as `<play>.review.json`. The window picks it
up within a second; you don't need to reopen anything. Each suggestion shows as:

- a **shaded zone** at the spot you should have been in;
- a **ghost** of the player, skating the suggested route from where it splits off from yours;
- an **arrow** from where you really were;
- a **green bar** in the Feedback lane, from the split to the moment.

Press N / Shift-N to step through suggestions, and G to hide or show the ghosts. The inspector card gives
the principle, the reason, and how far off you were.

If you edit the play after the review, the suggestions grey out and say "Play edited since". They're
still drawn, but they were judged on the old motion. Ask for a fresh review.

## Working with Claude Code

Open Claude Code in this repo. Three skills do the hockey work:

| You want | Say something like | What happens |
| --- | --- | --- |
| A new play from a description | "draft a play: D-zone breakout, I'm LW strong side, our D rims it and their D pinches" | `draft-play` writes a `.rink.json` and opens it in your window |
| A change to a play | "move their RD to the net front at 3 s", "make the pass come 0.5 s later", "add a shot at 6 s" | Claude edits the file and reloads the window |
| A review | "review my positioning in this play" | `review-positioning` writes suggestions; ghosts appear |
| The ideal version | "show me the ideal version of this play" | a new `-ideal` play that you can compare with yours in the picker |
| Your team's way of playing | "set up my team's system" | `team-system` interviews you and writes `system.md`, which reviews then follow |

Things that make this faster:

- **"This play" and "this moment" work.** Claude runs `rink current`, which reports the play in your
  window, the playhead time, the selected player, and whether you have unsaved edits. So "what should I
  have done here?" while paused at 2.2 s with yourself selected is a complete question.
- **Save before asking.** If you have unsaved edits, Claude sees `"dirty": true` and asks you to save,
  because it reads the file.
- **Claude shows its result for you.** After editing, it runs
  `rink open <play> --at <ms> --select <id>`. The window switches to (or reloads) that play, at that
  moment, with that player selected. It does this once, when it's done.
- **Decision points are your questions.** When you draft a play, say what you want judged ("was I too
  deep when their D pinched?"). It goes into the play as a decision point and shows in orange on the
  timeline.
- **Ask "why", or argue.** For example: "why not stay on the wall?" or "I couldn't get there, their F2
  was in the lane". Claude can check a spot without saving anything (`rink compare`), then update or
  drop a suggestion.
- **Ghost wrong?** Ask Claude to remove it ("drop the suggestion at 4 s"), or ask for a fresh review
  after you change the play.

## What to do in the app, what to ask Claude for

| In the app (fastest by hand) | Ask Claude (no UI for it yet) |
| --- | --- |
| Moving players, facing, timing of keyframes | Renaming a play or changing its notes |
| Who has the puck (Alt-click) | Shots, whistles, zone entries/exits, hits, saves |
| Retiming and deleting events | Decision points and event notes |
| Deleting keyframes, undo/redo | Play length, team names, colors, jersey numbers |
| Opening plays, starting from a template | Adding or removing players, changing who "me" is |
|  | Deleting plays or backup copies from the library |

## Keyboard and mouse reference

| Input | Action |
| --- | --- |
| ?, Cmd-/ | Show the shortcut sheet |
| Cmd-O | Open a play or start a new one (Ctrl-O works too) |
| ↑ ↓ Enter Esc | Move through the picker, open, close |
| Drag a player or the puck | Move them; creates a keyframe at the playhead (snapped to 100 ms) |
| Shift-drag | Turn facing |
| Alt-click | Give the puck: pass, turnover or recovery. Again to undo |
| Click the ice | Clear the selection |
| Drag a timeline ◆ | Retime a keyframe (bottom lane) or event (top lane) |
| Hover a timeline ◆ | See what it is |
| Space | Play / pause |
| ← → | Step 100 ms |
| Shift-← Shift-→ | Previous / next keyframe of the selected player |
| Home End, Cmd-← Cmd-→ | Start / end |
| Delete, Backspace | Delete the selected player's keyframe at the playhead |
| Cmd-Z, Shift-Cmd-Z (Ctrl-Y) | Undo, redo |
| ] | Playback speed |
| N, Shift-N | Next / previous position suggestion |
| G | Show / hide ghosts |
| F | Flip ends |
| Pinch, wheel, Cmd-scroll | Zoom around the pointer |
| Two-finger scroll | Pan |
| 0, Cmd-0 | Fit the rink |
| Cmd-S, Ctrl-S | Save |
| Cmd-H, Cmd-Q | Hide, quit |

## Files and folders

The library folder holds your plays. It's `$RINK_REPLAY_LIBRARY` if set, otherwise
`~/Library/Application Support/rink-replay/plays/` on macOS. `rink list` prints it.

| File | What it is |
| --- | --- |
| `<name>.rink.json` | A play: players, keyframes, events, notes |
| `<name>.review.json` | Position suggestions for that play |
| `<name>.your-edits-<time>.rink.json` | Your unsaved edits, kept when a play was reloaded under you |
| `system.md` | Your team's system, read by reviews |
| `.app-state.json`, `.open-request.json` | How Claude and the window talk; leave them alone |

All of these are plain JSON or Markdown, so you can diff them, back them up, or put them in git.

## Troubleshooting

- **Claude's change doesn't show.** The window reloads only on `rink open`, not when a file changes. Ask
  Claude to open it, or pick the play again with Cmd-O.
- **"Couldn't open the play".** The file doesn't load. Run `rink check <play>` to see why.
- **Ghosts are grey with "Play edited since".** The play changed after the review. Ask for a fresh review.
- **The review is about an old version of my play.** You probably didn't save before asking. Press Cmd-S
  and ask again.
- **New features are missing.** The window is running an old build. Quit it with Cmd-Q, rebuild, and run
  `rink open` again.
- **Pinch zoom does nothing.** It works only on macOS. Use the mouse wheel or Cmd-scroll elsewhere.
