<script lang="ts">
  import type { PlayerState } from "../player.svelte";

  let { player }: { player: PlayerState } = $props();

  const at = (ms: number) => `${(ms / player.duration) * 100}%`;
  const label = (kind: string, note?: string) => [kind.replaceAll("_", " "), note].filter(Boolean).join(": ");
</script>

<div class="lane">
  <div class="track">
    {#each player.events as event, i (i)}
      <button
        class="mark"
        class:decision={event.kind === "decision_point"}
        style:left={at(event.t_ms)}
        title={label(event.kind, event.note)}
        aria-label={label(event.kind, event.note)}
        onclick={() => player.seek(event.t_ms)}
      ></button>
    {/each}
    <div class="playhead" style:left={at(player.t)}></div>
  </div>
</div>

<style>
  .lane {
    padding: 0 20px;
    background: var(--panel);
  }
  .track {
    position: relative;
    height: 22px;
  }
  .mark {
    position: absolute;
    top: 5px;
    width: 10px;
    height: 12px;
    margin-left: -5px;
    padding: 0;
    border: none;
    border-radius: 2px;
    background: var(--accent);
    cursor: pointer;
  }
  .mark.decision {
    background: var(--warn);
  }
  .playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--text);
    pointer-events: none;
  }
</style>
