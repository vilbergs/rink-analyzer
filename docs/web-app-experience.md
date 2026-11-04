# The web app experience

How the web app guides a player through the theory curriculum, written 2026-11-01. It merges three
things that already exist: the lessons in the vault (`Theory 0 - Curriculum` and `Theory 1` to `4`),
the plays in the Rink Replay library, and the quiz engine in chalktalk. The models are Brilliant (one
idea at a time, learn by doing, every explanation is interactive) and Duolingo (a path, short
sessions, instant feedback, come back tomorrow).

The companion note [web-app-design.md](web-app-design.md) covers the technical split. This one covers
what the user sees and does.

## The one-sentence version

A lesson is ten minutes on a phone: watch a play beat by beat, get stopped at the decision point and
asked what you would do, see what it costs to be wrong, and leave with one cue for Saturday. The path
shows where you are, practice brings back what you missed, and the Saturday check-in closes the loop
with the ice. No streaks, points or badges: retention is not a goal, understanding is.

## What each source contributes

| Source | Take | Leave |
| --- | --- | --- |
| The vault lessons | The lesson anatomy: principle, one play, decision point, transcript, "what wrong looks like", cue, Saturday log. The module order. The prose itself moves into the repo. | The vault as a source. After the move it can embed or link the app's content, not the other way around. |
| Rink Replay | Plays as keyframed, to-scale motion with captions and answers. `sample()`, facts, the suggestion check and the ghost rendering, which already draw "where you were" against "where you should be". | The editor as the primary surface. Editing stays a later phase. |
| chalktalk | Questions as a registry of types with pure grading, grade 0/1/2, feedback that explains the cost, model answers played as plans, situations as data with permanent ids, the bottom sheet over the rink. | Static boards. Every chalktalk situation becomes a moment in a moving play. |
| Brilliant | One concept per lesson. Interact before you read. Short explanations that pause for a question. | Long courses. Streaks. |
| Duolingo | The path. Locked until ready. Mixed practice of what you got wrong. | Everything built for retention: hearts, lives, leagues, XP, daily goals, reminders. |

## Structure

```text
Course: Right wing, recreational 5v5
  Module 1 · D-zone            L1 strong side wall · L2 weak side slot
  Module 2 · Breakouts         L3 I'm the wall · L4 I'm the weak side
  Module 3 · Neutral zone      L5 forecheck as F1 · L6 backcheck · L9 regroups and the trap
  Module 4 · Offensive zone    L7 rush entry · L8 cycle and net front
```

A **lesson** is 8 to 12 **steps** over one play, with a contrast variant of that play. Steps are the
unit of interaction. A lesson takes 5 to 10 minutes and ends with the cue.

A **module** is 2 or 3 lessons plus a **practice** node (mixed questions from the module, weighted to
what was graded below 2) and a **Saturday check-in** node.

The **path** is the home screen: modules as sections, lessons as nodes in order, the next one open,
the rest locked. A lesson stays open after completion so it can be replayed. Practice nodes unlock
when every lesson in the module is done. The check-in node is always open on Saturday and Sunday.

## A lesson, step by step

L1, "Puck in my corner", as it would run:

1. **Principle card.** One sentence: "The strong-side winger owns the point." One line under it on what
   the lesson will ask. A single Continue.
2. **Watch, 0.0 to 2.0 s.** The play runs at 0.5x with the captions as subtitles, and stops at the
   beat. The caption for the beat sits in the sheet. Continue.
3. **Pick.** "Who is your man right now?" Tap a player. Their LD is right, their C is "close" with
   feedback on why the corner is the D's battle, anyone else is wrong. The answer is marked on the
   rink.
4. **Watch, 2.0 to 2.5 s.** Stops at the decision point. The orange marker on the lane glows.
5. **Choice.** The decision point's question. Four options, each with a grade and feedback. The right
   option plays the real play on; a wrong one plays the contrast variant from this moment, so the cost
   is seen, not read.
6. **Spot.** Rewind to 2.5 s with your player hidden. "Where should you be?" Tap the rink. Graded by
   distance to the drafted position: within 2 m right, within 4 m close. After answering, the target
   zone, your tap and the ghost appear, with the facts for both: "You: below the puck, 6.5 m from their
   LD. Target: above the puck, 2.5 m, in the lane to the point."
