<script lang="ts">
  import { GRADES } from "../grades";
  import type { LessonSession } from "../session.svelte";
  import type { Step } from "../types";

  let { session, step }: { session: LessonSession; step: Extract<Step, { type: "choice" }> } = $props();

  const response = $derived(session.response);
  const chosen = $derived(response?.answer.kind === "choice" ? response.answer.index : null);
</script>

<div class="step-body">
  <p class="prompt">{step.prompt}</p>
  <ul>
    {#each step.options as option, i (i)}
      {@const grade = GRADES[option.grade]}
      <li>
        <button
          class="option"
          class:chosen={chosen === i}
          class:right={chosen !== null && option.grade === 2}
          style:--grade={chosen === i || (chosen !== null && option.grade === 2) ? grade.color : null}
          disabled={chosen !== null}
          onclick={() => session.respond({ kind: "choice", index: i })}
        >
          {option.text}
          {#if chosen !== null && (chosen === i || option.grade === 2)}
            <span class="tag">{grade.label}</span>
          {/if}
        </button>
        {#if chosen === i && response}
          <p class="feedback" style:--grade={grade.color}>{response.graded.feedback}</p>
        {/if}
      </li>
    {/each}
  </ul>
</div>
{#if response}
  <div class="actions">
    {#if response.graded.watch}
      {@const watch = response.graded.watch}
      <button class="btn" onclick={() => session.watchOn(watch.play, watch.from_ms)}>Watch what happens</button>
    {/if}
    <button class="btn primary" onclick={() => session.next()}>Continue</button>
  </div>
{/if}

<style>
  .prompt {
    font-size: 17px;
    font-weight: 600;
  }
  ul {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .option {
    display: grid;
    gap: 4px;
    width: 100%;
    min-height: 48px;
    padding: 12px 14px;
    border: 1px solid var(--grade, var(--border));
    border-radius: 10px;
    background: var(--raised);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .option:disabled {
    cursor: default;
  }
  .option:disabled:not(.chosen, .right) {
    opacity: 0.5;
  }
  .option.chosen {
    border-width: 2px;
  }
  .tag {
    color: var(--grade);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .feedback {
    margin: 8px 2px 0;
    padding-left: 10px;
    border-left: 3px solid var(--grade);
  }
</style>
