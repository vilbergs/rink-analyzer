<script lang="ts">
  import CaptionStrip from "../CaptionStrip.svelte";
  import { fetchText, wasmReady } from "../load";
  import RinkCanvas from "../rink/RinkCanvas.svelte";
  import type { SceneOpts } from "../rink/paint";
  import { PlayHandle } from "../wasm/rink_wasm.js";
  import { PlayerState } from "./player.svelte";
  import EventLane from "./timeline/EventLane.svelte";
  import Transport from "./timeline/Transport.svelte";

  const STEP_MS = 100;

  let player = $state<PlayerState | null>(null);
  let loadError = $state<string | null>(null);

  load().then(
    (p) => (player = p),
    (e: Error) => (loadError = e.message),
  );

  async function load(): Promise<PlayerState> {
    const [, play, review] = await Promise.all([
      wasmReady(),
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

  function scene(p: PlayerState, t: number, opts: SceneOpts) {
    return p.handle.scene(t, { ...opts, selected: p.selected, review: p.reviewJson });
  }

  function select(p: PlayerState, x: number, y: number, opts: SceneOpts) {
    p.selected = p.handle.hit_test(Math.round(p.t), x, y, opts) ?? null;
  }

  $effect(() => () => player?.pause());
</script>

<svelte:window {onkeydown} {onkeyup} />

{#if player}
  {@const p = player}
  <main>
    <header><a href="#/">Lessons</a> {p.title}</header>
    <RinkCanvas
      t={p.t}
      scene={(t, opts) => scene(p, t, opts)}
      bind:view={player.view}
      onTap={(x, y, opts) => select(p, x, y, opts)}
    />
    <CaptionStrip text={p.caption?.text ?? null} />
    <Transport player={p} />
    <EventLane player={p} />
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
    display: flex;
    gap: 12px;
    padding: 8px 12px;
    background: var(--panel);
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }
  header a {
    color: var(--text-muted);
    font-weight: 400;
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