7. **Watch, 2.5 to 4.5 s.** The push, the shot lane, the save.
8. **Contrast.** "What wrong looks like." The wrong variant plays with its caption. A toggle flips
   between the two with the playhead kept, the way Ghosts does in the desktop app.
9. **Cue card.** "When the puck goes into my corner, look up at my point man." A Done button, and the
   lesson is marked studied.

Steps 3, 5 and 6 are graded. There is no lesson score. The node shows studied or not, and which
graded steps are still below 2. Those feed the module's practice node.

## Step types

Every step runs over the play at a moment or a segment, so every type is "the rink plus one
question". Types marked *later* need something that does not exist yet.

| Type | What the user does | Graded by | From |
| --- | --- | --- | --- |
| `watch` | Plays a segment with captions, stops at a beat | Not graded | Rink Replay captions |
| `card` | Reads the principle or the cue | Not graded | Vault lesson |
| `choice` | Picks one of 2 to 4 options | Authored grade per option | chalktalk `choice` |
| `spot` | Taps where their player should be at a frozen moment | Distance to the drafted position, two radii | chalktalk `spot`, Rink Replay `compare` |
| `pick` | Taps a player: their man, the open teammate, the pinching D | Authored answer with partials | chalktalk `pick` |
| `freeze` | Taps Go during playback at the moment to leave | Time window around the drafted moment | chalktalk's planned "freeze the frame"; fits L2 and L4 ("leave on the pass, not the arrival") |
| `order` | Taps the three forwards in F1, F2, F3 order | Distances to the carrier at that moment, so a drafted play gives one right answer | chalktalk `order` |
| `contrast` | Flips between the play and its wrong variant | Not graded | Vault "what wrong looks like" |
| `route` *later* | Drags a route for their player from the frozen moment | End zone, which side of a defender it passes, speed under 8 m/s | chalktalk's planned "draw your route", Rink Replay orders |
| `predict` *later* | Taps where the puck goes next | Authored answer | chalktalk's planned "predict the play" |
| `recall` *later* | Fills the missing word in the principle or picks the cue for a situation | Authored | Duolingo-style sentence completion, with the glossary |

Grading is 0, 1 or 2, as in chalktalk. Every wrong or close answer gets feedback that says what it
costs on the ice, in role language. The right answer is also shown as motion: the ghost skates the
drafted route, or the play continues from the right choice.

## Feedback on the rink

The desktop app already draws a suggestion as a shaded target zone, a ghost on the better route and an
arrow from where you were. A `spot` answer is the same picture with your tap as "actual". A `choice`
answer plays the variant. A `freeze` answer shows two markers on the event lane, yours and the drafted
moment, with the gap in tenths of a second. Below the rink, the facts card gives the same spatial
facts the Claude review reads (zone, strong or weak side, above or below the puck, distance to the
nearest opponent, open lanes), for your answer and the model answer side by side.

## Progress and coming back

- **Lesson state**: locked, open, studied. Nothing else.
- **Attempts**: one row per graded step answered, with the grade and when. Permanent step ids, so
  content can be reworded without losing history. This is chalktalk's `attempts` table. Attempts
  exist for practice selection and for your own review, not for a score.
- **Practice**: a module's practice node draws 6 to 8 questions, favoring steps with a recent grade
  below 2, then the oldest attempts. Practice can draw from the whole course once Module 2 is done.
- **Saturday check-in**: one screen per cue studied so far: "Did you catch yourself doing it?" with
  yes, no and a one-line note. It is a log, not a score, and it exports as the `· L1 ✓` line the
  session log in the vault uses, for as long as that log lives there.
- **Storage**: browser storage (IndexedDB) while the experience is being tuned, exported as JSON.
  User accounts come later, once the lessons feel right. To keep that cheap, progress is append-only
  attempt rows (step id, grade, answered at) and check-in rows behind one `ProgressStore` interface.
  An account backend is a second implementation of that interface plus a one-time upload of the local
  rows. The lesson player never knows which one it is talking to.

## Screens

- **Path** (home): modules top to bottom, nodes marked locked, open or studied, the next lesson
  pulled out as a card with its principle and "10 min".
- **Lesson**: the rink fills the top of the screen. On a phone a bottom sheet holds the step (caption,
  question, options, feedback) at about half height, collapsible, scrolling behind. On a desktop the
  same sheet is a right panel. A thin step progress bar sits on top. The event lane sits under the
  rink only in `watch` and `freeze` steps.
