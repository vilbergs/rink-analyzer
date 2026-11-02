<script lang="ts">
  import type { LessonSession } from "../session.svelte";
  import { firstSentence, paragraphs } from "../text";

  let { session, step, intro }: { session: LessonSession; step: { section: string }; intro: string | null } =
    $props();

  const [lead, rest] = $derived(firstSentence(session.meta.sections[step.section] ?? ""));
</script>

<div class="step-body">
  <span class="eyebrow">{step.section}</span>
  <p class="lead">{lead}</p>
  {#each paragraphs(rest) as text, i (i)}
    <p class="rest">{text}</p>
  {/each}
  {#if intro}
    <p class="intro">{intro}</p>
  {/if}
</div>
<div class="actions">
  <button class="btn primary" onclick={() => session.next()}>Continue</button>
</div>

<style>
  .lead {
    font-size: 28px;
    font-weight: 700;
    line-height: 1.2;
  }
  .rest {
    color: var(--text-soft);
    font-size: 18px;
    line-height: 1.45;
  }
  .intro {
    color: var(--text-muted);
    font-size: 14px;
  }
</style>
