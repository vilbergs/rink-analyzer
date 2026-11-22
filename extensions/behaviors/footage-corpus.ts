/**
 * The detector training corpus on the GPU worker. Each method runs one `rink-corpus` command
 * there over Tailscale SSH, with `corpus/sources.yaml` as JSON on stdin, and stores the JSON
 * summary it prints. The corpus itself stays on the worker under ~/rink-corpus.
 *
 * @module
 */
import { z } from "npm:zod@4";
import { parse } from "jsr:@std/yaml@1.3.0";
import { corpus as runCorpus, run, SSH } from "./corpus/worker.ts";

const GlobalArgsSchema = z.object({
  host: z.string().default("vilberg@furulid").describe("SSH target on the tailnet; Tailscale SSH authenticates it"),
  sources: z.string().default("corpus/sources.yaml").describe("The source list, relative to the repo root"),
});

type GlobalArgs = z.infer<typeof GlobalArgsSchema>;

type Context = {
  globalArgs: GlobalArgs;
  repoDir: string;
  logger: { info(msg: string, ...args: unknown[]): void };
  writeResource: (specName: string, name: string, data: Record<string, unknown>) => Promise<{ name: string }>;
};

/** Run one rink-corpus command on the worker with the source list as JSON on stdin. */
async function corpus(context: Context, command: string, options: Record<string, string | number | undefined>) {
  const { host, sources } = context.globalArgs;
  const path = sources.startsWith("/") ? sources : `${context.repoDir}/${sources}`;
  return await runCorpus(host, command, options, JSON.stringify(parse(await Deno.readTextFile(path))));
}

const Summary = z.object({ ran_at: z.iso.datetime() }).loose();

/** Downloads, frame sampling and draft boxes for the player/goalie/referee/puck detector. */
export const behavior = {
  type: "@vilbergs/footage-corpus",
  version: "2026.11.22.1",
  description: "Collect hockey footage and public datasets on the GPU worker, sample frames, and draw draft boxes for labeling",
  globalArguments: GlobalArgsSchema,
  resources: {
    build: { description: "The rink-corpus binary built on the worker", schema: Summary, lifetime: "infinite", garbageCollection: 10 },
    sync: { description: "Videos, datasets and models fetched by the latest sync", schema: Summary, lifetime: "infinite", garbageCollection: 20 },
    sample: { description: "Frames sampled per video by the latest run", schema: Summary, lifetime: "infinite", garbageCollection: 20 },
    prelabel: { description: "Frames given draft boxes by the latest run, with box counts per class", schema: Summary, lifetime: "infinite", garbageCollection: 20 },
    status: { description: "What the corpus holds: videos, hours, frames, labeled, reviewed, disk use", schema: Summary, lifetime: "infinite", garbageCollection: 50 },
  },
  methods: {
    build: {
      description: "Copy the Rust workspace to the worker and build rink-corpus with CUDA there",
      arguments: z.object({}),
      execute: async (_args: Record<string, never>, context: Context) => {
        const { host } = context.globalArgs;
        const src = ".cache/rink-corpus-build";
        await run("ssh", [...SSH, host, `mkdir -p ${src} ~/rink-corpus/bin`]);
        // -R keeps the relative paths, so rsync runs from the repo root.
        await run("sh", ["-c", `cd '${context.repoDir}' && rsync -azR -e 'ssh ${SSH.join(" ")}' Cargo.toml Cargo.lock crates/ '${host}:${src}/'`]);
        const out = await run("ssh", [
          ...SSH,
          host,
          `cd ${src} && export PATH=/opt/cuda/bin:$PATH CUDA_COMPUTE_CAP=89 && ` +
          `~/.cargo/bin/cargo build -q --release --locked -p rink-footage --features corpus-cuda --bin rink-corpus && ` +
          `install -m 755 target/release/rink-corpus ~/rink-corpus/bin/ && sha256sum ~/rink-corpus/bin/rink-corpus`,
        ]);
        const handle = await context.writeResource("build", "build", { ran_at: new Date().toISOString(), sha256: out.trim().split(" ")[0] });
        return { dataHandles: [handle] };
      },
    },
    sync: {
      description: "Download new videos from each YouTube source (capped per source) and fetch datasets and models not yet on disk",
      arguments: z.object({ maxVideos: z.number().int().positive().optional().describe("Default cap per source when a source sets none") }),
      execute: async (args: { maxVideos?: number }, context: Context) => {
        const summary = await corpus(context, "sync", { "max-videos": args.maxVideos });
        context.logger.info("sync done: {summary}", { summary: JSON.stringify(summary.downloaded) });
        return { dataHandles: [await context.writeResource("sync", "sync", summary)] };
      },
    },
    sample: {
      description: "Pull bursts of consecutive frames at source resolution from videos not sampled yet, dropping still stretches",
      arguments: z.object({
        every: z.number().positive().optional().describe("Seconds between bursts (default 4)"),
        burst: z.number().int().positive().optional().describe("Consecutive frames per burst (default 3)"),
      }),
      execute: async (args: { every?: number; burst?: number }, context: Context) => {
        const summary = await corpus(context, "sample", { every: args.every, burst: args.burst });
        return { dataHandles: [await context.writeResource("sample", "sample", summary)] };
      },
    },
    prelabel: {
      description: "Draw draft boxes with the HockeyAI YOLOv8 model on CUDA for frames without labels",
      arguments: z.object({
        size: z.number().int().multipleOf(32).optional().describe("Model input size (default 1280, large enough for pucks)"),
        confidence: z.number().gt(0).lte(1).optional().describe("Score bar; pucks use half of it (default 0.3)"),
      }),
      execute: async (args: { size?: number; confidence?: number }, context: Context) => {
        const summary = await corpus(context, "prelabel", { size: args.size, confidence: args.confidence });
        return { dataHandles: [await context.writeResource("prelabel", "prelabel", summary)] };
      },
    },
    status: {
      description: "Count what the corpus holds on the worker",
      arguments: z.object({}),
      execute: async (_args: Record<string, never>, context: Context) => {
        return { dataHandles: [await context.writeResource("status", "status", await corpus(context, "status", {}))] };
      },
    },
  },
};
