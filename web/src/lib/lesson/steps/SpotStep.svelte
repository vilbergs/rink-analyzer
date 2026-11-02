<script lang="ts">
  import { GRADES } from "../grades";
  import PlayerBadge from "../PlayerBadge.svelte";
  import type { LessonSession } from "../session.svelte";
  import type { EntityFacts, Step } from "../types";

  let { session, step }: { session: LessonSession; step: Extract<Step, { type: "spot" }> } = $props();

  const graded = $derived(session.response?.graded ?? null);

  const SIDE = { strong: "Strong", weak: "Weak", middle: "Middle" };
  const PUCK = { above: "Above", level: "Level", below: "Below" };
  const lane = (f: EntityFacts) =>
    f.open_pass_lane_from_carrier === undefined ? "n/a" : f.open_pass_lane_from_carrier ? "Open" : "Covered";
</script>

{#snippet nearest(f: EntityFacts)}
  {#if f.nearest_opponent}
    <PlayerBadge {session} id={f.nearest_opponent.id} />
    {f.nearest_opponent.dist_m} m
  {:else}
    None
  {/if}
{/snippet}

<div class="step-body">
  <p class="prompt">{step.prompt}</p>
  {#if graded}
    {@const grade = GRADES[graded.grade]}
    <p class="feedback" style:--grade={grade.color}>
      <span class="tag">{grade.label}</span>
      {graded.feedback}
    </p>
    {#if graded.facts}
      {@const { actual, target, distance_m } = graded.facts}
      <div class="facts">
        <table>
          <thead>
            <tr>
              <td></td>
              <th scope="col"><span class="dot" style:--dot="var(--accent)"></span>You</th>
              <th scope="col"><span class="dot" style:--dot="var(--feedback)"></span>Target</th>
            </tr>
          </thead>
          <tbody>
            <tr><th scope="row">Side</th><td>{SIDE[actual.side]}</td><td>{SIDE[target.side]}</td></tr>
            <tr>
              <th scope="row">Puck</th><td>{PUCK[actual.relative_to_puck]}</td><td>{PUCK[target.relative_to_puck]}</td>
            </tr>
            <tr><th scope="row">To the puck</th><td>{actual.dist_to_puck_m} m</td><td>{target.dist_to_puck_m} m</td></tr>
            <tr><th scope="row">Nearest opp.</th><td>{@render nearest(actual)}</td><td>{@render nearest(target)}</td></tr>
            <tr><th scope="row">Lane from the carrier</th><td>{lane(actual)}</td><td>{lane(target)}</td></tr>
          </tbody>
        </table>
      </div>
      <p class="gap">{distance_m.toFixed(1)} m from the target.</p>
    {/if}
  {:else}
    <p class="hint">Your player is off the ice until you tap.</p>
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
    line-height: 1.4;
  }
  .hint,
  .gap {
    color: var(--text-muted);
  }
  .feedback {
    display: grid;
    gap: 2px;
  }
  .tag {
    color: var(--grade);
    font-size: 12px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  .facts {
    padding: 4px 12px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--raised);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 14px;
  }
  th,
  td {
    padding: 7px 4px;
    text-align: left;
    vertical-align: top;
  }
  tbody tr {
    border-top: 1px solid var(--border);
  }
  thead th {
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 600;
  }
  tbody th {
    width: 36%;
    color: var(--text-muted);
    font-weight: 400;
  }
  .dot {
    display: inline-block;
    width: 8px;
    height: 8px;
    margin-right: 6px;
    border-radius: 50%;
    background: var(--dot);
  }
</style>
