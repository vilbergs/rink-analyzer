# Rink Replay manual

How to retrace a shift, get it reviewed, and work quickly with the app and Claude Code together.
For the model, file format and build details, see the [README](../README.md).

- [The loop in one minute](#the-loop-in-one-minute)
- [Setup](#setup)
- [The window](#the-window)
- [Opening, creating and saving plays](#opening-creating-and-saving-plays)
- [Retracing a play](#retracing-a-play)
- [Giving orders instead of dragging](#giving-orders-instead-of-dragging)
- [Events and decision points](#events-and-decision-points)
- [The play, the roster and who you are](#the-play-the-roster-and-who-you-are)
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
   say who had the puck. Press D at the moment you want judged and type your question. The app saves
   as you go.
3. **Ask for a review.** For example: "review my positioning in this play". Suggestions appear on the rink
   as green ghosts. Press N to step through them.
4. **Ask follow-ups.** For example: "why not stay higher at 2.2 s?" or "show me the ideal version". Claude
   updates the files and reloads the window.

Keep one app window open the whole time. Claude drives it. You don't need to reopen anything.

## Setup

Install the app once from the repo root:

```sh
scripts/install-mac.sh
```

This builds **Rink Replay.app** into `~/Applications` and links `rink` into `~/.local/bin`, which is on
your PATH. Open the app from the Dock, Spotlight or Launchpad, or with `rink open <play>`. With no play
given, it opens the play you changed last.

After pulling new code, quit the app (Cmd-Q) and run the script again. A running window keeps the old
code.

While working on the app itself, `cargo run -p rink-app --release -- <play>` runs it straight from the
repo.

## The window

```text
┌ Title ▾ · file · Saved ──────────── status · Undo Redo · Ghosts Flip Fit · Save ┐
│                                                     │ Play            Edit      │
│                    Rink                             │ Selection                 │
│        (drag players, Alt-click for the puck)       │ Off the ice               │
│                                                     │ Position feedback         │
│                                                     │ Events          + Add     │
├─ ▶ 2.2s / 3.6s  1×  All players ────── legend · ? Shortcuts ────────────────────┤
│ Events    ◆        ◆   (blue: events, orange: decision points)                  │
│ Feedback  ━━━━━━━━━◆   (green: where the better route splits off → the moment)  │
│ #17 LW    ◆   ▬▬▬▬ ◆   (keyframes; grey bar: has the puck)                       │
└─────────────────────────────────────────────────────────────────────────────────┘
```

- **Toolbar.** Click the play's title to open another play (same as Cmd-O). Next to it is the file name
  and the save state: *Saving…* while you edit, *Saved* a moment later. The last status message ("Undid
  move of #17 LW", "Opened …") sits on the right. Drag an empty part of the toolbar to move the window.
- **Red banner.** Errors, and a play that changed on disk while you were editing it, show here until you
  deal with them (see [saving](#opening-creating-and-saving-plays)).
- **Shortcut sheet.** Press ? (or Cmd-/), click "? Shortcuts" at the right of the timeline bar, or choose
  Help → Keyboard Shortcuts. Every shortcut is listed in one dialog; Esc or a click outside closes it.
- **Menu bar.** File, Edit, View, Playback and Help hold the same commands as the keys, and show each key
  next to its command.
- **Rink.** It's drawn to scale (IIHF by default). Your player has an orange ring, and the selected one
  has a blue halo. The short line on each player shows where they face. The puck sits on the carrier's
  stick side.
- **Inspector** (the right-hand panel):
  - **Play:** the notes. Click Edit for the title, notes, team names and length.
  - **Selection:** the selected player's exact position (meters from center ice), speed, facing and
    keyframe count ("one here" means there's a keyframe at the playhead). Below that: their number,
    *This is me*, and *Take off the ice*.
  - **Off the ice:** roster spots with nobody on the ice. Click one to put them on.
  - **Position feedback:** one card per suggestion. Click a card to jump to its moment.
  - **Events:** passes, recoveries, decision points and so on. Click one to jump there and open it for
    editing. *+ Add* adds one at the playhead.
  - **Warnings:** problems `rink check` would also report, for example a pass with no receiver.
- **Timeline.** A transport bar (play, time, speed, *All players*) over the lanes. Click or drag in empty
  space to scrub. Hover a marker to see what it is. Drag a ◆ to retime it. *All players* (or A) gives
  every player on the ice a lane; click a lane's name to select that player.

Text fields work like any Mac text field. Changes apply as you type, and one visit to a field is one undo
step. Enter (Cmd-Enter in notes) or a click elsewhere finishes; Esc puts back what was there before.

## Opening, creating and saving plays

- **Cmd-O**, or a click on the title, opens the play picker. It lists every play in your library, newest
  first, with its length, number of decision points, and badges:
  - *Open*: the play in the window.
  - *Reviewed*: it has a review file.
  - *Your edits copy*: your edits, kept aside when the file changed under you; see below.

  Use ↑ ↓ and Enter, or click a play. Esc or a click outside closes the picker.
- **New play.** The bottom of the picker has three templates: Breakout, D-zone set and Faceoff. Each one
  has the full 5v5 roster, with home LW (#17) marked as you. It's saved into the library as soon as you
  change something, under a name made from its title. Give it a title with Play → Edit.
- **Saving is automatic.** About a second after you stop editing, the play is saved to its file. So
  Claude and the `rink` CLI always see what you see. Cmd-S saves at once. Quitting or closing the window
  saves too.
- **When Claude changed the file while you were editing it,** your edits are never written over Claude's
  change, and Claude's change never wipes out your edits. Your edits go into
  `<play>.your-edits-<time>.rink.json` next to it, and the red banner asks which version to keep:
  - *Keep my version* saves your edits over the file;
  - *Use the file's* loads the file. Your edits stay in the copy.

  Autosave pauses until you pick. Delete the copies from the library once you don't need them.

## Retracing a play

A play is a set of **keyframes**: where each player is at a given moment. Between keyframes, players
skate smooth curves. You don't need a keyframe every 100 ms; one at each change of direction or
speed is enough.

A good order of work:

1. **Set the start (0.0 s).** Press Home, then drag everyone to where they were when the play starts.
   Players who matter get a keyframe; leave the rest on their template spots. Take players who weren't
   involved off the ice, or put missing ones on from *Off the ice*.
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
5. **Mark what you want judged.** At that moment, with yourself selected, press D and type the question.
6. **Play it back** (Space) at 0.5× (press ] to change speed) and fix what looks wrong.

Faster by keyboard:

- **Tab / Shift-Tab** select the next or previous player on the ice, starting from you.
- **Alt-arrows** nudge the selected player half a meter on screen, as a keyframe at the playhead.
- **Cmd-C / Cmd-V** copy where the selected player is at the playhead, and paste that spot onto the
  selected player. Use it to hold someone in place ("he stood there until 3 s") or to line two players
  up.
- **All players** (A) in the timeline shows everyone's keyframes, so you can see who moves when.

Fixing things:

- **Wrong time:** drag the player's ◆ in their timeline lane left or right. The keyframe keeps its
  position and can't pass the keyframes next to it. Events (top lane) can be dragged the same way, and a
  pass keeps its flight time.
- **Wrong keyframe:** go to it with Shift-← / Shift-→ (that jumps between the selected player's
  keyframes) and press Delete.
- **Wrong event:** click it to change it, or click its ✕ to delete it.
- **Anything else:** Cmd-Z undoes it. A whole drag is one undo step. Shift-Cmd-Z redoes.

Coordinates, if you want to read the numbers: meters from center ice. +x is toward the end your team
attacks and +y is your team's left. F flips the view end to end without changing the play.

## Giving orders instead of dragging

Instead of placing every keyframe, you can tell players where to go, as in a strategy game, and let
the app skate them there:

1. Select a player and **right-click** (or Ctrl-click) where they should skate. A dotted line shows the
   path they'll take from the playhead, and a dashed ring shows where they'll stop. The inspector shows
   the order and when it's done.
2. Give the other players their orders the same way, then press **Space** and watch.
3. **Pause where something changes** and right-click again. The new order takes over from that moment:
   the player keeps the speed and direction they had, so a player flying up the wall turns in a wide arc,
   and one told to go back the way they came stops and pushes off first. You can also give orders while
   the play runs.

The skating has limits like a real skater's: a top speed, pushing off, braking, and turns that get wider
the faster they go. An order replaces everything that player did after its moment, including later
orders. Scrub back and right-click to change a direction you already gave.

- **Shift-right-click** adds a stop to the player's current order: they skate through the earlier spot
  without stopping and stop at the new one.
- **P** changes the pace of the selected player's current order: easy (4 m/s), skate (6.5 m/s) or sprint
  (7.8 m/s). **B** skates it backwards, facing away from the direction of travel. The inspector has the
  same buttons. New orders keep the player's last pace and direction.
- A spot past the boards is pulled onto the ice. If someone is still skating when the play ends, the
  play gets longer.
- Orders become ordinary keyframes, so dragging, nudging and Delete still work on them. In the timeline,
  an order's moment is an accent-colored ◆, and the keyframes it made are small grey ones.
- The puck takes no orders; passes and shots move it.

## Events and decision points

Events say what happened: passes, shots, hits, zone entries, whistles. Possession comes only from them;
see the [README](../README.md#model-in-brief) for the rules.

- **Add one:** move the playhead to the moment, select who did it, and click *+ Add* in Events, then the
  kind. Alt-click covers the common puck changes, and D covers decision points.
- **Edit one:** click it. It opens with:
  - its **kind**, which you can switch;
  - its **players** in order. Click a player chip to remove them. To add someone, select them on the rink
    and click *+ #19 C*. For a pass, the passer comes first, then the receiver; for a turnover, who loses
    it, then who gets it;
  - its **note**.

  Click the event again to close it.
- **Decision points** are your questions to the reviewer ("was I too deep when their D pinched?"). They
  show in orange on the timeline, and a review judges your positioning at each of them.

## The play, the roster and who you are

- **Play → Edit:** the title (it's also what the picker shows), the notes (what happened, and what you
  want judged), the team names, and the length in half-second steps. The length can't be shorter than
  the last keyframe or event.
- **Numbers:** select a player and type in the number field under their stats.
- **This is me:** marks the selected player as you. Reviews are about this player.
- **Take off the ice / put on the ice:** taking a player off removes their keyframes; they can't be in
  any events. *Off the ice* lists every empty roster spot, including roles a drafted play never had.
  Click one to put that player on near the puck, then drag them where they were.

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
| A bigger change to a play | "move their whole D-zone setup 2 m toward the slot", "replay it with our D pinching" | Claude edits the file and reloads the window |
| A review | "review my positioning in this play" | `review-positioning` writes suggestions; ghosts appear |
| The ideal version | "show me the ideal version of this play" | a new `-ideal` play that you can compare with yours in the picker |
| Your team's way of playing | "set up my team's system" | `team-system` interviews you and writes `system.md`, which reviews then follow |

Things that make this faster:

- **"This play" and "this moment" work.** Claude runs `rink current`, which reports the play in your
  window, the playhead time, the selected player, and whether you're mid-edit. So "what should I have
  done here?" while paused at 2.2 s with yourself selected is a complete question.
- **No need to save first.** The app autosaves about a second after your last edit. If Claude catches you
  mid-edit, it waits a moment and checks again.
- **Claude shows its result for you.** After editing, it runs
  `rink open <play> --at <ms> --select <id>`. The window switches to (or reloads) that play, at that
  moment, with that player selected. It does this once, when it's done.
- **Ask "why", or argue.** For example: "why not stay on the wall?" or "I couldn't get there, their F2
  was in the lane". Claude can check a spot without saving anything (`rink compare`), then update or
  drop a suggestion.
- **Ghost wrong?** Ask Claude to remove it ("drop the suggestion at 4 s"), or ask for a fresh review
  after you change the play.

## What to do in the app, what to ask Claude for

Everything about a play can be edited in the app now. Claude is for coaching, and for changes that
would take many edits by hand.

| In the app | Ask Claude |
| --- | --- |
| Moving and turning players, timing of keyframes | Drafting a play from a description |
| Who has the puck, and any other event | Reviews, "why" questions, the ideal version |
| Decision points and their questions | Moving many players at once, replaying a variant |
| Title, notes, team names, numbers, length | Team colors, a different rink (NHL) |
| Who you are; players on and off the ice | Your team's system |
| Opening plays, starting from a template | Deleting plays or edits copies from the library |

## Keyboard and mouse reference

| Input | Action |
| --- | --- |
| ?, Cmd-/ | Show the shortcut sheet |
| Cmd-O | Open a play or start a new one |
| ↑ ↓ Enter Esc | Move through the picker, open, close |
| Drag a player or the puck | Move them; creates a keyframe at the playhead (snapped to 100 ms) |
| Right-click, Ctrl-click | The selected player skates there from the playhead on |
| Shift-right-click | Add a stop to the selected player's current order |
| P | Pace of the current order: easy, skate, sprint |
| B | Skate the current order backwards / forwards |
| Shift-drag | Turn facing |
| Alt-click | Give the puck: pass, turnover or recovery. Again to undo |
| D | Decision point at the playhead for the selected player; type its question |
| Click the ice | Clear the selection |
| Tab, Shift-Tab | Select the next / previous player |
| Alt-arrows | Nudge the selected player 0.5 m |
| Cmd-C, Cmd-V | Copy / paste a player's position at the playhead |
| Drag a timeline ◆ | Retime a keyframe or event |
| Hover a timeline ◆ | See what it is |
| Click a lane name | Select that player |
| A | All players in the timeline |
| Space | Play / pause |
| ← → | Step 100 ms |
| Shift-← Shift-→ | Previous / next keyframe of the selected player |
| Home End, Cmd-← Cmd-→ | Start / end |
| Delete, Backspace | Delete the selected player's keyframe at the playhead |
| Cmd-Z, Shift-Cmd-Z | Undo, redo |
| ] | Playback speed |
| N, Shift-N | Next / previous position suggestion |
| G | Show / hide ghosts |
| F | Flip ends |
| Pinch, wheel, Cmd-scroll | Zoom around the pointer |
| Two-finger scroll | Pan |
| 0, Cmd-0 | Fit the rink |
| Cmd-S | Save now (it also saves by itself) |
| Cmd-H, Cmd-Q | Hide, quit |

In a text field, the letters and Space type text, and Esc, Enter and Cmd-Enter end editing.

## Files and folders

The library folder holds your plays. It's `$RINK_REPLAY_LIBRARY` if set, otherwise
`~/Library/Application Support/rink-replay/plays/`. `rink list` prints it.

| File | What it is |
| --- | --- |
| `<name>.rink.json` | A play: players, keyframes, events, notes |
| `<name>.review.json` | Position suggestions for that play |
| `<name>.your-edits-<time>.rink.json` | Your edits, kept aside when the file changed under you |
| `system.md` | Your team's system, read by reviews |
| `.app-state.json`, `.open-request.json` | How Claude and the window talk; leave them alone |

All of these are plain JSON or Markdown, so you can diff them, back them up, or put them in git.

## Troubleshooting

- **Claude's change doesn't show.** The window reloads only on `rink open`, not when a file changes. Ask
  Claude to open it, or pick the play again with Cmd-O.
- **A red banner says the play changed on disk.** Claude (or something else) wrote the file while you
  were editing it. Pick *Keep my version* or *Use the file's*; nothing is lost either way.
- **"Couldn't open the play".** The file doesn't load. Run `rink check <play>` to see why.
- **Ghosts are grey with "Play edited since".** The play changed after the review. Ask for a fresh review.
- **A shortcut doesn't work.** A text field probably has the focus; press Esc or click the rink.
- **New features are missing.** The app is running an old build. Quit it with Cmd-Q, run
  `scripts/install-mac.sh` again, and reopen it.
- **`rink: command not found`.** Run `scripts/install-mac.sh`; it links `rink` into `~/.local/bin`.
