/**
 * CVAT on the GPU worker, for correcting the detector's draft boxes. CVAT runs from its own
 * Docker Compose files with the corpus frames folder mounted read-only as its shared folder, so
 * frames are never copied. Tasks are named after the video they hold, which keeps `push`
 * idempotent: a video that already has a task is skipped.
 *
 * @module
 */
import { z } from "npm:zod@4";
import { corpus, run, runBytes, SSH, ssh } from "./corpus/worker.ts";

const GlobalArgsSchema = z.object({
  host: z.string().default("vilberg@furulid").describe("SSH target on the tailnet"),
  url: z.string().default("https://cvat.vilberg.dev").describe("CVAT's address on the tailnet, served by Caddy on the Pi"),
  pi: z.string().default("pi").describe("SSH name of the Pi that runs Caddy, reached through the jump host"),
  jump: z.string().default("furulid").describe("SSH jump host for the Pi"),
  version: z.string().default("v2.83.0").describe("CVAT release tag to run"),
  user: z.string().default("admin"),
  // Optional only because workflow validation reads required global arguments as step inputs.
  password: z.string().meta({ sensitive: true }).optional().describe("Admin password; the definition reads it from the vault"),
});

type GlobalArgs = z.infer<typeof GlobalArgsSchema>;

type Context = {
  globalArgs: GlobalArgs;
  repoDir: string;
  logger: { info(msg: string, ...args: unknown[]): void };
  writeResource: (specName: string, name: string, data: Record<string, unknown>) => Promise<{ name: string }>;
};

const FORMAT = "YOLO 1.1";
// Every CVAT service that reads uploaded or shared files gets the frames folder.
const SHARE_SERVICES = ["cvat_server", "cvat_worker_import", "cvat_worker_export", "cvat_worker_annotation", "cvat_worker_chunks"];

function override(): string {
  const mount = "    volumes:\n      - cvat_share:/home/django/share:ro\n";
  // Caddy on the Pi terminates HTTPS. Traefik keeps its X-Forwarded-Proto only from trusted
  // addresses, and Django needs it to accept the login form's https Origin.
  const traefik = "  traefik:\n    environment:\n      TRAEFIK_ENTRYPOINTS_web_FORWARDEDHEADERS_TRUSTEDIPS: 100.64.0.0/10\n";
  return "services:\n" + traefik + SHARE_SERVICES.map((s) => `  ${s}:\n${mount}`).join("") +
    "volumes:\n  cvat_share:\n    driver_opts:\n      type: none\n      o: bind\n      device: /home/vilberg/rink-corpus/frames\n";
}

/** A small client for the CVAT REST API, using basic auth. */
class Cvat {
  constructor(private args: GlobalArgs) {
    if (!args.password) throw new Error("cvat has no password; set it with --global-arg password='${{ vault.get(default, cvat-admin-password) }}'");
  }

  async request(method: string, path: string, body?: unknown): Promise<Response> {
    const auth = "Basic " + btoa(`${this.args.user}:${this.args.password}`);
    // A firewalled port drops packets instead of refusing, so every request needs its own deadline.
    const init: RequestInit = { method, headers: { Authorization: auth }, signal: AbortSignal.timeout(120_000) };
    if (body instanceof FormData) init.body = body;
    else if (body !== undefined) {
      init.body = JSON.stringify(body);
      (init.headers as Record<string, string>)["Content-Type"] = "application/json";
    }
    const response = await fetch(`${this.args.url}${path}`, init);
    if (!response.ok) throw new Error(`CVAT ${method} ${path}: ${response.status} ${(await response.text()).slice(0, 500)}`);
    return response;
  }

  async json<T = Record<string, unknown>>(method: string, path: string, body?: unknown): Promise<T> {
    return await (await this.request(method, path, body)).json() as T;
  }

  /** Every result of a paginated list. */
  async list<T>(path: string): Promise<T[]> {
    const results: T[] = [];
    let next: string | null = `${path}${path.includes("?") ? "&" : "?"}page_size=100`;
    while (next) {
      const page: { results: T[]; next: string | null } = await this.json("GET", next);
      results.push(...page.results);
      next = page.next ? page.next.slice(page.next.indexOf("/api/")) : null;
    }
    return results;
  }

  /** Wait for a background request (import, export, data upload) and return it. */
  async finish(rqId: string): Promise<{ status: string; message: string; result_url?: string }> {
    for (let wait = 1000;; wait = Math.min(wait * 2, 10_000)) {
      const rq: { status: string; message: string; result_url?: string } = await this.json("GET", `/api/requests/${encodeURIComponent(rqId)}`);
      if (rq.status === "finished") return rq;
      if (rq.status === "failed") throw new Error(`CVAT request ${rqId} failed: ${rq.message}`);
      await new Promise((r) => setTimeout(r, wait));
    }
  }
}

