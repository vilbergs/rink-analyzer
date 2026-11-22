/**
 * Running commands on the GPU worker (furulid) over Tailscale SSH, shared by the footage-corpus
 * and cvat behaviors.
 *
 * @module
 */

export const BIN = "~/rink-corpus/bin/rink-corpus";

// Tailscale SSH authenticates on the tailnet, so no key agent (1Password) gets asked. Keepalives
// make a dropped session fail instead of waiting forever.
export const SSH = [
  "-o", "BatchMode=yes", "-o", "IdentityAgent=none", "-o", "PubkeyAuthentication=no",
  "-o", "ConnectTimeout=15", "-o", "ServerAliveInterval=30", "-o", "ServerAliveCountMax=4",
];

/** Run a local command, feeding `stdin` if given, and return its stdout bytes. */
export async function runBytes(cmd: string, args: string[], stdin?: string | Uint8Array): Promise<Uint8Array> {
  const child = new Deno.Command(cmd, { args, stdin: stdin === undefined ? "null" : "piped", stdout: "piped", stderr: "piped" }).spawn();
  if (stdin !== undefined) {
    const writer = child.stdin.getWriter();
    await writer.write(typeof stdin === "string" ? new TextEncoder().encode(stdin) : stdin);
    await writer.close();
  }
  const { code, stdout, stderr } = await child.output();
  if (code !== 0) throw new Error(`${cmd} exited ${code}: ${new TextDecoder().decode(stderr).trim().slice(-2000)}`);
  return stdout;
}

export async function run(cmd: string, args: string[], stdin?: string | Uint8Array): Promise<string> {
  return new TextDecoder().decode(await runBytes(cmd, args, stdin));
}

/** Run `remote` on the worker through its login shell. */
export function ssh(host: string, remote: string, stdin?: string | Uint8Array): Promise<string> {
  return run("ssh", [...SSH, host, remote], stdin);
}

/** Run one rink-corpus command on the worker and parse the JSON summary it prints last. */
export async function corpus(
  host: string,
  command: string,
  options: Record<string, string | number | undefined>,
  stdin?: string | Uint8Array,
): Promise<Record<string, unknown>> {
  const flags = Object.entries(options).filter(([, v]) => v !== undefined).flatMap(([k, v]) => [`--${k}`, String(v)]);
  const out = await run("ssh", [...SSH, host, BIN, command, ...flags], stdin);
  return { ran_at: new Date().toISOString(), ...JSON.parse(out.trim().split("\n").at(-1)!) };
}
