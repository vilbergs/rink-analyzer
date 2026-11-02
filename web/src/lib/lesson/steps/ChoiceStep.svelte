<script lang="ts">
  import { GRADES } from "../grades";
  import Icon from "../Icon.svelte";
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
      {@const mine = chosen === i}
      {@const right = chosen !== null && !mine && option.grade === 2}
      <li>
        <button
          class="option"
          class:mine
          class:right
          class:dim={chosen !== null && !mine && !right}
          style:--grade={mine || right ? grade.color : null}
          disabled={chosen !== null}
          onclick={() => session.respond({ kind: "choice", index: i })}
        >
          {#if mine}
            <span class="tag">Your answer · {grade.label}</span>
          {:else if right}
            <span class="tag"><Icon name="check" size={14} />{grade.label}</span>
          {/if}
          <span>{option.text}</span>
          {#if mine && response}
            <span class="feedback">{response.graded.feedback}</span>
          {/if}
        </button>
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
    line-height: 1.4;
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
    gap: 6px;
    width: 100%;
    min-height: 48px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--raised);
    color: var(--text);
    font: inherit;
    font-size: 15px;
    line-height: 1.4;
    text-align: left;
    cursor: pointer;
  }
  .option:disabled {
    cursor: default;
  }
  .option.mine,
  .option.right {
    padding: 11px 13px;
    border: 2px solid var(--grade);
  }
  .option.dim {
    color: var(--text-muted);
  }
  .tag {
    display: flex;
    align-items: center;
    gap: 4px;
    color: var(--grade);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .feedback {
    margin-top: 4px;
    padding-top: 10px;
    border-top: 1px solid var(--border);
    color: var(--text-soft);
    font-size: 14px;
    line-height: 1.5;
  }
</style>
