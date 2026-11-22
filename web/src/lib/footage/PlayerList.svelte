<script lang="ts">
  import type { Review, Selection } from "./review.svelte";
  import { trackLabel } from "./tracks";

  let { review }: { review: Review } = $props();

  /** Everyone detected on the current frame, by track id and by confirmed player. */
  const inView = $derived.by(() => {
    const seen = new Set<Selection>();
    for (const track of review.frame.tracks) {
      if (track.predicted) continue;
      seen.add(track.id);
      const player = review.playerOn(review.frame.source_ms, track.id);
      if (player) seen.add(player);
    }
    return seen;
  });

  /** Select a skater, and go to where they next appear if they are not on screen. */
  function choose(event: MouseEvent, selection: Selection) {
    if (event.shiftKey && review.selected != null && selection !== review.selected) {
      review.partner = selection;
      return;
    }
    review.select(selection);
    if (inView.has(selection)) return;
    const runs = review.appearances;
    const next = runs.find((run) => run.start > review.index) ?? runs[0];
    if (next) review.seek(next.start);
  }

  function chooseMe() {
    const anchor = review.me.anchor;
    if (!anchor) return;
    if (!review.me.track) review.seekMs(anchor.source_ms);
    review.select(review.playerOn(review.frame.source_ms, anchor.track_id) ?? anchor.track_id);
  }

  const seconds = (frames: number) => {
    const total = (frames * review.span) / review.frames.length / 1000;
    return total < 10 ? total.toFixed(1) + " s" : Math.round(total) + " s";
  };
</script>

<section class="list">
  <h3>Players</h3>
  {#if review.anchors.length}
    <button class="item you" aria-pressed={review.isMe} onclick={chooseMe}>
      <i class:on={!!review.me.track}></i>
      <span>You</span>
      <small>{review.me.track ? trackLabel(review.me.track.id) : "out of view"}</small>
    </button>
  {/if}
  {#each review.roster.players as player (player.id)}
    <button
      class="item"
      class:away={!player.here}
      aria-pressed={review.selected === player.id}
      disabled={!player.here}
      title={player.here ? "" : "Named in another clip of this session"}
      onclick={(event) => choose(event, player.id)}
    >
      <i class:on={inView.has(player.id)}></i>
      <span>{player.name}</span>
      {#if review.partner === player.id}<small>compare</small>{/if}
    </button>
  {/each}
  {#if !review.roster.players.length && !review.anchors.length}
    <p>Nobody is named yet. Select a skater and name them in the panel on the right.</p>
  {/if}

  <h3>Unnamed tracks <small>{review.roster.tracks.length}</small></h3>
  {#each review.roster.tracks as track (track.id)}
    <button class="item" aria-pressed={review.selected === track.id} onclick={(event) => choose(event, track.id)}>
      <i class:on={inView.has(track.id)}></i>
      <span>{trackLabel(track.id)}{track.jersey ? " · " + track.jersey : ""}</span>
      <small>{review.partner === track.id ? "compare" : seconds(track.frames)}</small>
    </button>
  {/each}
</section>

<style>
  .list {
    display: grid;
    align-content: start;
    gap: 1px;
    padding: 4px 6px 12px;
  }
  h3 {
    margin: 0;
    padding: 12px 6px 6px;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  h3 small {
    margin-left: 4px;
    color: var(--text-dim);
    font-size: inherit;
  }
  p {
    margin: 0;
    padding: 0 6px;
    color: var(--text-muted);
    line-height: 1.45;
  }
  .item {
    display: grid;
    grid-template-columns: 8px 1fr auto;
    align-items: center;
    gap: 8px;
    padding: 5px 6px;
    border: 0;
    border-radius: 5px;
    background: none;
    color: var(--text-soft);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .item:hover:not(:disabled) {
    background: var(--raised);
  }
  .item[aria-pressed="true"] {
    background: color-mix(in srgb, #7df0b0 18%, transparent);
    color: var(--text);
  }
  .item.you[aria-pressed="true"] {
    background: color-mix(in srgb, #ffd260 18%, transparent);
  }
  .item.away {
    opacity: 0.45;
    cursor: default;
  }
  .item span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .item small {
    color: var(--text-dim);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }
  /* Lit when the skater is on screen at the playhead. */
  i {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--border);
  }
  i.on {
    background: var(--feedback);
  }
  .you i.on {
    background: #ffd260;
  }
</style>
