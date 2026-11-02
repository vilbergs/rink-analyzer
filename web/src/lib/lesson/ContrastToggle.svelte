<script lang="ts">
  import type { LessonSession } from "./session.svelte";
  import type { Which } from "./types";

  let { session }: { session: LessonSession } = $props();

  const SIDES: [Which, string][] = [
    ["main", "Right"],
    ["wrong", "Wrong"],
  ];
</script>

<div class="bar">
  <div class="toggle" role="radiogroup" aria-label="Which play">
    {#each SIDES as [which, label] (which)}
      <button
        role="radio"
        aria-checked={session.which === which}
        class={which}
        disabled={which === "wrong" && !session.hasWrong}
        onclick={() => (session.which = which)}>{label}</button
      >
    {/each}
  </div>
</div>

<style>
  .bar {
    display: flex;
    flex: none;
    justify-content: center;
    padding: 4px 16px 8px;
    background: var(--app-bg);
  }
  .toggle {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 4px;
    width: 220px;
    padding: 3px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--panel);
  }
  button {
    min-height: 44px;
    border: 1px solid transparent;
    border-radius: 9px;
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-size: 14px;
    font-weight: 600;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .main[aria-checked="true"] {
    border-color: #2b5a3f;
    background: #1c3326;
    color: #a8e6c3;
  }
  .wrong[aria-checked="true"] {
    border-color: #5a3236;
    background: #3a1f22;
    color: #ffb3b3;
  }
</style>
