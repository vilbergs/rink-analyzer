<script lang="ts">
  import { PlayerState } from "./lib/player.svelte";
  import RinkCanvas from "./lib/rink/RinkCanvas.svelte";
  import Transport from "./lib/timeline/Transport.svelte";
  import init, { PlayHandle } from "./lib/wasm/rink_wasm.js";

  async function load(): Promise<PlayerState> {
    const [, play, review] = await Promise.all([
      init(),
      fetchText("plays/breakout.rink.json"),
      fetchText("plays/breakout.review.json"),
    ]);
    return new PlayerState(PlayHandle.from_json(play), review);
  }

  async function fetchText(path: string): Promise<string> {
    const res = await fetch(import.meta.env.BASE_URL + path);
    if (!res.ok) throw new Error(`${path}: ${res.status} ${res.statusText}`);
    return res.text();
  }
</script>

{#await load()}
  <p class="status">Loading…</p>
{:then player}
  <main>
    <header>{player.title}</header>
    <RinkCanvas {player} />
    <Transport {player} />
  </main>
{:catch error}
  <p class="status">Could not load the play: {error.message}</p>
{/await}

<style>
  main {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto;
    height: 100%;
  }
  header {
    padding: 8px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }
  .status {
    padding: 16px;
    color: var(--text-muted);
  }
  @media (max-width: 699px) {
    main {
      height: auto;
      grid-template-rows: auto;
    }
    main > :global(.rink) {
      aspect-ratio: 2 / 1;
    }
  }
</style>
