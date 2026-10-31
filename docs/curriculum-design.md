# Rink Replay as a study tool

How the app could support a theory curriculum, written 2026-10-31 while setting one up in the HQ vault
(`Resources/Hockey/Theory curriculum.md`). The vault is where the curriculum, the progress and the Saturday
log live. The app is where a play is watched. This note is about the gaps between them.

## What the curriculum needs from a play

A lesson is: a principle, one play with a decision point, a transcript of what happens beat by beat with where
`me` is and why, a "what wrong looks like" variant, and a cue for the ice. Today the play file holds the
positions, the events, a question at each decision point, and free-text notes. Everything else was written
by hand into the vault, from `rink timeline` facts. That hand-written text is the thing most likely to drift
from the play once it's edited.

## Proposals, in the order I'd build them

### 1. Captions in the play, shown while it plays (done 2026-10-31)

The transcript belongs in the play file, not beside it.

```jsonc
"captions": [
  { "t_ms": 0,    "text": "Their C has it in our left corner. I'm above the puck, between it and their RD." },
  { "t_ms": 2500, "text": "Pass to the point. I push now, because I'm already inside." }
]
```

- Model: `captions: Vec<Caption>` on `Play`, `#[serde(default)]`, so no migration and schema stays 3.
- App: a caption strip under the rink showing the caption whose `t_ms` is the latest at or before the
  playhead; small marks on the timeline. Editable in the inspector like event notes.
- CLI: `rink caption <play> --t MS --text "..."` to write one; `rink transcript <play> [--md]` to print them
  as the table the vault notes use. Then the vault embeds the export instead of a hand copy, and the app and
  the note can't disagree, the same way `sample()` keeps the renderer and the CLI honest.
- Drafting: `rink narrate <play>` emits a first-pass transcript from the facts the CLI already computes
  (carrier changes, zone changes, events, `me`'s side and above/below). Claude then rewrites it into hockey
  language. The draft is mechanical; the rewrite is the `draft-play` skill's job.

This is the "transcript for each sequence" feature. It's cheap because every input exists already.

### 2. Decision points carry an answer (done 2026-10-31)

A decision point has a question (`note`) and no answer. Add `answer: String` (default empty). The app hides it
until the user presses a key at that moment, then shows it. A lesson becomes self-contained in the file:
question, reveal, caption. `rink summary` prints both.

### 3. Open a play from a vault link (done 2026-10-31)

Obsidian links to custom URL schemes. `rink://open?play=<name>&at=2500&select=home_LW` from a lesson note
should land in the running window at that moment.

- `scripts/install-mac.sh` adds `CFBundleURLTypes` for `rink` to Info.plist.
- The app handles the open-URL event by writing the same `.open-request.json` that `rink open` writes, so the
  existing 0.75 s poll does the rest. No new code path in the window.
- `rink open` already starts the app when no window runs, so the URL handler can `open -a "Rink Replay"`
  first and then write the request.

This is the one change that removes a copy-paste step from every lesson.

### 4. Tags and order on plays

`tags: ["dzone", "weak-side"]` and `lesson: 2` in the file. `rink list --tag dzone`, and a picker section
that groups tagged plays in lesson order under a "Curriculum" heading. Small. Mostly for the picker, since
the vault already orders the lessons.

### 5. Quiz mode

Hide `me` from half a second before a decision point. The user places a marker where they think they should
be, presses reveal, and the app shows the drafted position, the distance, and the facts for both (which is
what `rink compare` returns today). This is the first feature that makes studying active instead of
watching, and it reuses the suggestion rendering: the drafted spot is a suggestion with the user's marker as
"actual".

### 6. Contrast variants

Every lesson has a "what wrong looks like" version. Today that's an instruction to drag a keyframe. A
`variant_of: <play id>` field would let the app overlay the other version's `me` as a ghost (the review
ghost renderer already does this for a suggested route), with a toggle. The `draft-play` skill would write
both files.

### 7. Stills for the notes

`rink render <play> --at MS --out lesson1.png`: a top-down frame with the rink, players and the caption.
rink-core has no renderer outside GPUI; a small tiny-skia renderer of the rink and dots is a day's work and
makes the vault notes readable on the phone, where the app isn't.

## Not worth building

- Progress tracking in the app. The vault's progress table and the Saturday log already do this, and
  `/week` reads them.
- Lesson sequencing or a course UI inside the app. The picker with a "Curriculum" section (4) is enough; the
  vault is the syllabus.
- Automatic review of the curriculum plays. They are drafted as the right answer; reviews are for the user's
  own retraced plays.

## Status

1 to 3 landed on 2026-10-31: `captions` and `answer` in the file, `rink caption`, `rink answer`,
`rink transcript --md`, the caption strip and the `A` reveal in the app, and `rink://open` links for the
installed app. `rink narrate` (the mechanical draft from facts) is still open. 4 to 7 are proposals.

## Order and size

1 and 2 are model fields plus a strip and a reveal in the inspector, with CLI subcommands beside the existing
ones. 3 is a plist entry and one event handler. Together they turn a play file into a lesson. 4 is a half
day. 5 and 6 are each a few days and need the first three. 7 is independent.
