/**
 * The lesson content under `curriculum/` as a swamp behavior: an inventory of
 * what each lesson uses, and a lint against STYLE.md and glossary.json.
 *
 * @module
 */
import { z } from "npm:zod@4";
import { inventory, lint, loadCurriculum } from "./curriculum/content.ts";

const GlobalArgsSchema = z.object({
  dir: z.string().default("curriculum").describe("The curriculum directory, relative to the repo root or absolute"),
});

type GlobalArgs = z.infer<typeof GlobalArgsSchema>;

const IssueSchema = z.object({
  lesson: z.string(),
  step: z.string().nullable(),
  rule: z.string(),
  where: z.string(),
  text: z.string(),
});

const IssuesSchema = z.object({
  checked_at: z.iso.datetime(),
  lessons: z.number(),
  total: z.number(),
  issues: z.array(IssueSchema),
  by_rule: z.record(z.string(), z.number()),
  by_lesson: z.record(z.string(), z.number()),
});

const InventorySchema = z.object({
  checked_at: z.iso.datetime(),
  lessons: z.array(z.object({
    id: z.string(),
    module: z.string(),
    title: z.string(),
    steps: z.number(),
    graded: z.number(),
    captions: z.number(),
    terms: z.array(z.object({ term: z.string(), tier: z.enum(["A", "B", "C"]), count: z.number() })),
  })),
  totals: z.object({ lessons: z.number(), steps: z.number(), graded: z.number(), captions: z.number() }),
});

type Context = {
  globalArgs: GlobalArgs;
  repoDir: string;
  logger: { info(msg: string, ...args: unknown[]): void };
  writeResource: (specName: string, name: string, data: Record<string, unknown>) => Promise<{ name: string }>;
};

function resolveDir(context: Context): string {
  const dir = context.globalArgs.dir;
  return dir.startsWith("/") ? dir : `${context.repoDir}/${dir}`;
}

function tally(keys: string[]): Record<string, number> {
  const out: Record<string, number> = {};
  for (const k of keys) out[k] = (out[k] ?? 0) + 1;
  return out;
}

/** Lesson content checks for `curriculum/`. */
export const behavior = {
  type: "@vilbergs/curriculum",
  version: "2026.11.03.1",
  description: "Inventory and lint of the lesson content under curriculum/ against STYLE.md and glossary.json",
  globalArguments: GlobalArgsSchema,
  reports: ["@vilbergs/curriculum-summary"],
  resources: {
    inventory: {
      description: "Per lesson: steps, graded steps, captions, and the glossary terms it uses",
      schema: InventorySchema,
      lifetime: "infinite",
      garbageCollection: 20,
    },
    issues: {
      description: "Every style and structure issue the lint found, with counts by rule and by lesson",
      schema: IssuesSchema,
      lifetime: "infinite",
      garbageCollection: 20,
    },
  },
  methods: {
    inventory: {
      description: "Count what each lesson uses: steps, graded steps, captions and glossary terms",
      arguments: z.object({}),
      execute: async (_args: Record<string, never>, context: Context) => {
        const c = await loadCurriculum(resolveDir(context));
        const lessons = inventory(c);
        const handle = await context.writeResource("inventory", "inventory", {
          checked_at: new Date().toISOString(),
          lessons,
          totals: {
            lessons: lessons.length,
            steps: lessons.reduce((n, l) => n + l.steps, 0),
            graded: lessons.reduce((n, l) => n + l.graded, 0),
            captions: lessons.reduce((n, l) => n + l.captions, 0),
          },
        });
        context.logger.info("{count} lessons inventoried", { count: lessons.length });
        return { dataHandles: [handle] };
      },
    },
    lint: {
      description: "Check every lesson against STYLE.md and glossary.json; the issues data is empty when the content is clean",
      arguments: z.object({}),
      execute: async (_args: Record<string, never>, context: Context) => {
        const c = await loadCurriculum(resolveDir(context));
        const issues = lint(c);
        const handle = await context.writeResource("issues", "issues", {
          checked_at: new Date().toISOString(),
          lessons: c.lessons.length,
          total: issues.length,
          issues,
          by_rule: tally(issues.map((i) => i.rule)),
          by_lesson: tally(issues.map((i) => i.lesson)),
        });
        context.logger.info("{count} issues in {lessons} lessons", { count: issues.length, lessons: c.lessons.length });
        return { dataHandles: [handle] };
      },
    },
  },
};
