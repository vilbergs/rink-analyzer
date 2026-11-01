<script lang="ts">
  import type { PlayerState } from "../player.svelte";

  let { player }: { player: PlayerState } = $props();

  const seconds = (ms: number) => `${(ms / 1000).toFixed(1)}s`;
</script>

<div class="transport">
  <button class="play" onclick={() => player.toggle()} aria-label={player.playing ? "Pause" : "Play"}>
    {player.playing ? "❚❚" : "▶"}
  </button>
  <span class="readout">{seconds(player.t)} / {seconds(player.duration)}</span>
  <button class="speed" onclick={() => player.cycleSpeed()} title="Playback speed">{player.speed}×</button>
  <input
    type="range"
    min="0"
    max={player.duration}
    step="100"
    value={player.t}
    oninput={(e) => player.seek(e.currentTarget.valueAsNumber)}
    aria-label="Playhead"
  />
</div>

<style>
  .transport {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 12px;
    background: var(--panel);
    border-top: 1px solid var(--border);
  }
  button {
    min-width: 36px;
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: var(--raised);
    color: var(--text);
    font: inherit;
    cursor: pointer;
  }
  button:hover {
    border-color: var(--accent);
  }
  .readout {
    min-width: 92px;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  input {
    flex: 1;
    min-width: 0;
    accent-color: var(--accent);
  }
  @media (max-width: 699px) {
    .transport {
      flex-wrap: wrap;
    }
    input {
      flex-basis: 100%;
    }
  }
</style>
