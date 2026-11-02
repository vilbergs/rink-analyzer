<script lang="ts">
  import Icon from "../Icon.svelte";
  import type { LessonSession } from "../session.svelte";
  import { paragraphs } from "../text";
  import type { Step } from "../types";

  let { session, step }: { session: LessonSession; step: Extract<Step, { type: "contrast" }> } = $props();
</script>

<div class="step-body">
  <span class="eyebrow">{step.section}</span>
  {#each paragraphs(session.meta.sections[step.section] ?? "") as text, i (i)}
    <p class="prose">{text}</p>
  {/each}
</div>
<div class="actions">
  <button class="btn" onclick={() => (session.playing ? session.pause() : session.play())}>
    <Icon name={session.playing ? "pause" : "play"} size={18} />
    {session.playing ? "Pause" : "Play"}
  </button>
  <button class="btn primary" onclick={() => session.next()}>Continue</button>
</div>

<style>
  .prose {
    font-size: 17px;
    line-height: 1.5;
  }
</style>
