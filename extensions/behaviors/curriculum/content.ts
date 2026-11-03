/**
 * The curriculum as data, and the checks STYLE.md asks for.
 *
 * `loadCurriculum` reads `curriculum/` from disk. Everything else is pure:
 * `surfaces` lists every piece of text a learner reads, `lint` runs the text
 * rules and the structural checks over it, and `inventory` counts what each
 * lesson uses.
 *
 * @module
 */

export type Tier = "A" | "B" | "C";

export type Term = {
  term: string;
  tier: Tier;
  taught_in?: string;
  match: string[];
  gloss?: string;
  replace_with?: string;
};

export type Glossary = { schema: number; terms: Term[] };

export type Module = { id: string; title: string; description?: string; lessons: string[] };

export type Course = { schema: number; title?: string; modules: Module[] };

export type Play = {
  id: string;
  duration_ms: number;
  entities: { id: string; is_me?: boolean }[];
  events: { t_ms: number; kind: string }[];
  captions?: { t_ms: number; text: string }[];
};

export type Option = { text: string; grade: number; fb: string; play?: string };

export type Step =
  | { type: "card"; id: string; section: string }
  | { type: "watch"; id: string; from_ms: number; to_ms: number; play?: string }
  | { type: "choice"; id: string; at_ms: number; prompt: string; options: Option[] }
  | {
    type: "spot";
    id: string;
    at_ms: number;
    prompt: string;
    target?: [number, number];
    r1_m?: number;
    r2_m?: number;
    fb: { right: string; close: string; wrong: string };
  }
  | { type: "contrast"; id: string; at_ms: number; section: string; to_ms?: number };

export type Lesson = {
  id: string;
  stem: string;
  module: string;
  title: string;
  play: string;
  wrong: string | null;
  sections: Record<string, string>;
  steps: Step[];
  main: Play;
  variant: Play | null;
};

export type Curriculum = {
  course: Course;
  glossary: Glossary;
  lessons: Lesson[];
  /** Lesson ids in course order. */
  order: string[];
};

export type Kind =
  | "principle"
  | "words"
  | "wrong"
  | "check"
  | "cue"
  | "section"
  | "prompt"
  | "option"
  | "feedback"
  | "caption";

/** One piece of text a learner reads, and where it lives. */
export type Text = { lesson: string; step: string | null; kind: Kind; where: string; text: string };

export type Issue = { lesson: string; step: string | null; rule: string; where: string; text: string };

const SECTION_KINDS: Record<string, Kind> = {
  "Principle": "principle",
  "Words": "words",
  "What wrong looks like": "wrong",
  "Check yourself": "check",
  "Cue": "cue",
};

const ALLOWED_SECTIONS = Object.keys(SECTION_KINDS);

export const LIMITS = {
  steps: [6, 12] as const,
  options: [3, 4] as const,
  optionWords: 10,
  feedbackWords: 35,
  captionWords: 14,
  principleWords: 45,
  cueWords: 15,
  promptWords: 30,
  optionSpread: 2,
};

// ---------------------------------------------------------------- loading

export async function loadCurriculum(dir: string): Promise<Curriculum> {
  const json = async (path: string) => JSON.parse(await Deno.readTextFile(path));
  const course = await json(`${dir}/course.json`) as Course;
  const glossary = await json(`${dir}/glossary.json`) as Glossary;
  const stems: string[] = [];
  for await (const entry of Deno.readDir(`${dir}/lessons`)) {
    if (entry.isFile && entry.name.endsWith(".md")) stems.push(entry.name.slice(0, -3));
  }
  stems.sort();
  const lessons: Lesson[] = [];
  for (const stem of stems) {
    const md = await Deno.readTextFile(`${dir}/lessons/${stem}.md`);
    const steps = await json(`${dir}/lessons/${stem}.steps.json`) as { steps: Step[] };
    const { meta, sections } = parseMarkdown(md);
    const main = await json(`${dir}/plays/${meta.play}.rink.json`) as Play;
    const variant = meta.wrong ? await json(`${dir}/plays/${meta.wrong}.rink.json`) as Play : null;
    lessons.push({
      id: meta.id,
      stem,
      module: meta.module,
      title: meta.title,
      play: meta.play,
      wrong: meta.wrong ?? null,
      sections,
      steps: steps.steps,
      main,
      variant,
    });
  }
  return { course, glossary, lessons, order: course.modules.flatMap((m) => m.lessons) };
}

