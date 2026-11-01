<script lang="ts">
  import { paragraphs } from "../paragraphs";
  import type { LessonSession } from "../session.svelte";

  let { session, step }: { session: LessonSession; step: { section: string } } = $props();

  const texts = $derived(paragraphs(session.meta.sections[step.section] ?? ""));
</script>

<div class="step-body">
  <span class="eyebrow">{step.section}</span>
  {#each texts as text, i (i)}
    <p class:lead={texts.length === 1}>{text}</p>
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
