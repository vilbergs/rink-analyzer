<script lang="ts">
  import type { CourseModule } from "./content";
  import { GRADES } from "./grades";
  import type { LessonSession } from "./session.svelte";
  import { stepName } from "./text";

  let { session, module }: { session: LessonSession; module: CourseModule | null } = $props();

  const revisit = $derived(
    session.steps.flatMap((step, index) => {
      const graded = session.responses[step.id]?.graded;
      if (!graded || graded.grade === 2 || !("prompt" in step)) return [];
      const label = GRADES[graded.grade].label;
      return [
        {
          index,
          step,
          grade: graded.grade,
          verdict: graded.facts ? `${label}, ${graded.facts.distance_m.toFixed(1)} m` : label,
        },
      ];
    }),
  );
</script>

<div class="result">
  <p class="title">{session.meta.title}</p>
  <span class="eyebrow">Cue for Saturday</span>
  <p class="cue">{session.meta.sections["Cue"] ?? ""}</p>
  <span class="eyebrow">Steps to revisit</span>
  {#if revisit.length > 0}
    <ul>
      {#each revisit as row (row.step.id)}
        <li>
          <button onclick={() => session.goTo(row.index)}>
            <span class="meta">
              Step {row.index + 1} · {stepName(row.step)} ·
              <span style:color={GRADES[row.grade].color}>{row.verdict}</span>
            </span>
            <span class="prompt">{row.step.prompt}</span>
          </button>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="none">Every graded step was right.</p>
  {/if}
</div>
<div class="actions">
  <a class="btn primary" href={`#/topic/${session.meta.module}`}>Back to {module?.title ?? "the topic"}</a>
</div>

<style>
  .result {
    display: grid;
    flex: 1;
    align-content: start;
    gap: 12px;
    padding: 24px 16px;
    overflow-y: auto;
    background: var(--app-bg);
  }
  .title {
    margin: 0 0 8px;
    color: var(--text-muted);
    font-size: 14px;
  }
  .cue {
    margin: 0 0 20px;
    font-size: 30px;
    font-weight: 700;
    letter-spacing: -0.01em;
    line-height: 1.22;
  }
  ul {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  button {
    display: grid;
    gap: 4px;
    width: 100%;
    min-height: 44px;
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--raised);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .meta {
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 600;
  }
  .prompt {
    font-size: 15px;
    line-height: 1.4;
  }
  .none {
    margin: 0;
    color: var(--text-muted);
  }
  .actions {
    background: var(--app-bg);
  }
</style>
