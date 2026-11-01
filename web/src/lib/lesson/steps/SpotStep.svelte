<script lang="ts">
  import { GRADES } from "../grades";
  import type { LessonSession } from "../session.svelte";
  import type { EntityFacts, Step } from "../types";

  let { session, step }: { session: LessonSession; step: Extract<Step, { type: "spot" }> } = $props();

  const graded = $derived(session.response?.graded ?? null);

  const SIDE = { strong: "Strong side", weak: "Weak side", middle: "Middle" };
  const VS_PUCK = { above: "Above", level: "Level", below: "Below" };

  const rows: [string, (f: EntityFacts) => string][] = [
    ["Side", (f) => SIDE[f.side]],
    ["Vs the puck", (f) => VS_PUCK[f.relative_to_puck]],
    ["To the puck", (f) => `${f.dist_to_puck_m} m`],
    [
      "Nearest opponent",
      (f) => (f.nearest_opponent ? `${session.name(f.nearest_opponent.id)}, ${f.nearest_opponent.dist_m} m` : "None"),
    ],
    [
      "Lane from the carrier",
      (f) => (f.open_pass_lane_from_carrier === undefined ? "n/a" : f.open_pass_lane_from_carrier ? "Open" : "Covered"),
    ],
  ];
</script>

<div class="step-body">
  <p class="prompt">{step.prompt}</p>
  {#if graded}
    {@const grade = GRADES[graded.grade]}
    <p class="feedback" style:--grade={grade.color}>
      <span class="tag">{grade.label}</span>
      {graded.feedback}
    </p>
    {#if graded.facts}
      {@const facts = graded.facts}
      <table>
        <thead>
          <tr><th></th><th>You</th><th>Target</th></tr>
        </thead>
        <tbody>
          {#each rows as [label, show] (label)}
            <tr><th scope="row">{label}</th><td>{show(facts.actual)}</td><td>{show(facts.target)}</td></tr>
          {/each}
        </tbody>
      </table>
      <p class="gap">{facts.distance_m.toFixed(1)} m from the target.</p>
    {/if}
  {:else}
    <p class="hint">Tap the rink.</p>
  {/if}
</div>
{#if graded}
  <div class="actions">
    <button class="btn primary" onclick={() => session.next()}>Continue</button>
  </div>
{/if}

<style>
  .prompt {
    font-size: 17px;
    font-weight: 600;
  }
  .hint {
    color: var(--accent);
  }
  .feedback {
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
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 14px;
  }
  th,
  td {
    padding: 6px 4px;
    border-bottom: 1px solid var(--border);
    text-align: left;
    vertical-align: top;
  }
  thead th {
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 600;
  }
  tbody th {
    width: 34%;
    color: var(--text-muted);
    font-weight: 400;
  }
  .gap {
    color: var(--text-muted);
  }
</style>
