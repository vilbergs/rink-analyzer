/**
 * After `@vilbergs/curriculum` runs, the issues per lesson and per rule, and
 * how the count moved since the previous lint.
 *
 * @module
 */

type Handle = { name: string; version: number; specName?: string };

type Issue = { lesson: string; step: string | null; rule: string; where: string; text: string };

type Issues = { total: number; lessons: number; issues: Issue[]; by_rule: Record<string, number>; by_lesson: Record<string, number> };

type Inventory = {
  lessons: { id: string; steps: number; graded: number; captions: number; terms: { term: string; tier: string; count: number }[] }[];
};

type Context = {
  modelType: string;
  modelId: string;
  methodName: string;
  executionStatus: string;
  errorMessage?: string;
  dataHandles: Handle[];
  dataRepository: {
    getContent(type: string, modelId: string, name: string, version?: number): Promise<Uint8Array | null>;
    findAllForModel(type: string, modelId: string): Promise<{ name: string; version: number }[]>;
  };
};

async function read<T>(context: Context, name: string, version?: number): Promise<T | null> {
  const raw = await context.dataRepository.getContent(context.modelType, context.modelId, name, version);
  return raw ? JSON.parse(new TextDecoder().decode(raw)) as T : null;
}

async function previousVersion(context: Context, handle: Handle): Promise<number | null> {
  const all = await context.dataRepository.findAllForModel(context.modelType, context.modelId);
  const older = all.filter((d) => d.name === handle.name && d.version < handle.version).map((d) => d.version);
  return older.length ? Math.max(...older) : null;
}

function table(header: string[], rows: string[][]): string {
  const line = (cells: string[]) => `| ${cells.join(" | ")} |`;
  return [line(header), line(header.map(() => "---")), ...rows.map(line)].join("\n");
}

async function lintReport(context: Context, handle: Handle) {
  const now = await read<Issues>(context, handle.name, handle.version);
  if (!now) return { markdown: "No issues data was written.", json: {} };
  const prev = await previousVersion(context, handle);
  const before = prev === null ? null : await read<Issues>(context, handle.name, prev);
  const lessons = [...new Set([...Object.keys(now.by_lesson), ...Object.keys(before?.by_lesson ?? {})])].sort();
  const delta = lessons.map((id) => ({ lesson: id, before: before?.by_lesson[id] ?? 0, now: now.by_lesson[id] ?? 0 }));
  const byRule = Object.entries(now.by_rule).sort((a, b) => b[1] - a[1]);

  const md = [
    `# Curriculum lint: ${now.total} issue${now.total === 1 ? "" : "s"} in ${now.lessons} lessons`,
    before ? `\nPrevious lint: ${before.total}. Change: ${now.total - before.total >= 0 ? "+" : ""}${now.total - before.total}.` : "",
    "\n## By lesson\n",
    table(["Lesson", "Before", "Now"], delta.map((d) => [d.lesson, String(d.before), String(d.now)])),
    "\n## By rule\n",
    table(["Rule", "Count"], byRule.map(([rule, n]) => [rule, String(n)])),
    "\n## Issues\n",
    now.issues.length
      ? table(["Lesson", "Step", "Where", "Rule", "What"], now.issues.map((i) => [i.lesson, i.step ?? "", i.where, i.rule, i.text]))
      : "None. The content is clean.",
  ].join("\n");
  return { markdown: md, json: { total: now.total, previous: before?.total ?? null, by_lesson: delta, by_rule: now.by_rule } };
}

async function inventoryReport(context: Context, handle: Handle) {
  const inv = await read<Inventory>(context, handle.name, handle.version);
  if (!inv) return { markdown: "No inventory data was written.", json: {} };
  const rows = inv.lessons.map((l) => [
    l.id,
    String(l.steps),
    String(l.graded),
    String(l.captions),
    l.terms.filter((t) => t.tier === "A").length.toString(),
    l.terms.filter((t) => t.tier === "B").map((t) => t.term).join(", "),
    l.terms.filter((t) => t.tier === "C").map((t) => t.term).join(", "),
  ]);
  return {
    markdown: `# Curriculum inventory\n\n${table(["Lesson", "Steps", "Graded", "Captions", "Tier A terms", "Tier B terms", "Tier C terms"], rows)}`,
    json: { lessons: inv.lessons },
  };
}

/** Issues per lesson and rule after a lint, or the term inventory after an inventory run. */
export const report = {
  name: "@vilbergs/curriculum-summary",
  description: "Curriculum lint issues per lesson and per rule, with the change since the previous run",
  scope: "method",
  labels: ["curriculum"],
  execute: async (context: Context) => {
    if (context.executionStatus !== "succeeded") {
      return { markdown: `# Curriculum ${context.methodName} failed\n\n${context.errorMessage ?? ""}`, json: { error: context.errorMessage ?? "" } };
    }
    const handle = context.dataHandles[0];
    if (!handle) return { markdown: "Nothing was written.", json: {} };
    return context.methodName === "lint" ? lintReport(context, handle) : inventoryReport(context, handle);
  },
};
