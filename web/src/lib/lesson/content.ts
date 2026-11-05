import { fetchText, wasmReady } from "../load";
import { LessonHandle } from "../wasm/rink_wasm.js";
import type { PlayEntity, PlayTeam } from "./types";

export type LessonSummary = { id: string; title: string; principle: string; stepCount: number };
export type CourseModule = { id: string; title: string; description: string; lessons: LessonSummary[] };
export type Course = { id: string; title: string; description: string; requires: string[]; modules: CourseModule[] };
export type Catalog = { courses: Course[] };

type CatalogFile = {
  schema: number;
  courses: {
    id: string;
    title: string;
    description?: string;
    requires?: string[];
    modules: { id: string; title: string; description?: string; lessons: string[] }[];
  }[];
};

export type LoadedLesson = {
  handle: LessonHandle;
  entities: PlayEntity[];
  teams: PlayTeam[];
  /** The rink's width in meters, across the ice. */
  rinkWidth: number;
  hasWrong: boolean;
};

/** The courses in order, each with its modules in order, each lesson with its title, principle and step count. */
export async function loadCatalog(): Promise<Catalog> {
  const file: CatalogFile = JSON.parse(await fetchText("curriculum/courses.json"));
  return {
    courses: await Promise.all(
      file.courses.map(async ({ id, title, description, requires, modules }) => ({
        id,
        title,
        description: description ?? "",
        requires: requires ?? [],
        modules: await Promise.all(
          modules.map(async (module) => ({
            id: module.id,
            title: module.title,
            description: module.description ?? "",
            lessons: await Promise.all(module.lessons.map(summary)),
          })),
        ),
      })),
    ),
  };
}

let cachedCatalog: Promise<Catalog> | null = null;

/** `loadCatalog()`, fetched once per page load. */
export function catalog(): Promise<Catalog> {
  cachedCatalog ??= loadCatalog();
  return cachedCatalog;
}

async function summary(id: string): Promise<LessonSummary> {
  const [md, steps] = await Promise.all([
    fetchText(`curriculum/lessons/${id}.md`),
    fetchText(`curriculum/lessons/${id}.steps.json`),
  ]);
  return {
    id,
    title: frontmatter(md).title ?? id,
    principle: section(md, "Principle"),
    stepCount: (JSON.parse(steps) as { steps: unknown[] }).steps.length,
  };
}

export async function loadLesson(id: string): Promise<LoadedLesson> {
  const [md, steps] = await Promise.all([
    fetchText(`curriculum/lessons/${id}.md`),
    fetchText(`curriculum/lessons/${id}.steps.json`),
  ]);
  const { play, wrong } = frontmatter(md);
  if (!play) throw new Error(`${id}.md names no play`);
  const playFile = (stem: string) => fetchText(`curriculum/plays/${stem}.rink.json`);
  const [main, wrongJson] = await Promise.all([playFile(play), wrong ? playFile(wrong) : null, wasmReady()]);
  const file = JSON.parse(main);
  return {
    handle: LessonHandle.load(md, steps, main, wrongJson),
    entities: file.entities,
    teams: file.teams,
    rinkWidth: file.rink.width_m,
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
