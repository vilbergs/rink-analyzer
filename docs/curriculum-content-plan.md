# Curriculum content: less jargon, more teaching

A review of `curriculum/` (L1 to L4, their plays and captions) written 2026-11-02, with a plan
for rewriting the content and for a swamp pipeline that keeps new lessons to the same standard.
It changes no code. The step engine, the check and the player stay as they are; everything here
is content, a style guide, and swamp.

## What the review found

The four lessons are structurally sound: one play, one decision, a wrong variant that shows the
cost, feedback that says what a wrong choice costs on the ice. That is the right skeleton. The
problems are in the words.

1. **The first thing a learner reads is the densest sentence in the lesson.** Every lesson opens
   with the Principle card: "The strong-side winger owns the point. Be inside their D before the
   pass goes there." Four terms (strong side, owns, the point, inside their D) and no play yet.
   Research on multimedia learning (Mayer's pretraining principle) says the names and
   characteristics of the key things should be known before the lesson runs; Brilliant's rule is
   to interact before you read. Both say: show the rink first, name things on it, then state the
   idea.
2. **Terms are used before they are ever explained, and nothing in the course explains them.**
   Inventory of the four lessons, captions included: point, point man, slot, high slot, hash marks,
   wall, strong side, weak side, above/below the puck, inside him, inside shoulder, lane, shot lane,
   cross-ice lane, D-to-D, rim, pinch, chip it off the glass, forecheck, back door, outlet, middle
   lane, 3-on-2, F2, zone exit, rush, walk the line, puck-watching, your man. None has a definition
   anywhere. The design note mentions a glossary; it does not exist yet.
3. **Naming is inconsistent.** L4 calls the same opponent "their LW" in the question and "their F2"
   in the captions and self-check. L1 uses "their LD", "my point man" and "their D" for one player.
   The rink shows that player as a red badge reading "LD". The text should say what the badge says.
4. **The right answer is the longest option.** L1's decision: the right option is 13 words with
   three terms, the wrong ones are shorter. L3 and L4 have the same shape. Test-wise learners pick
   the long one; everyone else reads a paragraph per option on a phone. Options should be short,
   parallel and free of the reasoning, which belongs in the feedback.
5. **Prose is anchored to seconds.** "At 2.5 s, were you between the puck and their LD?" Nobody
   remembers a play by its millisecond clock. The play has events (the pass to the point, the
   D-to-D pass). Prose should anchor to those; the JSON keeps the milliseconds.
6. **Feedback is good but uneven.** The best feedback says what happens next and why ("He catches
   it with time to look, and shoots through the lane you are still skating into"). The weakest
   restates the right answer with more jargon ("Above the puck, inside their LD, in the lane up
   the wall"). The evidence on feedback (elaborated feedback beats "right/wrong" and beats "here
   is the right answer", especially for low prior knowledge; gains plateau as it gets longer) says:
   always explain, keep it to two or three sentences.
7. **Questions ask what to do before asking what you see.** Teaching Games for Understanding and
   the house rule for this course (rec hockey, no set tactics: train reads of what the other nine
   skaters are doing) both want a "what are they doing?" question before "what do you do?". L1's
   "Who is your man?" and L3's "What were you doing before it got there?" are close; L2 and L4 go
   straight to the decision.
8. **Small things.** Lesson titles carry jargon in brackets ("Puck in my corner (strong side)").
   Opponents are "he" throughout; the Saturday game is mixed and the reader is not a native
   English speaker, so neutral, plain wording costs nothing. Captions run to 25 words at 0.5x
   speed. The Self-check card is a reading task placed after the contrast, where a retrieval
   question would do more.

## Principles for the rewrite

Each one comes from a source worth trusting, cited at the end.

- **Name things on the rink before using the name.** A term is taught by tapping it on the rink,
  not by reading a definition. (Pretraining and signaling, Mayer; learn by doing, Brilliant.)
- **Read, then decide.** The first question in a lesson is about what the opponents and
  teammates are doing. The decision comes second. (TGfU questioning: what, where, when, why.)
- **Every answer gets a reason, and the reason is short.** Three parts: the verdict in a few
  words, what happens next on the ice, the read that was missed. Thirty-five words or fewer.
  (Elaborated feedback meta-analyses; Hattie and Timperley's task and process levels.)
- **Options are short and parallel.** Six to ten words, same grammatical shape, no reasoning.
  Three options is enough; a fourth rarely adds discrimination.
- **Anchor to beats, not seconds.** "When the pass leaves for the point" instead of "at 2.5 s".
- **One name per player, the one on the badge.** "their LD", "our C". Never "point man", "my man",
  "F2", "their D".
- **The cue is a detection cue.** Where is the pressure, where is the help, what is the opponent
  doing. L4's "hold, hold, puck moves, go" is the model; L1's "look up at my point man, not at the
  battle" is right in shape and wrong in words.
- **Retrieval at the end, not re-reading.** Close with one question answered from memory, then
  the cue. (Practice testing is one of the two high-utility techniques in Dunlosky et al.)

## The vocabulary, in three tiers

**Tier A: taught once, in a new Lesson 0, then used freely.** These are what teammates say on the
bench and what the facts table already shows. Each is a spot on the rink, so Lesson 0 teaches
them with the existing `spot` step: "Tap the point", "Tap the slot", "Tap your wall".

| Term | Plain gloss for the glossary | How Lesson 0 teaches it |
| --- | --- | --- |
| the point | just inside our blue line, where their defenders stand | tap it |
| the slot | the ice in front of our net, between the circles | tap it |
| the wall (boards) | the side of the rink; "my wall" is the right-hand side when our net is at the bottom | tap it |
| hash marks | the short lines beside the faceoff circles; "stopped at the hash marks" | tap them |
| strong side, weak side | the half of the rink the puck is in, and the other half | tap which side you are on |
| above, below the puck | further from our net than the puck, or closer; on the phone, higher or lower on the screen | tap a player above the puck |
| a lane | the straight line a pass or shot would travel | tap the lane from puck to point |
| inside | between an opponent and our net | tap the inside spot |

**Tier B: taught in the lesson that first needs it, with a short "Words" card.** pinch, rim and
forecheck (L3); D-to-D (L2); chip off the glass and 3-on-2 (L3); outlet and middle lane (L4);
zone exit (L3). The Words card is a `card` step over a new `## Words` section, two or three terms,
one line each, written for the situation the lesson is about.

**Tier C: replaced with plain words.** point man → their LD at the point; your man → the player
you are watching; inside shoulder → between them and the net; own the point → the point is your
job; walk the line → skate along the blue line; back door → the far post; puck-watching → watching
the puck instead of your player; F2 → their LW; sits on you, on my hip → stays right next to you;
eats it, throws it blind → gets hit, or passes without looking; the middle is dead → the middle is
covered; level with → beside; in stride → without slowing down.

## Lesson shape after the rewrite

Ten or eleven steps, within the check's 7 to 11. New steps replace, they do not pile on.

1. `card` Principle, rewritten as a situation and a read, not a rule. L1: "The puck is in my
   corner. Their defender at the point is waiting for a pass. Stand between the puck and that
   defender, so the pass is never easy."
2. `card` Words (Tier B only; L1 has none and skips this).
3. `watch` to the first beat.
4. `choice` Read: what are they doing? L1: "Their C has the puck in the corner. What are they
   looking for?" Options: a pass to the point, a shot from the corner, a pass across the slot.
5. `watch` to the decision.
6. `choice` Decide, short parallel options. L1: "Where are you when the pass leaves?" Options:
   "Between the puck and their LD" / "At the hash marks, ready either way" / "On the wall, helping
   in the corner" / "In front of our net".
7. `spot` the same moment.
8. `watch` the outcome.
9. `contrast` What wrong looks like.
10. `card` Check yourself: one retrieval question, anchored to a beat, with the answer on the next
    line after a blank paragraph so the card shows both. (A `recall` step type would do this
    better; until it exists the card works.)
11. `card` Cue.

### L1 before and after

| Piece | Now | Proposed |
| --- | --- | --- |
| Title | Puck in my corner (strong side) | Puck in my corner |
| Principle | The strong-side winger owns the point. Be inside their D before the pass goes there. | The puck is in my corner. Their defender at the point is waiting for a pass. Be between the puck and that defender before the pass goes. |
| Right option | Already inside him, above the puck, stick in the lane up the wall | Between the puck and their LD |
| Its feedback | That's the job. You were between the puck and the point before the pass went, so you push while it travels and meet him a stick length off on his inside shoulder. He can't walk to the middle, and his shot goes through a half-closed lane. | Right. You were already between the puck and their LD, so you move as the pass travels and arrive as it does. Their shot has to go through you. |
| Wrong feedback | That's our RD's battle. From below the puck you can't reach the pass: their LD gets it clean with the whole top of the zone, and you chase uphill while he picks his shot. | That corner is our RD's job. From there you are ten meters from their LD. The pass is clean, and they pick a shot while you chase. |
| Caption at 2.5 s | Pass to the point. I push now, because I'm already inside him. | Pass to the point. I move now: I'm already between the puck and their LD. |
| Self-check | At 2.5 s, were you between the puck and their LD, or below the puck? What does the lane to the point look like from where you are? | When the pass left for the point, who were you between? (The puck and their LD. From there the pass lane runs through you.) |
| Cue | When the puck goes into my corner, look up at my point man, not at the battle. | Puck in my corner: find their defender at the point, not the fight for the puck. |

The same table should be written for L2 to L4 before any file changes, so the rewrite is reviewed
as a whole. L2's "stick in the cross-ice lane", "back door" and "D-to-D"; L3's "rim", "pinch",
"chip it off the glass", "in stride"; L4's "F2", "the middle is dead", "sits on you" are the
heaviest spots.

## Content files to add

- `curriculum/glossary.json`: `{ term, gloss, tier, lesson }`, where `lesson` is where the term is
  first taught. Tier A terms point at Lesson 0. The same file later feeds a tap-to-define in the
  player and the `recall` step type; for now it is the lint's source of truth.
- `curriculum/lessons/00-the-rink.md` and `.steps.json`: Lesson 0, "Reading the rink", in a new
  module `basics` before `dzone`. Six to eight `spot` steps over a static play
  (`curriculum/plays/00-the-rink.rink.json`, one keyframe per player, `me` marked, puck in the
  right corner so strong side, above and below have an answer), each with an explicit `target`
  and generous radii (3 m and 6 m). No wrong variant. Every step the engine needs already exists.
- `curriculum/STYLE.md`: the rules above as a checklist, so the `draft-lesson` skill and the lint
  agree on what good looks like.
- A `## Words` section in the lessons that need Tier B terms, and the rewritten sections and
  steps for L1 to L4, plus rewritten captions in the eight play files (through `rink caption`).

## The swamp pipeline

The point of doing this in swamp is that lesson five is checked the same way as lesson one, by
the same workflow the deploy already runs, with the results kept as data so a rewrite can be
compared with the version before it.

No published extension covers this. `swamp extension search` for curriculum, markdown, lint and
readability returns nothing, and `swamp behavior type search` finds no local type, so a custom
behavior is the right primitive (architecture guide: no type covers it; the existing
`command/shell` steps in `web-build` are the ad-hoc exception, not the pattern to grow).

### 1. Behavior type `rink/curriculum` in `extensions/behaviors/curriculum.ts`

One behavior, `curriculum`, pointed at `curriculum/`. Methods, each producing one data item:

| Method | Reads | Produces | Rule it enforces |
| --- | --- | --- | --- |
| `inventory` | lessons, steps, plays, glossary | `inventory`: per lesson, the terms used in each section, step and caption, with counts | nothing; it is the data the others and the report read |
| `lint` | `inventory` via `data.latest`, `glossary.json`, `STYLE.md` rules as constants | `issues`: `{ lesson, step, rule, text }` | glossary term used before the lesson that teaches it; Tier C term present; option longer than 10 words or options whose lengths spread by more than 2x; reasoning words ("because", "so") inside an option; seconds in prose (`\d+(\.\d+)? s`); caption over 14 words; feedback over 35 words; player named by anything but a badge name; F1/F2/F3; "he/him" when the style says neutral |
| `check` | the repo | `check`: the structural result | runs the same content checks as `cargo test -p rink-lesson` through Deno's command API, so the workflow has one status to assert on |
| `variants` | main plays | the `.wrong.rink.json` files, and `variants` data listing what changed | replaces `scripts/gen-wrong-variants.py` once the generator moves into the type; until then this method is omitted and the script stays |

Pin every `npm:` import, zod from `npm:zod@4`, and keep the lint rules as data (an array of
`{ id, pattern, message, scope }`) so adding a rule is adding a row.

### 2. Workflow `curriculum-check`

```yaml
name: curriculum-check
description: Inventory the curriculum, lint it against the style guide and glossary, run the structural check, and fail on any issue.
jobs:
  - name: inventory
    steps:
      - name: inventory
        task: { type: behavior, behaviorType: rink/curriculum, behaviorName: curriculum, methodName: inventory }
  - name: lint
    dependsOn: [{ job: inventory, condition: { type: succeeded } }]
    steps:
      - name: lint
        task: { type: behavior, behaviorType: rink/curriculum, behaviorName: curriculum, methodName: lint }
      - name: clean
        dependsOn: [{ step: lint, condition: { type: succeeded } }]
        task:
          type: assert
          expr: size(data.latest("curriculum", "issues").attributes.issues) == 0
          message: "the curriculum has style issues; run `swamp report get curriculum-summary`"
  - name: structure
    dependsOn: [{ job: inventory, condition: { type: succeeded } }]
    steps:
      - name: check
        task: { type: behavior, behaviorType: rink/curriculum, behaviorName: curriculum, methodName: check }
```

Then `web-build` gains a `curriculum` job that nests `curriculum-check` before `wasm`, the way
`deploy-pi` nests `web-build`. A lesson that uses "point man" cannot reach rink.vilberg.dev.

### 3. Report `curriculum-summary` in `extensions/reports/curriculum-summary.ts`

Runs after `lint` and `inventory`. Per lesson: terms used, terms taught, issues by rule, and the
change against the previous `issues` version (data is versioned, so "L2 went from 9 issues to 0"
is one comparison). This is what a rewrite is reviewed against, and what `draft-lesson` prints
after writing a new lesson.

### 4. Authoring loop for lesson five and on

1. The `draft-lesson` skill (Phase 4 in the experience note) writes the Markdown, the steps, the
   play and the wrong variant, reading `STYLE.md` and `glossary.json` first.
2. It runs `swamp workflow run curriculum-check`.
3. On failure it reads `swamp report get curriculum-summary --json`, fixes the named steps, and
   runs again. New terms go into `glossary.json` with the lesson that teaches them, or get
   rewritten in plain words.
4. `swamp workflow run deploy-pi` ships it.

## Order of work

1. Write `STYLE.md` and `glossary.json`. Half a day, content only. Everything else checks
   against them.
2. Build the `rink/curriculum` behavior with `inventory` and `lint`, and the `curriculum-check`
   workflow. Run it on today's content to get the baseline issue list; it should be long.
3. Lesson 0 over a static play, through the existing `spot` step.
4. Rewrite L1 fully (prose, steps, captions in both plays), review it on a phone, then L2 to L4
   to the same template. Each rewrite is one commit that takes its lesson's issues to zero.
5. Add the report, nest `curriculum-check` into `web-build`.
6. Only then the player changes that content alone cannot make: tap-to-define from the glossary,
   a `recall` step, and a "Words" style for Tier B cards. These are for the agents working on
   `web/` and `rink-lesson`, and nothing above waits on them.

## What this plan does not do

- It does not change the step engine, the check in `rink-lesson`, or the player. The new lesson
  shape uses only `card`, `watch`, `choice`, `spot` and `contrast`.
- It does not add scheme names, formations or F1/F2/F3 labels. The course trains reads of what
  individual players are doing, which is what a Saturday game with no agreed tactics rewards.
- It does not add scores, streaks or any motivational mechanic.

## Sources

- Mayer, R. E. Multimedia learning principles: pretraining, segmenting, signaling.
  https://lucidea.com/blog/mayers-12-principles-of-multimedia-learning and
  https://twut.nd.edu/PDF/Principles-MultimediaLearning.pdf
- Van der Kleij et al. (2015), meta-analysis of feedback in computer-based learning: elaborated
  feedback 0.49 vs. correct answer 0.32 vs. right/wrong 0.05.
  https://www.ati-online.com/pdfs/researchK12/ABriefReviewLiteratureonTestFeedback.pdf
- Network meta-analysis of computer-based feedback strategies: elaborated feedback is the only
  type that helps low prior knowledge learners. https://hal.in2p3.fr/LARAC/hal-04607898v1
- Hattie and Timperley (2007), feedback at the task, process and self-regulation levels.
  https://www.bera.ac.uk/blog/how-to-optimise-the-use-of-hattie-and-timperleys-feedback-levels-for-student-learning
- Dunlosky et al. (2013), practice testing and distributed practice as the high-utility
  techniques. https://www.schreyerinstitute.psu.edu/pdf/SixStrategiesEffectiveLearning.YanaWeinstein.2017.pdf
- Worked examples and the expertise reversal effect: guidance first, then fade it.
  https://en.wikipedia.org/wiki/Expertise_reversal_effect
- Teaching Games for Understanding, questioning as what, where, when, why.
  https://us.humankinetics.com/blogs/excerpt/teaching-tactical-awareness-and-skill-acquisition
- Perceptual-cognitive training in team sports, 2024 meta-analysis: gains in anticipation and
  decision making; transfer to play is the open question.
  https://www.ncbi.nlm.nih.gov/pmc/articles/PMC11505547/
- Brilliant, "learn by doing": interact before you read. https://brilliant.org/help/why-brilliant
- Plain hockey glossaries used for the Tier A glosses: https://thehockeywriters.com/?p=133981 and
  https://howtohockey.com/the-rink-within-the-rink-using-the-markings/