/** Frontmatter `key: value` lines between `---` lines, then `## ` sections. */
export function parseMarkdown(md: string): { meta: Record<string, string>; sections: Record<string, string> } {
  const lines = md.split(/\r?\n/);
  const meta: Record<string, string> = {};
  let i = 0;
  if (lines[0] === "---") {
    for (i = 1; i < lines.length && lines[i] !== "---"; i++) {
      const at = lines[i].indexOf(":");
      if (at > 0) meta[lines[i].slice(0, at).trim()] = lines[i].slice(at + 1).trim().replace(/^"(.*)"$/, "$1");
    }
    i++;
  }
  const sections: Record<string, string> = {};
  let current: string | null = null;
  const body: string[] = [];
  const flush = () => {
    if (current !== null) sections[current] = body.join("\n").trim();
    body.length = 0;
  };
  for (; i < lines.length; i++) {
    if (lines[i].startsWith("## ")) {
      flush();
      current = lines[i].slice(3).trim();
    } else if (current !== null) {
      body.push(lines[i]);
    }
  }
  flush();
  return { meta, sections };
}

// --------------------------------------------------------------- surfaces

export function surfaces(lesson: Lesson): Text[] {
  const out: Text[] = [];
  const add = (step: string | null, kind: Kind, where: string, text: string) =>
    out.push({ lesson: lesson.id, step, kind, where, text });
  for (const [name, text] of Object.entries(lesson.sections)) {
    add(null, SECTION_KINDS[name] ?? "section", `## ${name}`, text);
  }
  for (const step of lesson.steps) {
    if (step.type === "choice") {
      add(step.id, "prompt", "prompt", step.prompt);
      step.options.forEach((o, i) => {
        add(step.id, "option", `option ${i + 1}`, o.text);
        add(step.id, "feedback", `option ${i + 1} feedback`, o.fb);
      });
    } else if (step.type === "spot") {
      add(step.id, "prompt", "prompt", step.prompt);
      for (const k of ["right", "close", "wrong"] as const) add(step.id, "feedback", `${k} feedback`, step.fb[k]);
    }
  }
  for (const c of lesson.main.captions ?? []) add(null, "caption", `caption at ${c.t_ms} ms`, c.text);
  for (const c of lesson.variant?.captions ?? []) add(null, "caption", `wrong caption at ${c.t_ms} ms`, c.text);
  return out;
}

export function words(text: string): number {
  return text.trim().split(/\s+/).filter(Boolean).length;
}

// ------------------------------------------------------------- text rules

type Context = { order: string[]; glossary: Glossary; lesson: string };

type TextRule = { id: string; kinds: Kind[] | "all"; check: (t: Text, cx: Context) => string | null };

function escape(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
}

const patterns = new WeakMap<Term, RegExp>();

function pattern(term: Term): RegExp {
  let re = patterns.get(term);
  if (!re) {
    const alts = [...term.match].sort((a, b) => b.length - a.length).map(escape).join("|");
    re = new RegExp(`(?<![\\w-])(?:${alts})(?![\\w-])`, "i");
    patterns.set(term, re);
  }
  return re;
}

function overWords(kind: Kind, limit: number): TextRule {
  return {
    id: `${kind}-words`,
    kinds: [kind],
    check: (t) => {
      const n = words(t.text);
      return n > limit ? `${n} words, the limit is ${limit}` : null;
    },
  };
}

