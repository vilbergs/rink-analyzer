import { fetchText, wasmReady } from "../load";
import { LessonHandle } from "../wasm/rink_wasm.js";
import type { PlayEntity } from "./types";

export type LessonSummary = { id: string; title: string; principle: string; stepCount: number };
export type CourseModule = { id: string; title: string; description: string; lessons: LessonSummary[] };
export type Course = { title: string; modules: CourseModule[] };

type CourseFile = {
  schema: number;
  title?: string;
  modules: { id: string; title: string; description?: string; lessons: string[] }[];
};

export type LoadedLesson = { handle: LessonHandle; entities: PlayEntity[]; hasWrong: boolean };

/** `l1-dzone-strong-side-wall` is stored as `01-dzone-strong-side-wall`. */
export function lessonStem(id: string): string {
  const m = /^l(\d+)-(.+)$/.exec(id);
  if (!m) throw new Error(`lesson id \`${id}\` is not \`l<n>-<name>\``);
  return `${m[1].padStart(2, "0")}-${m[2]}`;
}

/** The course title and its modules in order, each lesson with its title, principle and step count. */
export async function loadCourse(): Promise<Course> {
  const course: CourseFile = JSON.parse(await fetchText("curriculum/course.json"));
  return {
    title: course.title ?? "",
    modules: await Promise.all(
      course.modules.map(async ({ id, title, description, lessons }) => ({
        id,
        title,
        description: description ?? "",
        lessons: await Promise.all(lessons.map(summary)),
      })),
    ),
  };
}

async function summary(id: string): Promise<LessonSummary> {
  const stem = lessonStem(id);
  const [md, steps] = await Promise.all([
    fetchText(`curriculum/lessons/${stem}.md`),
    fetchText(`curriculum/lessons/${stem}.steps.json`),
  ]);
  return {
    id,
    title: frontmatter(md).title ?? id,
    principle: section(md, "Principle"),
    stepCount: (JSON.parse(steps) as { steps: unknown[] }).steps.length,
  };
}

export async function loadLesson(id: string): Promise<LoadedLesson> {
  const stem = lessonStem(id);
  const [md, steps] = await Promise.all([
    fetchText(`curriculum/lessons/${stem}.md`),
    fetchText(`curriculum/lessons/${stem}.steps.json`),
  ]);
  const { play, wrong } = frontmatter(md);
  if (!play) throw new Error(`${stem}.md names no play`);
  const playFile = (stem: string) => fetchText(`curriculum/plays/${stem}.rink.json`);
  const [main, wrongJson] = await Promise.all([playFile(play), wrong ? playFile(wrong) : null, wasmReady()]);
  return {
    handle: LessonHandle.load(md, steps, main, wrongJson),
    entities: JSON.parse(main).entities,
    hasWrong: wrongJson !== null,
  };
}

/** The `key: value` lines between the leading `---` lines. rink-lesson validates the rest. */
function frontmatter(md: string): Record<string, string> {
  const block = /^---\r?\n([\s\S]*?)\r?\n---/.exec(md)?.[1] ?? "";
  const pairs = block.split(/\r?\n/).map((line) => /^\s*([\w-]+)\s*:\s*(.*?)\s*$/.exec(line));
  return Object.fromEntries(pairs.filter((m) => m !== null).map(([, k, v]) => [k, v.replace(/^"(.*)"$/, "$1")]));
}

function section(md: string, heading: string): string {
  const chunk = md.split(/^## /m).find((c) => c.split("\n", 1)[0].trim() === heading);
  return chunk?.slice(chunk.indexOf("\n") + 1).trim() ?? "";
}
