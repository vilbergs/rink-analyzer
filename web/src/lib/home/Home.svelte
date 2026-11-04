<script lang="ts">
  import type { ProgressStore } from "../progress/store";
  import { settings, type Shot } from "../settings.svelte";
  import CourseIcon from "./CourseIcon.svelte";
  import Icon from "./Icon.svelte";
  import { loadPath, type Path } from "./path";
  import Segments from "./Segments.svelte";
  import ZoneGlyph from "./ZoneGlyph.svelte";

  let { progress }: { progress: ProgressStore } = $props();

  let path = $state.raw<Path | null>(null);
  let loadError = $state<string | null>(null);

  $effect(() => {
    loadPath(progress).then(
      (p) => (path = p),
      (e: Error) => (loadError = e.message),
    );
  });

  const current = $derived(path?.modules.find((m) => m.next));

  const HANDS: { value: Shot; label: string }[] = [
    { value: "left", label: "Left" },
    { value: "right", label: "Right" },
  ];

  async function exportProgress() {
    const blob = new Blob([await progress.exportJson()], { type: "application/json" });
    const a = document.createElement("a");
    a.href = URL.createObjectURL(blob);
    a.download = `rink-replay-progress-${new Date().toISOString().slice(0, 10)}.json`;
    a.click();
    URL.revokeObjectURL(a.href);
  }
</script>

<main class="home">
  <header>
    <span class="eyebrow">Rink Replay</span>
    <a href="#/viewer">Play viewer</a>
  </header>
  {#if path}
    <div class="course">
      <CourseIcon />
      <h1>{path.title}</h1>
    </div>
    <div class="shot">
      <span class="shot-label">I shoot</span>
      <div class="seg" role="group" aria-label="Which way you shoot">
        {#each HANDS as hand (hand.value)}
          <button
            class="segbtn"
            class:on={settings.shot === hand.value}
            aria-pressed={settings.shot === hand.value}
            onclick={() => settings.setShot(settings.shot === hand.value ? null : hand.value)}
          >
            {hand.label}
          </button>
        {/each}
      </div>
      <span class="shot-hint">
        {#if settings.shot}Lessons show the {settings.shot}-shot notes.{:else}Set it to see only your hand's notes.{/if}
      </span>
    </div>
    <h2 class="eyebrow">Topics</h2>
    <ul>
      {#each path.modules as module (module.id)}
        <li class="card">
          <a class="topic" href="#/topic/{module.id}">
            <ZoneGlyph module={module.id} />
            <span class="text">
              <span class="title">{module.title}</span>
              <span class="sub">
                {#if module.studied > 0}
                  {module.studied} of {module.lessons.length} studied
                {:else}
                  {module.lessons.length} lessons · start with “{module.lessons[0]?.title}”
                {/if}
              </span>
            </span>
            <span class="chevron"><Icon name="chevron" /></span>
            <span class="bar"><Segments lessons={module.lessons} /></span>
          </a>
          {#if module === current && module.next}
            <div class="next">
              <div>
                <span class="eyebrow accent">Up next · Lesson {module.next.n}</span>
                <span class="title">{module.next.title}</span>
              </div>
              <a class="btn primary" href="#/lesson/{module.next.id}">Continue</a>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {:else if loadError}
    <p class="status">Could not load the course: {loadError}</p>
  {:else}
    <p class="status">Loading…</p>
  {/if}
  <footer>
    <button class="link" onclick={exportProgress}>Export progress</button>
    <p>Progress is kept in this browser.</p>
  </footer>
</main>

<style>
  .home {
    max-width: 640px;
    margin: 0 auto;
    padding: calc(4px + env(safe-area-inset-top)) calc(16px + env(safe-area-inset-right))
      calc(24px + env(safe-area-inset-bottom)) calc(16px + env(safe-area-inset-left));
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    height: 44px;
  }
  header a {
    display: flex;
    align-items: center;
    min-height: 44px;
    color: var(--text-muted);
    font-size: 13px;
  }
  .course {
    display: flex;
    gap: 14px;
    align-items: center;
    margin: 12px 0 28px;
  }
  h1 {
    margin: 0;
    font-size: 24px;
  }
  h2 {
    margin: 0 0 10px;
  }
  .shot {
    display: grid;
    grid-template-columns: auto auto;
    gap: 4px 12px;
    align-items: center;
    margin: -16px 0 24px;
  }
  .shot-label {
    font-size: 14px;
    font-weight: 600;
  }
  .seg {
    display: inline-flex;
    border: 1px solid var(--border);
    border-radius: 10px;
    overflow: hidden;
  }
  .segbtn {
    min-width: 64px;
    min-height: 40px;
    padding: 0 14px;
    border: none;
    background: var(--raised);
    color: var(--text-muted);
    font: inherit;
    font-size: 14px;
    cursor: pointer;
  }
  .segbtn + .segbtn {
    border-left: 1px solid var(--border);
  }
  .segbtn.on {
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }
  .shot-hint {
    grid-column: 1 / 3;
    color: var(--text-muted);
    font-size: 13px;
  }
  ul {
    display: grid;
    gap: 10px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .card {
    border: 1px solid var(--border);
    border-radius: 14px;
    background: var(--panel);
    overflow: hidden;
  }
  .topic {
    display: grid;
    grid-template-columns: 40px 1fr 20px;
    gap: 6px 12px;
    align-items: center;
    padding: 14px 14px 16px;
    color: inherit;
    text-decoration: none;
  }
  .topic:active {
    background: var(--raised);
  }
  .text {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .title {
    display: block;
    font-size: 16px;
    font-weight: 600;
  }
  .sub {
    color: var(--text-muted);
    font-size: 13px;
    line-height: 1.35;
  }
  .chevron {
    display: flex;
    color: var(--text-muted);
  }
  .bar {
    display: flex;
    grid-column: 2 / 4;
    margin-top: 6px;
  }
  .next {
    display: flex;
    gap: 12px;
    align-items: center;
    justify-content: space-between;
    padding: 12px 14px;
    border-top: 1px solid var(--border);
  }
  .next div {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .accent {
    color: var(--accent);
  }
  .next .btn {
    display: flex;
    flex-shrink: 0;
    align-items: center;
    text-decoration: none;
  }
  footer {
    display: grid;
    justify-items: center;
    gap: 4px;
    margin-top: 32px;
    color: var(--text-muted);
    font-size: 13px;
  }
  footer p {
    margin: 0;
  }
  .link {
    min-height: 44px;
    padding: 0 12px;
    border: none;
    background: none;
    color: var(--text-muted);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }
  .status {
    color: var(--text-muted);
  }
</style>