- **Result**: at the end of a lesson, the cue and the steps worth revisiting.
- **Practice**: the same lesson screen, with mixed steps.
- **Check-in**: a short form over the week's cues.
- **Library** (later): own plays from the desktop app, viewed and eventually edited here.

Mobile web first: a phone browser is the target device, portrait, touch, safe-area insets, and the
rink fits the screen width. Desktop gets the same layout with more room. A Tauri wrapper for a native
app, as chalktalk has for iOS, is a possible later step and is out of scope now. Nothing in the plan
should assume it: no native plugins, no app store, storage that works in a browser.

## Content model

The repo is the source. The vault is not. Each lesson is a Markdown file for the prose and a JSON
file for the interaction, with the same stem, under `curriculum/`. The curriculum ships with the app.

```text
curriculum/
  course.json                                  module order, lesson order, the check-in text
  lessons/01-dzone-strong-side-wall.md         frontmatter + prose: principle, cue, what wrong looks like, self-check
  lessons/01-dzone-strong-side-wall.steps.json the steps
  plays/01-dzone-strong-side-wall.rink.json
  plays/01-dzone-strong-side-wall.wrong.rink.json
```

Markdown holds what a person reads and edits in sentences. JSON holds what the player executes and
the tests check. A `card` step points at a Markdown section by heading, so the prose is written once.
The transcript is not written at all: it is the play's captions, rendered by the app, as the vault
tables already were through `rink transcript --md`.

```markdown
---
id: l1-dzone-strong-side-wall
module: dzone
title: Puck in my corner (strong side)
play: 01-dzone-strong-side-wall
wrong: 01-dzone-strong-side-wall.wrong
---
## Principle
The strong-side winger owns the point. Be inside their D before the pass goes there.

## Cue
When the puck goes into my corner, look up at my point man, not at the battle.

## What wrong looks like
Drag yourself to the wall below the puck, next to our RD, as if helping in the corner. ...

## Self-check
At 2.5 s, were you between the puck and their LD, or below the puck? ...
```

```jsonc
// curriculum/lessons/01-dzone-strong-side-wall.steps.json
{
  "schema": 1,
  "lesson": "l1-dzone-strong-side-wall",
  "steps": [
    { "id": "l1-principle", "type": "card", "section": "Principle" },
    { "id": "l1-watch-1", "type": "watch", "from_ms": 0, "to_ms": 2000 },
    { "id": "l1-pick-man", "type": "pick", "at_ms": 2000, "prompt": "Who is your man right now?",
      "answer": "away_LD", "partial": ["away_C"],
      "fb": { "right": "...", "partial": "...", "wrong": "..." } },
    { "id": "l1-watch-2", "type": "watch", "from_ms": 2000, "to_ms": 2500 },
    { "id": "l1-choice-dp", "type": "choice", "at_ms": 2500, "prompt": "...",
      "options": [ { "text": "...", "grade": 2, "fb": "..." }, { "text": "...", "grade": 0, "fb": "...", "play": "wrong" } ] },
    { "id": "l1-spot-dp", "type": "spot", "at_ms": 2500, "prompt": "Where should you be?",
      "target": [-22.0, -11.5], "r1_m": 2.0, "r2_m": 4.0, "fb": { "right": "...", "close": "...", "wrong": "..." } },
    { "id": "l1-watch-3", "type": "watch", "from_ms": 2500, "to_ms": 4500 },
    { "id": "l1-contrast", "type": "contrast", "at_ms": 2500, "section": "What wrong looks like" },
    { "id": "l1-cue", "type": "card", "section": "Cue" }
  ]
}
```

Rules carried over from chalktalk: ids are permanent and kebab-case; anything filtered on is a field,
everything read whole is nested; a `schema` version on every JSON file; a test builds every lesson
and checks every reference (play exists, `at_ms` is within the play, every `section` exists in the
Markdown, a `spot` target is on the ice and reachable from the real position, as the suggestion check
already enforces).

Moving in: the four existing vault notes split into eight lesson Markdown files, the steps are written
by hand for L1 to L4 to find the right rhythm, and from L5 on a `draft-lesson` skill beside
`draft-play` writes both files and the wrong variant from a description, then runs the check. The
vault's `Theory` notes become a pointer to the app, or go.

## Standalone, no backend

