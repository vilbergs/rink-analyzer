<script lang="ts">
  import type { LessonSession } from "../session.svelte";

  let { session, step }: { session: LessonSession; step: { section: string } } = $props();

  const paragraphs = $derived((session.meta.sections[step.section] ?? "").split(/\n\s*\n/));
</script>

<div class="step-body">
  <span class="eyebrow">{step.section}</span>
  {#each paragraphs as text, i (i)}
    <p class:lead={paragraphs.length === 1}>{text}</p>
  {/each}
</div>
<div class="actions">
  <button class="btn primary" onclick={() => session.next()}>Continue</button>
</div>

<style>
  .lead {
    font-size: 19px;
    line-height: 1.4;
  }
</style>