export const TEXT_RULES: TextRule[] = [
  {
    id: "term-banned",
    kinds: "all",
    check: (t, cx) => {
      for (const term of cx.glossary.terms) {
        if (term.tier !== "C") continue;
        const m = pattern(term).exec(t.text);
        if (m) return `"${m[0]}" is not used; write "${term.replace_with}"`;
      }
      return null;
    },
  },
  {
    id: "term-before-taught",
    kinds: "all",
    check: (t, cx) => {
      const here = cx.order.indexOf(cx.lesson);
      if (here < 0) return null;
      for (const term of cx.glossary.terms) {
        if (term.tier === "C" || !term.taught_in) continue;
        const taught = cx.order.indexOf(term.taught_in);
        if (taught < 0 || taught <= here) continue;
        const m = pattern(term).exec(t.text);
        if (m) return `"${m[0]}" is taught in ${term.taught_in}, which comes later`;
      }
      return null;
    },
  },
  {
    id: "clock-in-prose",
    kinds: "all",
    check: (t) => {
      const m = /\b\d+(?:[.,]\d+)?\s?(?:s|sec|seconds|ms)\b/.exec(t.text);
      return m ? `"${m[0]}" is a time on the clock; say what happens instead` : null;
    },
  },
  {
    id: "pronoun",
    kinds: "all",
    check: (t) => {
      const m = /\b(?:he|him|his|himself|she|her|hers|herself)\b/i.exec(t.text);
      return m ? `"${m[0]}": players are they` : null;
    },
  },
  {
    id: "option-reasoning",
    kinds: ["option"],
    check: (t) => /\b(?:because|so that)\b|, so\b|:/.test(t.text) ? "an option states a choice, the feedback gives the reason" : null,
  },
  overWords("option", LIMITS.optionWords),
  overWords("feedback", LIMITS.feedbackWords),
  overWords("caption", LIMITS.captionWords),
  overWords("principle", LIMITS.principleWords),
  overWords("cue", LIMITS.cueWords),
  overWords("prompt", LIMITS.promptWords),
];

// -------------------------------------------------------- structural rules

function structure(lesson: Lesson, c: Curriculum): Issue[] {
  const out: Issue[] = [];
  const at = (step: string | null, rule: string, where: string, text: string) =>
    out.push({ lesson: lesson.id, step, rule, where, text });

  const [minSteps, maxSteps] = LIMITS.steps;
  if (lesson.steps.length < minSteps || lesson.steps.length > maxSteps) {
    at(null, "steps-count", "steps", `${lesson.steps.length} steps, expected ${minSteps} to ${maxSteps}`);
  }
  if (lesson.play !== lesson.stem) at(null, "frontmatter-play", "play", `play is ${lesson.play}, the stem is ${lesson.stem}`);
  if (lesson.wrong !== null && lesson.wrong !== `${lesson.stem}.wrong`) {
    at(null, "frontmatter-wrong", "wrong", `wrong is ${lesson.wrong}, expected ${lesson.stem}.wrong`);
  }
  if (lesson.variant) {
    const me = (p: Play) => p.entities.filter((e) => e.is_me).map((e) => e.id).join(",");
    if (me(lesson.main) !== me(lesson.variant)) at(null, "wrong-me", "plays", "the wrong variant marks a different me");
    if (lesson.main.duration_ms !== lesson.variant.duration_ms) at(null, "wrong-duration", "plays", "the wrong variant has a different duration");
    if (lesson.main.id === lesson.variant.id) at(null, "wrong-id", "plays", "the wrong variant shares the play's id");
  }

  const required = ["Principle", "Cue", ...(lesson.variant ? ["What wrong looks like"] : [])];
  for (const name of required) {
    if (!lesson.sections[name]) at(null, "section-missing", `## ${name}`, "required section is missing or empty");
  }
  for (const name of Object.keys(lesson.sections)) {
    if (!ALLOWED_SECTIONS.includes(name)) at(null, "section-unknown", `## ${name}`, `not one of ${ALLOWED_SECTIONS.join(", ")}`);
  }

  const first = lesson.steps[0];
  if (!first || first.type !== "card" || first.section !== "Principle") at(null, "first-step", "steps", "the first step is the Principle card");
  const last = lesson.steps[lesson.steps.length - 1];
  if (!last || last.type !== "card" || last.section !== "Cue") at(null, "last-step", "steps", "the last step is the Cue card");

  const decisions = new Set(lesson.main.events.filter((e) => e.kind === "decision_point").map((e) => e.t_ms));
  const choices = new Set<number>();
  for (const step of lesson.steps) {
    if (step.type === "choice") {
      choices.add(step.at_ms);
      const [minOpts, maxOpts] = LIMITS.options;
      if (step.options.length < minOpts || step.options.length > maxOpts) {
        at(step.id, "option-count", "options", `${step.options.length} options, expected ${minOpts} to ${maxOpts}`);
      }
      const right = step.options.filter((o) => o.grade === 2);
      if (right.length !== 1) at(step.id, "one-right-option", "options", `${right.length} options graded 2, expected one`);
      const counts = step.options.map((o) => words(o.text));
      const longest = Math.max(...counts);
      const others = step.options.filter((o) => o.grade !== 2).map((o) => words(o.text));
      if (right.length === 1 && others.length && words(right[0].text) >= Math.max(...others) + 2) {
        at(step.id, "right-is-longest", "options", "the right option is longer than every other by two words or more");
      }
      if (longest > Math.min(...counts) * LIMITS.optionSpread) {
        at(step.id, "option-spread", "options", `options run from ${Math.min(...counts)} to ${longest} words`);
      }
      step.options.forEach((o, i) => {
        if (o.play === "wrong" && (o.grade === 2 || !lesson.variant)) {
          at(step.id, "play-wrong", `option ${i + 1}`, "only a wrong or close option plays the wrong variant, and only when there is one");
        }
      });
    } else if (step.type === "spot" && !step.target && !decisions.has(step.at_ms)) {
      at(step.id, "spot-off-decision", "at_ms", `${step.at_ms} ms is not a decision point (${[...decisions].sort((a, b) => a - b).join(", ")})`);
    }
  }
  for (const t of decisions) {
    if (!choices.has(t)) at(null, "decision-without-choice", `decision point at ${t} ms`, "no choice step asks about it");
  }

  const listed = c.order.filter((id) => id === lesson.id).length;
  if (listed === 0) at(null, "course-unlisted", "course.json", "the lesson is not in any module");
  if (listed > 1) at(null, "course-duplicate", "course.json", `the lesson is listed ${listed} times`);
  const module = c.course.modules.find((m) => m.lessons.includes(lesson.id));
  if (module && module.id !== lesson.module) {
    at(null, "course-module", "module", `listed under ${module.id} but declares ${lesson.module}`);
  }
  return out;
}

