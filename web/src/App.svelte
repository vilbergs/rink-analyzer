<script lang="ts">
  import init, { PlayHandle } from "./lib/wasm/rink_wasm.js";

  async function load(): Promise<PlayHandle> {
    const [, play] = await Promise.all([init(), fetchText("plays/breakout.rink.json")]);
    return PlayHandle.from_json(play);
  }

  async function fetchText(path: string): Promise<string> {
    const res = await fetch(import.meta.env.BASE_URL + path);
    if (!res.ok) throw new Error(`${path}: ${res.status} ${res.statusText}`);
    return res.text();
  }
</script>

{#await load()}
  <p class="status">Loading…</p>
{:then handle}
  <main>
    <header>{handle.title()}</header>
    <canvas></canvas>
  </main>
{:catch error}
  <p class="status">Could not load the play: {error.message}</p>
{/await}

<style>
  main {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  header {
    padding: 8px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }
  canvas {
    flex: 1;
    min-height: 0;
  }
  .status {
    padding: 16px;
    color: var(--text-muted);
  }
</style>
