<script lang="ts">
  import CaptionStrip from "../CaptionStrip.svelte";
  import RinkCanvas from "../rink/RinkCanvas.svelte";
  import { loadLesson } from "./content";
  import { LessonSession } from "./session.svelte";
  import type { ProgressStore } from "../progress/store";
  import CardStep from "./steps/CardStep.svelte";
  import ChoiceStep from "./steps/ChoiceStep.svelte";
  import SpotStep from "./steps/SpotStep.svelte";
  import WatchStep from "./steps/WatchStep.svelte";

  let { id, progress }: { id: string; progress: ProgressStore } = $props();

  let session = $state<LessonSession | null>(null);
  let loadError = $state<string | null>(null);

  $effect(() => {
    let open: LessonSession | null = null;
    loadLesson(id).then(
      (loaded) => (session = open = new LessonSession(loaded, progress)),
      (e: Error) => (loadError = e.message),
    );
    return () => open?.close();
  });
</script>

<div class="lesson">
  <header>
    <a class="close" href="#/" aria-label="Close lesson">✕</a>
    {#if session}
      <button class="back" onclick={() => session?.back()} disabled={session.index === 0} aria-label="Previous step"
        >‹</button
      >
      <ol class="progress" aria-label="Step {session.index + 1} of {session.steps.length}">
        {#each session.steps as step, i (step.id)}
          <li class:done={session.finished || i <= session.index}></li>
        {/each}
      </ol>
    {/if}
  </header>

  {#if session}
    {@const s = session}
    <div class="rink-box">
      <RinkCanvas
        t={s.t}
        scene={(t, opts) => s.handle.scene(s.which, t, opts)}
        hidden={s.hidden}
        marks={s.marks}
        onTap={(x, y, opts) => s.tap(x, y, opts)}
      />
    </div>
    <CaptionStrip text={s.step.type === "watch" || s.step.type === "contrast" || s.response ? s.caption : null} />
    <section class="sheet">
      {#key s.index}
        {#if s.step.type === "card"}
          <CardStep session={s} step={s.step} />
        {:else if s.step.type === "watch"}
          <WatchStep session={s} step={s.step} />
        {:else if s.step.type === "choice"}
          <ChoiceStep session={s} step={s.step} />
        {:else if s.step.type === "spot"}
          <SpotStep session={s} step={s.step} />
        {/if}
      {/key}
    </section>
  {:else if loadError}
    <p class="status">Could not load the lesson: {loadError}</p>
  {:else}
    <p class="status">Loading…</p>
  {/if}
</div>

<style>
  .lesson {
    position: fixed;
    inset: 0;
    display: flex;
    flex-direction: column;
    padding: env(safe-area-inset-top) env(safe-area-inset-right) 0 env(safe-area-inset-left);
  }
  header {
    display: flex;
    align-items: center;
    gap: 4px;
    padding: 4px 12px 4px 4px;
  }
  .close,
  .back {
    display: grid;
    place-items: center;
    width: 40px;
    height: 40px;
    border: none;
    background: none;
    color: var(--text-muted);
    font: inherit;
    font-size: 20px;
    text-decoration: none;
  }
  .back:disabled {
    opacity: 0.3;
  }
  .progress {
    display: flex;
    flex: 1;
    gap: 3px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .progress li {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--raised);
  }
  .progress li.done {
    background: var(--accent);
  }
  .rink-box {
    display: grid;
    flex: none;
    aspect-ratio: 2 / 1;
    width: 100%;
    max-height: 50vh;
    touch-action: none;
  }
  .sheet {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    background: var(--panel);
    border-top: 1px solid var(--border);
    border-radius: 16px 16px 0 0;
  }
  .status {
    padding: 16px;
    color: var(--text-muted);
  }
</style>