The web app is the whole experience on its own. It is a static site: the core as wasm, the Svelte
shell, and `curriculum/` shipped in the bundle. Grading runs in the browser. Progress and attempts
live in browser storage (IndexedDB) and export as JSON. No request leaves the page after it loads.

To make that hold on a phone, the app ships a web app manifest and a service worker, so it can be
added to the home screen and works offline. Installed pages are also exempt from iOS Safari's
seven-day storage eviction, which otherwise can wipe progress on a site not visited for a week.

What needs a backend, all of it after the experience is tuned: user accounts and sync across devices
(planned, Phase 6), Claude reviewing the user's own retraced plays (an LLM call; today done through
the CLI on the desktop), or several people authoring content without a redeploy.

## Where the logic lives

`rink-core` stays a crate with no idea that lessons exist. It knows plays, sampling, facts, the
suggestion check and the scene. A turn-based game or any other product sits beside the lesson code,
not inside it.

```text
rink-core          plays, sample, facts, review check, scene, transform
  ├── rink-app     GPUI editor (desktop)
  ├── rink-cli     rink: facts and reviews for agents
  ├── rink-lesson  lesson and step model, grading, practice selection, progress model (pure Rust)
  │     └── rink-wasm   rink-core + rink-lesson for the browser
  │           └── web   Svelte: path, lesson player, sheet, check-in
  └── rink-game    later: a turn-based game over the same plays
```

`rink-lesson` is new. Grading that measures anything (`spot` distances, `order` by distance to the
carrier, `route` legality, `freeze` windows against the play's events) belongs there, because it is
the same geometry the review uses and should not be rewritten in TypeScript. Table lookups (`choice`,
`pick`) live there too, so there is one registry of step types, one grade scale and one test suite.
Svelte renders steps and collects answers; it never computes a grade. This is chalktalk's
"pure logic in `types/<type>.ts`" rule, moved into Rust so the CLI can grade a lesson file in a test and
so the desktop app could run a lesson later without a browser.

The step registry is one `enum Step` with a variant per type, and one `fn grade(step, answer, play)
-> Graded { grade, feedback, model }`. Adding a type adds a variant, its grading, and a Svelte
component. Nothing else changes.

## Phases

1. **Lesson player** with `card`, `watch`, `choice`, `spot`, `contrast`. `rink-lesson` with the
   lesson file, grading for `spot` and `choice`, and the check. L1 to L4 authored from the vault notes
   and the drafted plays, with their wrong variants. No path yet: a list of four lessons.
2. **Path and progress.** Modules, locking, attempts in browser storage, the result screen, export.
3. **Practice and check-in.** `pick`, `freeze` and `order`. The practice node and its selection rule.
   The Saturday check-in and its log line.
4. **Authoring.** The `draft-lesson` skill, `rink lesson check`, L5 to L9.
5. **Later types** (`route`, `predict`, `recall`) and the library of own plays.
6. **User accounts and sync.** After the experience is tuned on browser storage. A second
   `ProgressStore` over a backend, and an import of the local rows on first sign-in.

Status: Phase 1 landed 2026-11-01 in `web/`. Phase 2 landed 2026-11-02: the course home, the topic path
with locking, attempts in browser storage, the result screen and export. The rink in a lesson is portrait,
the user's own net at the bottom, cropped to the ice the user's player and the puck cover.

Phase 1 is the one to feel in the hand before anything else. If a lesson is not clear on a phone with
L1, the path will not fix it.

## Decided

- **No gamification.** No streaks, scores, rings, XP or reminders. Progress is studied or not, plus the
  attempts that drive practice. Retention is not a goal of this app right now.
- **The repo is the content source.** Markdown for prose, JSON for steps, plays as `.rink.json`. The
  vault is not a source.
- **Mobile web first.** Built and tested on a phone browser before the desktop layout gets attention.
  A native wrapper (Tauri) is a later option, not part of this plan.
- **Spot grading.** A tap within 2 m of the drafted position is right, within 4 m is close, further is
  wrong. Positions are placed by hand, so the radii are forgiving. A step can override them.
- **One winger course, drawn from the right wing.** The reads are mirror images on the left wing, so
  there is no left-wing course and no mirroring. What differs between wingers is which way they
  shoot: a `## Shot` section in a lesson carries a right-shot and a left-shot paragraph, and the
  player shows the one for the hand set on the home screen (decided 2026-11-04).
