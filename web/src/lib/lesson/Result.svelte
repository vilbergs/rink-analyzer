<script lang="ts">
  import { GRADES } from "./grades";
  import type { LessonSession } from "./session.svelte";

  let { session }: { session: LessonSession } = $props();

  const revisit = $derived(
    session.steps.flatMap((step) => {
      const graded = session.responses[step.id]?.graded;
      return graded && graded.grade < 2 && "prompt" in step ? [{ id: step.id, prompt: step.prompt, grade: graded.grade }] : [];
    }),
  );
</script>

<div class="result">
  <span class="eyebrow">Your cue</span>
  <p class="cue">{session.meta.sections["Cue"] ?? ""}</p>
  {#if revisit.length > 0}
    <span class="eyebrow">Worth another look</span>
    <ul>
      {#each revisit as step (step.id)}
        <li style:--grade={GRADES[step.grade].color}>
          <span class="tag">{GRADES[step.grade].label}</span>
          {step.prompt}
        </li>
      {/each}
    </ul>
  {/if}
</div>
<div class="actions">
  <a class="btn primary" href="#/">Back to lessons</a>
</div>

<style>
  .result {
    display: grid;
    align-content: start;
    gap: 12px;
    padding: 24px 16px;
    line-height: 1.5;
  }
  .cue {
    margin: 0 0 16px;
    font-size: 24px;
    font-weight: 600;
    line-height: 1.3;
  }
  ul {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: grid;
    gap: 2px;
    padding-left: 10px;
    border-left: 3px solid var(--grade);
  }
  .tag {
    color: var(--grade);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .actions a {
    text-align: center;
    text-decoration: none;
  }
</style>