type Task = { id: number; name: string; size: number };
type Job = { id: number; task_id: number; state: string };

const Summary = z.object({ ran_at: z.iso.datetime() }).loose();

/** Label the corpus in CVAT: start it, load draft boxes as tasks, take corrected boxes back. */
export const behavior = {
  type: "@vilbergs/cvat",
  version: "2026.11.22.1",
  description: "Run CVAT on the GPU worker to correct the detector's draft boxes, one task per video",
  globalArguments: GlobalArgsSchema,
  resources: {
    site: { description: "The Caddy site and DNS record that put CVAT on the tailnet", schema: Summary, lifetime: "infinite", garbageCollection: 10 },
    up: { description: "CVAT version and address after the latest start", schema: Summary, lifetime: "infinite", garbageCollection: 10 },
    push: { description: "Tasks created from videos with draft boxes", schema: Summary, lifetime: "infinite", garbageCollection: 20 },
    pull: { description: "Frames whose corrected boxes came back as reviewed labels", schema: Summary, lifetime: "infinite", garbageCollection: 20 },
  },
  methods: {
    site: {
      description: "Serve CVAT at its tailnet address through Caddy on the Pi: install deploy/pi/cvat.caddy and point the DNS name at the Pi",
      arguments: z.object({}),
      execute: async (_args: Record<string, never>, context: Context) => {
        const { pi, jump, url } = context.globalArgs;
        const name = new URL(url).hostname;
        if (!/^[\w.-]+$/.test(name)) throw new Error(`unexpected site name ${name}`);
        // The Pi is not on Tailscale SSH; its own keys apply, as in the deploy-pi workflow.
        const opts = ["-o", "BatchMode=yes", "-o", "ConnectTimeout=15", "-o", "StrictHostKeyChecking=accept-new", "-J", jump];
        await run("rsync", ["-az", "-e", `ssh ${opts.join(" ")}`, `${context.repoDir}/deploy/pi/cvat.caddy`, `${context.repoDir}/deploy/pi/ensure-dns.py`, `${pi}:/tmp/`]);
        const out = await run("ssh", [...opts, pi, [
          "set -e",
          `dns=$(sudo -n python3 -I /tmp/ensure-dns.py ${name} "$(tailscale ip -4)"); echo "$dns"`,
          `case "$dns" in 'dns ok'*) ;; *) command -v pihole >/dev/null && sudo -n pihole reloaddns >/dev/null 2>&1 && echo 'pihole cache flushed' ;; esac`,
          "sudo -n install -m 644 /tmp/cvat.caddy /etc/caddy/cvat.caddy",
          "grep -q 'import /etc/caddy/cvat.caddy' /etc/caddy/Caddyfile || echo 'import /etc/caddy/cvat.caddy' | sudo -n tee -a /etc/caddy/Caddyfile >/dev/null",
          "sudo -n sh -c 'set -a; . /etc/caddy/cloudflare.env; caddy validate --config /etc/caddy/Caddyfile' >/dev/null 2>&1 || { echo 'caddy config does not validate' >&2; exit 1; }",
          "sudo -n systemctl reload caddy",
          "rm -f /tmp/cvat.caddy /tmp/ensure-dns.py",
        ].join("; ")]);
        const handle = await context.writeResource("site", "site", { ran_at: new Date().toISOString(), url, dns: out.trim().split("\n") });
        return { dataHandles: [handle] };
      },
    },
    up: {
      description: "Start CVAT with Docker Compose on the worker, with the corpus frames as its shared folder, and make sure the admin login exists",
      arguments: z.object({}),
      execute: async (_args: Record<string, never>, context: Context) => {
        const { host, version: tag, user, password } = context.globalArgs;
        const name = new URL(context.globalArgs.url).hostname;
        if (!/^v\d+\.\d+\.\d+$/.test(tag)) throw new Error(`not a CVAT release tag: ${tag}`);
        if (!/^[\w.-]+$/.test(name) || !/^\w+$/.test(user)) throw new Error(`unexpected host ${name} or user ${user}`);
        const cvat = new Cvat(context.globalArgs);
        await ssh(
          host,
          `set -eo pipefail; mkdir -p ~/rink-corpus/frames; ` +
            `if [ ! -d ~/cvat/.git ]; then git clone -q --depth 1 --branch ${tag} https://github.com/cvat-ai/cvat ~/cvat; fi; cd ~/cvat; ` +
            `if [ "$(git describe --tags)" != ${tag} ]; then git fetch -q --depth 1 origin tag ${tag} && git checkout -q ${tag}; fi; ` +
            `cat > docker-compose.override.yml; CVAT_HOST=${name} docker compose up -d --quiet-pull 2>&1 | tail -20`,
          override(),
        );
        // Without credentials: the admin may not exist yet, and CVAT answers bad credentials with 401.
        const aboutUrl = `${context.globalArgs.url}/api/server/about`;
        let about: Record<string, unknown> | undefined;
        for (let i = 0; i < 60 && !about; i++) {
          about = await fetch(aboutUrl, { signal: AbortSignal.timeout(10_000) })
            .then((r) => (r.ok ? r.json() : undefined))
            .catch(() => undefined);
          if (!about) await new Promise((r) => setTimeout(r, 5000));
        }
        if (!about) throw new Error(`CVAT did not answer at ${aboutUrl}`);
        // Creating the admin again fails with "already taken"; any other failure is real.
        await ssh(
          host,
          `read -r P; out=$(docker exec -e DJANGO_SUPERUSER_PASSWORD="$P" cvat_server bash -c ` +
            `'python3 ~/manage.py createsuperuser --noinput --username ${user} --email ${user}@localhost.localdomain' 2>&1) ` +
            `|| { echo "$out" | grep -q "already taken" || { echo "$out" >&2; exit 1; }; }`,
          password + "\n",
        );
        const me: { username: string } = await cvat.json("GET", "/api/users/self");
        const handle = await context.writeResource("up", "up", { ran_at: new Date().toISOString(), version: about.version, url: context.globalArgs.url, user: me.username });
        return { dataHandles: [handle] };
      },
    },
    push: {
      description: "Create a CVAT task for each video whose on-ice frames have draft boxes and no task yet, with the draft boxes loaded",
      arguments: z.object({ videos: z.string().optional().describe("Comma-separated video ids; all when left out") }),
      execute: async (args: { videos?: string }, context: Context) => {
        const { host } = context.globalArgs;
        const cvat = new Cvat(context.globalArgs);
        const existing = new Set((await cvat.list<Task>("/api/tasks")).map((t) => t.name));
        const exported = await corpus(host, "cvat-export", { videos: args.videos }) as {
          classes: string[];
          videos: Record<string, { zip: string; frames: string[] }>;
        };
        const created: Record<string, { task: number; frames: number }> = {};
        for (const [video, { zip, frames }] of Object.entries(exported.videos)) {
          if (existing.has(video) || frames.length === 0) continue;
          const task: Task = await cvat.json("POST", "/api/tasks", {
            name: video,
            labels: exported.classes.map((name) => ({ name, type: "rectangle" })),
          });
          const data: { rq_id: string } = await cvat.json("POST", `/api/tasks/${task.id}/data`, {
            server_files: frames,
            image_quality: 95,
            sorting_method: "predefined",
            use_cache: true,
          });
          await cvat.finish(data.rq_id);
          const form = new FormData();
          const bytes = await runBytes("ssh", [...SSH, host, `cat ~/rink-corpus/${zip}`]);
          form.append("annotation_file", new Blob([bytes as BlobPart]), `${video}.zip`);
          const upload: { rq_id: string } = await cvat.json("POST", `/api/tasks/${task.id}/annotations?format=${encodeURIComponent(FORMAT)}`, form);
          await cvat.finish(upload.rq_id);
          created[video] = { task: task.id, frames: frames.length };
          context.logger.info("task {id} for {video}: {n} frames", { id: task.id, video, n: frames.length });
        }
        const handle = await context.writeResource("push", "push", { ran_at: new Date().toISOString(), created, skipped: [...existing] });
        return { dataHandles: [handle] };
      },
    },
    pull: {
      description: "Export every task whose jobs are all completed and store its boxes as reviewed labels in the corpus",
      arguments: z.object({}),
      execute: async (_args: Record<string, never>, context: Context) => {
        const { host } = context.globalArgs;
        const cvat = new Cvat(context.globalArgs);
        const jobs = await cvat.list<Job>("/api/jobs");
        const done = (await cvat.list<Task>("/api/tasks")).filter((t) => {
          const own = jobs.filter((j) => j.task_id === t.id);
          return own.length > 0 && own.every((j) => j.state === "completed");
        });
        const pulled: Record<string, unknown> = {};
        for (const task of done) {
          const rq: { rq_id: string } = await cvat.json(
            "POST",
            `/api/tasks/${task.id}/dataset/export?format=${encodeURIComponent(FORMAT)}&save_images=false`,
          );
          const { result_url } = await cvat.finish(rq.rq_id);
          if (!result_url) throw new Error(`export of task ${task.id} has no result_url`);
          const zip = new Uint8Array(await (await cvat.request("GET", result_url.slice(result_url.indexOf("/api/")))).arrayBuffer());
          pulled[task.name] = await corpus(host, "cvat-review", {}, zip);
        }
        const handle = await context.writeResource("pull", "pull", { ran_at: new Date().toISOString(), pulled });
        return { dataHandles: [handle] };
      },
    },
  },
};
