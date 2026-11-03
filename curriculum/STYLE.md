# Writing a lesson

The reader plays recreational hockey on Saturdays, has no coach, and reads English as a second
language. Every sentence is read on a phone between two taps. These rules are what the lint in
`extensions/behaviors/curriculum.ts` checks; the reasoning is in `docs/curriculum-content-plan.md`.

## Words

- A hockey term is used only after it has been taught. `glossary.json` says where each term is
  taught: Tier A in Lesson 0, Tier B by the Words card of the lesson named in `taught_in`.
  Tier C terms are never used; the glossary names the plain words that replace them.
- A new Tier B term gets a row in the glossary and a line in the lesson's `## Words` section.
- Players are named as their badge on the rink reads, with whose they are: "their LD", "our C",
  "their LW". Never "point man", "my man", "F2", "their D".
- Opponents and teammates are "they". The Saturday game is mixed.
- Prose anchors to what happens, not to the clock: "when the pass leaves for the point", not
  "at 2.5 s". Milliseconds live in the JSON only.

## Shape of a lesson

Ten or eleven steps, in this order. Steps replace; they do not pile on. The first question comes
before the Principle: a learner who has just guessed what their LD is looking for reads the
Principle as the answer to something, not as a rule to memorize. (Brilliant's rule for the same
reason: "We don't teach how to do something before asking questions.")

1. `card` **Words**: only when the lesson teaches Tier B terms. One line per term.
2. `watch` to the first beat.
3. `choice` **the read**: what are they doing, what are they looking for. Getting it wrong is
   fine; the Principle follows.
4. `card` **Principle**: the situation, what they are trying to do, and what to do about it.
   Three sentences or fewer, under 45 words. No rule-speak ("the winger owns the point").
5. `watch` to the decision.
6. `choice` **the decision**: where are you, what do you do.
7. `spot` at the same moment.
8. `watch` the outcome.
9. `contrast` **What wrong looks like**.
10. `card` **Check yourself**: one question answered from memory, then the answer in its own
    paragraph.
11. `card` **Cue**: one line for the ice.

A lesson with no question, such as Lesson 0, starts with the Principle.

## Questions

- A prompt is one or two sentences. Say what is happening, then ask.
- Three or four options. Each is a short phrase, six to ten words, the same grammatical shape as
  the others, with no reasoning in it. The reasoning goes in the feedback. The right option must
  not be the longest.
- Exactly one option is graded 2. A graded-1 option is a reasonable idea that is late or costs
  something. A graded-0 option that plays the wrong variant carries `"play": "wrong"`.

## Feedback

Three parts, 35 words or fewer: the verdict in a few words, what happens next on the ice, the
read that was missed or made.

> Right. You were already between the puck and their LD, so you move as the pass travels and
> arrive as it does. Their shot has to go through you.

> That corner is our RD's job. From there you are ten meters from their LD. The pass gets there
> with nobody in the way, and they pick a shot while you chase.

## Captions

A caption is what you would say out loud while the play runs at half speed: 14 words or fewer,
one thing that happens and where I am. The wrong variant's captions say what it costs.

## Cue

One line, 15 words or fewer, that starts with the situation and ends with what to look for.
"Puck in my corner: find their defender at the point, not the fight for the puck."

## Sections

`## Principle` and `## Cue` are required by the engine. `## What wrong looks like` is required
when the lesson has a wrong variant. `## Check yourself` is the retrieval card. `## Words` is
optional. Nothing else.

## Checking

`swamp workflow run curriculum-check` runs the inventory, the lint and the structural check.
`swamp report get @vilbergs/curriculum-summary --model curriculum` shows the issues per lesson
and the change since the last run. `cargo test -p rink-lesson --test curriculum` is the engine's
own check and runs inside `web-build`.
