<script lang="ts">
  import Icon from "../Icon.svelte";
  import type { LessonSession } from "../session.svelte";
  import { seconds, stepName } from "../text";
  import type { Step } from "../types";

  let { session, step }: { session: LessonSession; step: Extract<Step, { type: "watch" }> } = $props();

  let seen = $state(false);

  $effect(() => {
    if (session.t >= step.to_ms) seen = true;
  });

  const rows = $derived(session.captions.filter((c) => c.t_ms >= step.from_ms && c.t_ms <= step.to_ms));
  const now = $derived(rows.findLast((c) => c.t_ms <= session.t));
</script>

<div class="step-body">
  <span class="eyebrow">{stepName(step)}</span>
  <ol>
    {#each rows as caption (caption.t_ms)}
      <li class:now={caption === now} class:ahead={caption.t_ms > session.t}>
        <span class="time">{seconds(caption.t_ms)}</span>
        <span>{caption.text}</span>
      </li>
    {/each}
  </ol>
</div>
<div class="actions">
  <button class="btn" onclick={() => session.enter()}><Icon name="replay" size={18} />Replay</button>
  <button class="btn primary" disabled={!seen} onclick={() => session.next()}>Continue</button>
</div>

<style>
  ol {
    display: grid;
    gap: 12px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: grid;
    grid-template-columns: 3.2em 1fr;
    gap: 8px;
    color: var(--text-soft);
    font-size: 17px;
    line-height: 1.4;
  }
  li.now {
    color: var(--text);
  }
  li.ahead {
    color: var(--text-dim);
  }
  .time {
    padding-top: 3px;
    color: var(--text-muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  li.ahead .time {
    color: var(--text-dim);
  }
</style>
