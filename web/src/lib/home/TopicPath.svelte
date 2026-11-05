<script lang="ts">
  import type { ProgressStore } from "../progress/store";
  import Icon from "./Icon.svelte";
  import { findModule, loadPath, type Path } from "./path";
  import Segments from "./Segments.svelte";

  let { id, progress }: { id: string; progress: ProgressStore } = $props();

  let path = $state.raw<Path | null>(null);
  let loadError = $state<string | null>(null);

  $effect(() => {
    loadPath(progress).then(
      (p) => (path = p),
      (e: Error) => (loadError = e.message),
    );
  });

  const found = $derived(path && findModule(path.courses, (m) => m.id === id));
  const module = $derived(found?.module);
  const firstLocked = $derived(module?.lessons.find((l) => l.state === "locked"));
</script>

<main class="topic">
  <a class="back" href="#/"><Icon name="back" size={18} />Home</a>
  {#if found && module}
    <span class="eyebrow">{found.course.title} · Topic {found.index + 1} of {found.course.modules.length}</span>
    <h1>{module.title}</h1>
    <p class="description">{module.description}</p>
    <div class="progress">
      <Segments lessons={module.lessons} />
      <span class="count">{module.studied} of {module.lessons.length}</span>
    </div>
    <ol>
      {#each module.lessons as lesson (lesson.id)}
        <li class={lesson.state} class:current={lesson === module.next}>
          {#if lesson === module.next}
            <span class="node">{lesson.n}</span>
            <div class="card">
              <span class="eyebrow accent">Up next</span>
              <h2>{lesson.title}</h2>
              <p class="principle">{lesson.principle}</p>
              <span class="meta">{lesson.stepCount} steps · about 10 min</span>
              <a class="btn primary" href="#/lesson/{lesson.id}">Start lesson</a>
            </div>
          {:else if lesson.state === "locked"}
            <span class="node"><Icon name="lock" size={18} /></span>
            <div class="row">
              <span class="title">{lesson.title}</span>
              <span class="meta">
                {lesson === firstLocked ? `Locked · unlocks after lesson ${lesson.n - 1}` : "Locked"}
              </span>
            </div>
          {:else}
            <a class="row" href="#/lesson/{lesson.id}">
              <span class="node">
                {#if lesson.state === "studied"}<Icon name="check" size={18} />{:else}{lesson.n}{/if}
              </span>
              <span class="title">{lesson.title}</span>
              <span class="meta">{lesson.state === "studied" ? "Studied · tap to review" : "Open"}</span>
            </a>
          {/if}
        </li>
      {/each}
    </ol>
  {:else if loadError}
    <p class="status">Could not load the courses: {loadError}</p>
  {:else if path}
    <p class="status">There is no topic “{id}”.</p>
  {:else}
    <p class="status">Loading…</p>
  {/if}
</main>

<style>
  .topic {
    max-width: 640px;
    margin: 0 auto;
    padding: calc(4px + env(safe-area-inset-top)) calc(16px + env(safe-area-inset-right))
      calc(24px + env(safe-area-inset-bottom)) calc(16px + env(safe-area-inset-left));
  }
  .back {
    display: flex;
    width: fit-content;
    gap: 2px;
    align-items: center;
    min-height: 44px;
    margin: 0 0 8px -6px;
    color: var(--text-muted);
    font-size: 14px;
    text-decoration: none;
  }
  h1 {
    margin: 4px 0 6px;
    font-size: 24px;
  }
  .description {
    margin: 0 0 14px;
    color: var(--text-muted);
    font-size: 15px;
    line-height: 1.45;
  }
  .progress {
    display: flex;
    gap: 12px;
    align-items: center;
    margin-bottom: 24px;
  }
  .count {
    color: var(--text-muted);
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  ol {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    position: relative;
    display: grid;
    grid-template-columns: 40px 1fr;
    gap: 14px;
    padding-bottom: 20px;
  }
  li:not(:last-child)::before {
    content: "";
    position: absolute;
    top: 40px;
    bottom: 0;
    left: 19px;
    width: 2px;
    background: var(--border);
  }
  li.studied:not(:last-child)::before {
    background: var(--feedback);
  }
  .node {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border: 2px solid var(--border);
    border-radius: 50%;
    background: var(--raised);
    color: var(--text-muted);
    font-weight: 700;
    font-variant-numeric: tabular-nums;
  }
  .studied .node {
    border-color: var(--feedback);
    background: var(--feedback);
    color: #0f1115;
  }
  .current .node {
    border-color: var(--accent);
    background: var(--accent);
    color: #fff;
    box-shadow: 0 0 0 5px rgba(76, 141, 255, 0.22);
  }
  .locked .node {
    background: var(--panel);
    opacity: 0.6;
  }
  .row {
    display: grid;
    align-content: center;
    gap: 2px;
    min-height: 44px;
    color: inherit;
    text-decoration: none;
  }
  a.row {
    grid-column: 1 / 3;
    grid-template-columns: 40px 1fr;
    column-gap: 14px;
  }
  a.row .node {
    grid-row: 1 / 3;
  }
  .title {
    font-size: 16px;
    font-weight: 600;
  }
  .locked .title {
    color: var(--text-muted);
    font-weight: 500;
  }
  .meta {
    color: var(--text-muted);
    font-size: 13px;
  }
  .card {
    display: grid;
    gap: 8px;
    padding: 14px;
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--panel);
  }
  .accent {
    color: var(--accent);
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .principle {
    margin: 0;
    line-height: 1.45;
    font-size: 15px;
  }
  .card .btn {
    display: flex;
    align-items: center;
    justify-content: center;
    margin-top: 4px;
    text-decoration: none;
  }
  .status {
    color: var(--text-muted);
  }
</style>
