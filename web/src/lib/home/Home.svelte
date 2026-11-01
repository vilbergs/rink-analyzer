<script lang="ts">
  import { loadCourse, type CourseModule } from "../lesson/content";
  import type { ProgressStore } from "../progress/store";

  let { progress }: { progress: ProgressStore } = $props();

  let studied = $state<Record<string, string>>({});
  let modules = $state<CourseModule[] | null>(null);
  let loadError = $state<string | null>(null);

  loadCourse().then(
    (m) => (modules = m),
    (e: Error) => (loadError = e.message),
  );

  $effect(() => {
    progress.studied().then((s) => (studied = s));
  });

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
  <h1>Rink Replay</h1>
  {#if modules}
    {#each modules as module (module.id)}
      <section>
        <h2>{module.title}</h2>
        <ul>
          {#each module.lessons as lesson (lesson.id)}
            <li>
              <a href="#/lesson/{lesson.id}">
                <span class="title">{lesson.title}</span>
                <span class="principle">{lesson.principle}</span>
                <span class="meta">
                  10 min
                  {#if studied[lesson.id]}<span class="studied">✓ Studied</span>{/if}
                </span>
              </a>
            </li>
          {/each}
        </ul>
      </section>
    {/each}
  {:else if loadError}
    <p class="status">Could not load the course: {loadError}</p>
  {:else}
    <p class="status">Loading…</p>
  {/if}
  <footer>
    <a href="#/viewer">Play viewer</a>
    <button class="link" onclick={exportProgress}>Export progress</button>
  </footer>
</main>

<style>
  .home {
    max-width: 640px;
    margin: 0 auto;
    padding: calc(16px + env(safe-area-inset-top)) calc(16px + env(safe-area-inset-right))
      calc(24px + env(safe-area-inset-bottom)) calc(16px + env(safe-area-inset-left));
  }
  h1 {
    margin: 8px 0 20px;
    font-size: 22px;
  }
  h2 {
    margin: 24px 0 8px;
    color: var(--text-muted);
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
  }
  ul {
    display: grid;
    gap: 8px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li a {
    display: grid;
    gap: 4px;
    padding: 14px 16px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--panel);
    color: inherit;
    text-decoration: none;
  }
  li a:active {
    background: var(--raised);
  }
  .title {
    font-size: 16px;
    font-weight: 600;
  }
  .principle {
    color: var(--text-muted);
    line-height: 1.4;
  }
  .meta {
    color: var(--text-muted);
    font-size: 12px;
  }
  footer {
    display: flex;
    justify-content: space-between;
    margin-top: 32px;
    font-size: 13px;
  }
  footer a,
  .link {
    padding: 0;
    border: none;
    background: none;
    color: var(--text-muted);
    font: inherit;
    text-decoration: underline;
    cursor: pointer;
  }
  .studied {
    margin-left: 8px;
    color: var(--feedback);
    font-weight: 600;
  }
  .status {
    color: var(--text-muted);
  }
</style>
