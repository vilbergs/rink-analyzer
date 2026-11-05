import { assertEquals } from "jsr:@std/assert@1.0.13";
import { catalog, type Curriculum, inventory, type Lesson, lint, parseMarkdown } from "./content.ts";

const glossary = {
  schema: 1,
  terms: [
    { term: "the point", tier: "A" as const, taught_in: "l0-basics", match: ["point"] },
    { term: "pinch", tier: "B" as const, taught_in: "l3-later", match: ["pinch", "pinches"] },
    { term: "point man", tier: "C" as const, match: ["point man"], replace_with: "their LD at the point" },
  ],
};

function lesson(over: Partial<Lesson> = {}): Lesson {
  const play = { id: "p", duration_ms: 3000, entities: [{ id: "home_RW", is_me: true }], events: [{ t_ms: 2000, kind: "decision_point" }], captions: [] };
  return {
    id: "l1-one",
    stem: "01-one",
    module: "m",
    title: "One",
    play: "01-one",
    wrong: "01-one.wrong",
    sections: { "Principle": "Short.", "Cue": "Look up.", "What wrong looks like": "Bad." },
    steps: [
      { type: "watch", id: "l1-watch", from_ms: 0, to_ms: 2000 },
      {
        type: "choice",
        id: "l1-choice",
        at_ms: 2000,
        prompt: "Where are you?",
        options: [
          { text: "Between the puck and their LD", grade: 2, fb: "Right." },
          { text: "At the hash marks, ready either way", grade: 1, fb: "Late." },
          { text: "In front of our net", grade: 0, fb: "Far.", play: "wrong" },
        ],
      },
      { type: "card", id: "l1-principle", section: "Principle" },
      { type: "spot", id: "l1-spot", at_ms: 2000, prompt: "Tap.", fb: { right: "r", close: "c", wrong: "w" } },
      { type: "contrast", id: "l1-contrast", at_ms: 2000, section: "What wrong looks like" },
      { type: "card", id: "l1-cue", section: "Cue" },
    ],
    main: play,
    variant: { ...play, id: "q" },
    ...over,
  };
}

function curriculum(lessons: Lesson[]): Curriculum {
  return {
    catalog: { schema: 1, courses: [{ id: "c", title: "C", modules: [{ id: "m", title: "M", lessons: lessons.map((l) => l.id) }] }] },
    glossary,
    lessons,
    order: ["l0-basics", ...lessons.map((l) => l.id), "l3-later"],
  };
}

const rules = (c: Curriculum) => lint(c).map((i) => i.rule).sort();

Deno.test("a clean lesson has no issues", () => {
  assertEquals(rules(curriculum([lesson()])), []);
});

Deno.test("a term used before the lesson that teaches it is flagged", () => {
  const c = curriculum([lesson({ sections: { "Principle": "Their LD pinches.", "Cue": "x", "What wrong looks like": "y" } })]);
  assertEquals(rules(c), ["term-before-taught"]);
});

Deno.test("a tier C term is flagged with its replacement", () => {
  const c = curriculum([lesson({ sections: { "Principle": "Watch the point man.", "Cue": "x", "What wrong looks like": "y" } })]);
  const found = lint(c);
  assertEquals(found.map((i) => i.rule), ["term-banned"]);
  assertEquals(found[0].text, '"point man" is not used; write "their LD at the point"');
});

Deno.test("clock times and pronouns are flagged", () => {
  const c = curriculum([lesson({ sections: { "Principle": "At 2.5 s he shoots.", "Cue": "x", "What wrong looks like": "y" } })]);
  assertEquals(rules(c), ["clock-in-prose", "pronoun"]);
});

Deno.test("the right option must not be the longest", () => {
  const l = lesson();
  const choice = l.steps[1];
  if (choice.type !== "choice") throw new Error("fixture");
  choice.options[0].text = "Already inside them, above the puck, in the lane";
  assertEquals(rules(curriculum([l])), ["right-is-longest"]);
});

Deno.test("a spot off a decision point needs an explicit target", () => {
  const l = lesson();
  const spot = l.steps[3];
  if (spot.type !== "spot") throw new Error("fixture");
  spot.at_ms = 1000;
  assertEquals(rules(curriculum([l])), ["spot-off-decision"]);
  spot.target = [-9, -12];
  assertEquals(rules(curriculum([l])), []);
});

Deno.test("the principle card follows the first question", () => {
  const l = lesson();
  const [watch, choice, principle, ...rest] = l.steps;
  l.steps = [principle, watch, choice, ...rest];
  assertEquals(rules(curriculum([l])), ["first-step", "principle-after-read"]);
});

Deno.test("a Shot section needs a right-shot and a left-shot paragraph", () => {
  const sections = { "Principle": "Short.", "Cue": "Look up.", "What wrong looks like": "Bad.", "Shot": "Right shot: forehand." };
  const l = lesson({ sections });
  l.steps.splice(1, 0, { type: "card", id: "l1-shot", section: "Shot" });
  const found = lint(curriculum([l]));
  assertEquals(found.map((i) => [i.rule, i.text]), [["shot-paragraphs", "needs a paragraph starting with Left shot:"]]);
  l.sections.Shot = "Right shot: forehand.\n\nLeft shot: backhand.";
  assertEquals(rules(curriculum([l])), []);
});

Deno.test("the inventory counts terms per lesson", () => {
  const c = curriculum([lesson({ sections: { "Principle": "The point. The point.", "Cue": "x", "What wrong looks like": "y" } })]);
  assertEquals(inventory(c)[0].terms, [{ term: "the point", tier: "A", count: 2 }]);
});

Deno.test("markdown frontmatter and sections parse", () => {
  const { meta, sections } = parseMarkdown("---\nid: a\ntitle: \"B\"\n---\n## Principle\nOne.\n\nTwo.\n## Cue\nGo.\n");
  assertEquals(meta, { id: "a", title: "B" });
  assertEquals(sections, { Principle: "One.\n\nTwo.", Cue: "Go." });
});

Deno.test("a course may build only on a course listed before it, and module ids are unique", () => {
  const issues = catalog({
    schema: 1,
    courses: [
      { id: "winger", title: "W", requires: ["fundamentals"], modules: [{ id: "dzone", title: "D", lessons: [] }] },
      { id: "fundamentals", title: "F", modules: [{ id: "dzone", title: "D", lessons: [] }] },
    ],
  });
  assertEquals(issues.map((i) => i.rule), ["course-requires", "module-id"]);
});