// ------------------------------------------------------------------- lint

export function lint(c: Curriculum): Issue[] {
  const out: Issue[] = [];
  const known = new Set(c.lessons.map((l) => l.id));
  for (const id of c.course.modules.flatMap((m) => m.lessons)) {
    if (!known.has(id)) out.push({ lesson: id, step: null, rule: "course-missing", where: "course.json", text: "no lesson has this id" });
  }
  for (const lesson of c.lessons) {
    out.push(...structure(lesson, c));
    const cx: Context = { order: c.order, glossary: c.glossary, lesson: lesson.id };
    for (const t of surfaces(lesson)) {
      for (const rule of TEXT_RULES) {
        if (rule.kinds !== "all" && !rule.kinds.includes(t.kind)) continue;
        const text = rule.check(t, cx);
        if (text) out.push({ lesson: t.lesson, step: t.step, rule: rule.id, where: t.where, text });
      }
    }
  }
  return out;
}

// -------------------------------------------------------------- inventory

export type LessonInventory = {
  id: string;
  module: string;
  title: string;
  steps: number;
  graded: number;
  captions: number;
  terms: { term: string; tier: Tier; count: number }[];
};

export function inventory(c: Curriculum): LessonInventory[] {
  return c.lessons.map((lesson) => {
    const texts = surfaces(lesson);
    const terms = c.glossary.terms
      .map((term) => {
        const re = new RegExp(pattern(term).source, "gi");
        const count = texts.reduce((n, t) => n + (t.text.match(re)?.length ?? 0), 0);
        return { term: term.term, tier: term.tier, count };
      })
      .filter((t) => t.count > 0);
    return {
      id: lesson.id,
      module: lesson.module,
      title: lesson.title,
      steps: lesson.steps.length,
      graded: lesson.steps.filter((s) => s.type === "choice" || s.type === "spot").length,
      captions: (lesson.main.captions?.length ?? 0) + (lesson.variant?.captions?.length ?? 0),
      terms,
    };
  });
}
