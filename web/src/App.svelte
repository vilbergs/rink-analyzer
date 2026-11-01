<script lang="ts">
  import CaptionStrip from "./lib/CaptionStrip.svelte";
  import { PlayerState } from "./lib/player.svelte";
  import RinkCanvas from "./lib/rink/RinkCanvas.svelte";
  import EventLane from "./lib/timeline/EventLane.svelte";
  import Transport from "./lib/timeline/Transport.svelte";
  import init, { PlayHandle } from "./lib/wasm/rink_wasm.js";

  const STEP_MS = 100;

  let player = $state<PlayerState | null>(null);
  let loadError = $state<string | null>(null);

  load().then(
    (p) => (player = p),
    (e: Error) => (loadError = e.message),
  );

  async function load(): Promise<PlayerState> {
    const [, play, review] = await Promise.all([
      init(),
      fetchText("plays/breakout.rink.json"),
      fetchText("plays/breakout.review.json"),
    ]);
    return new PlayerState(PlayHandle.from_json(play), review);
  }

  const keys: Record<string, (p: PlayerState) => void> = {
    " ": (p) => p.toggle(),
    ArrowLeft: (p) => p.seek(p.t - STEP_MS),
    ArrowRight: (p) => p.seek(p.t + STEP_MS),
    Home: (p) => p.seek(0),
    End: (p) => p.seek(p.duration),
    "0": (p) => p.fitView(),
  };

  function onkeydown(e: KeyboardEvent) {
    const action = keys[e.key];
    if (!player || !action || e.metaKey || e.ctrlKey || e.altKey) return;
    e.preventDefault();
    action(player);
  }

  // A focused button would also click on Space's keyup.
  function onkeyup(e: KeyboardEvent) {
    if (e.key === " ") e.preventDefault();
  }

  async function fetchText(path: string): Promise<string> {
    const res = await fetch(import.meta.env.BASE_URL + path);
    if (!res.ok) throw new Error(`${path}: ${res.status} ${res.statusText}`);
    return res.text();
  }
</script>

<svelte:window {onkeydown} {onkeyup} />

{#if player}
  <main>
    <header>{player.title}</header>
    <RinkCanvas {player} />
    <CaptionStrip {player} />
    <Transport {player} />
    <EventLane {player} />
  </main>
{:else if loadError}
  <p class="status">Could not load the play: {loadError}</p>
{:else}
  <p class="status">Loading…</p>
{/if}

<style>
  main {
    display: grid;
    grid-template-rows: auto minmax(0, 1fr) auto auto auto;
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
