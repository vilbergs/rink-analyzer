<script lang="ts">
  import { paragraphs } from "../paragraphs";
  import type { LessonSession } from "../session.svelte";
  import type { Step, Which } from "../types";

  let { session, step }: { session: LessonSession; step: Extract<Step, { type: "contrast" }> } = $props();

  const SIDES: [Which, string][] = [
    ["main", "Right"],
    ["wrong", "Wrong"],
  ];
</script>

<div class="step-body">
  <span class="eyebrow">{step.section}</span>
  <div class="toggle" role="radiogroup" aria-label="Which play">
    {#each SIDES as [which, label] (which)}
      <button
        role="radio"
        aria-checked={session.which === which}
        class:on={session.which === which}
        class:wrong={which === "wrong"}
        disabled={which === "wrong" && !session.hasWrong}
        onclick={() => (session.which = which)}>{label}</button
      >
    {/each}
  </div>
  {#each paragraphs(session.meta.sections[step.section] ?? "") as text, i (i)}
    <p>{text}</p>
  {/each}
</div>
<div class="actions">
  <button class="btn" onclick={() => (session.playing ? session.pause() : session.play())}>
    {session.playing ? "Pause" : "Play"}
  </button>
  <button class="btn primary" onclick={() => session.next()}>Continue</button>
</div>

<style>
  .toggle {
    display: grid;
    grid-template-columns: 1fr 1fr;
    padding: 3px;
    border-radius: 10px;
    background: var(--raised);
  }
  .toggle button {
    min-height: 40px;
    border: none;
    border-radius: 8px;
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .toggle button.on {
    background: var(--feedback);
    color: #0f1115;
  }
  .toggle button.on.wrong {
    background: var(--danger);
  }
</style>
