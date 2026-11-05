<script lang="ts">
  import RinkCanvas from "../rink/RinkCanvas.svelte";
  import type { ProgressStore } from "../progress/store";
  import { findModule } from "../home/path";
  import { catalog, loadLesson, type CourseModule } from "./content";
  import ContrastToggle from "./ContrastToggle.svelte";
  import Icon from "./Icon.svelte";
  import LessonLane from "./LessonLane.svelte";
  import Result from "./Result.svelte";
  import { LessonSession } from "./session.svelte";
  import CardStep from "./steps/CardStep.svelte";
  import ChoiceStep from "./steps/ChoiceStep.svelte";
  import ContrastStep from "./steps/ContrastStep.svelte";
  import SpotStep from "./steps/SpotStep.svelte";
  import WatchStep from "./steps/WatchStep.svelte";
  import { seconds, stepName } from "./text";

  let { id, progress }: { id: string; progress: ProgressStore } = $props();

  /** The canvas keeps this many pixels free around a focused crop. */
  const FOCUS_MARGIN_PX = 8;
  /** The rink box never takes more of the viewport's height than this. */
  const MAX_RINK_SHARE = 0.48;

  let session = $state<LessonSession | null>(null);
  let loadError = $state<string | null>(null);
  let module = $state<CourseModule | null>(null);
  let expanded = $state(false);
  let rinkWidth = $state(0);
  let viewportHeight = $state(0);

  $effect(() => {
    let open: LessonSession | null = null;
    let closed = false;
    loadLesson(id).then(
      (loaded) => {
        open = new LessonSession(loaded, progress);
        if (closed) open.close();
        else session = open;
      },
      (e: Error) => (loadError = e.message),
    );
    catalog().then((c) => (module = findModule(c.courses, (m) => m.lessons.some((l) => l.id === id))?.module ?? null));
    return () => {
      closed = true;
      open?.close();
    };
  });

  /** The crop's length at the scale the box's width gives, capped so the step keeps room. */
  const rinkHeight = $derived.by(() => {
    if (!session || rinkWidth === 0) return 0;
    const [x0, x1] = session.focus;
    const pxPerM = (rinkWidth - 2 * FOCUS_MARGIN_PX) / session.rinkWidth;
    return Math.round(Math.min(Math.abs(x1 - x0) * pxPerM + 2 * FOCUS_MARGIN_PX, viewportHeight * MAX_RINK_SHARE));
  });

  const intro = $derived.by(() => {
    if (!session || !module) return null;
    const n = module.lessons.findIndex((l) => l.id === id) + 1;
    return `${module.title} · lesson ${n} of ${module.lessons.length} · ${session.meta.title}. ${session.steps.length} steps over one play, about 10 minutes.`;
  });
  const firstCard = $derived(session?.steps.findIndex((s) => s.type === "card") ?? -1);
</script>

<svelte:window bind:innerHeight={viewportHeight} />

<div class="lesson">
  <header>
    <a class="icon-btn" href="#/" aria-label="Close lesson"><Icon name="close" size={22} /></a>
    {#if session}
      {@const n = session.steps.length}
      {@const at = session.finished ? n : session.index}
      <div
        class="progress"
        role="progressbar"
        aria-label="Step {Math.min(at + 1, n)} of {n}"
        aria-valuemin={1}
        aria-valuemax={n}
        aria-valuenow={Math.min(at + 1, n)}
      >
        {#each session.steps as step, i (step.id)}
          <span class:done={i < at} class:current={i === at}></span>
        {/each}
      </div>
      {#if !session.finished && session.step.type !== "card"}
        <button class="icon-btn" aria-label="Expand rink" aria-pressed={expanded} onclick={() => (expanded = !expanded)}>
          <Icon name={expanded ? "collapse" : "expand"} />
        </button>
      {/if}
    {/if}
  </header>

  {#if session?.finished}
    <Result {session} {module} />
  {:else if session}
    {@const s = session}
    {@const step = s.step}
    {#if step.type === "contrast"}
      <ContrastToggle session={s} />
    {/if}
    <div
      class="rink-box"
      class:expanded
      style:height={expanded ? null : `${rinkHeight}px`}
      bind:clientWidth={rinkWidth}
    >
      <RinkCanvas
        t={s.t}
        scene={(t, opts) => s.handle.scene(s.which, t, opts)}
        portrait
        focus={s.focus}
        flipped={s.flipped}
        hidden={s.hidden}
        marks={s.marks}
        onTap={(x, y, opts) => s.tap(x, y, opts)}
      />
    </div>
    <div class="under">
      {#if step.type === "watch"}
        <LessonLane session={s} {step} />
      {/if}
      {#if expanded}
        <p class="subtitle">{s.caption ?? ""}</p>
      {:else if step.type === "contrast"}
        <p class="caption-line"><span class="time">{seconds(s.t)}</span>{s.caption ?? ""}</p>
      {:else if step.type === "spot" && s.response}
        <ul class="legend">
          <li><span class="dot" style:--dot="var(--accent)"></span>Your tap</li>
          <li><span class="dot" style:--dot="var(--feedback)"></span>Target, {step.r1_m} m</li>
          <li><span class="dot ring"></span>{step.r2_m} m</li>
        </ul>
      {/if}
    </div>
    <section class="panel" hidden={expanded}>
      {#key s.index}
        {#if step.type === "card"}
          <CardStep session={s} {step} intro={s.index === firstCard ? intro : null} />
        {:else if step.type === "watch"}
          <WatchStep session={s} {step} />
        {:else if step.type === "choice"}
          <ChoiceStep session={s} {step} />
        {:else if step.type === "spot"}
          <SpotStep session={s} {step} />
        {:else if step.type === "contrast"}
          <ContrastStep session={s} {step} />
        {/if}
      {/key}
    </section>
    {#if expanded}
      <div class="actions">
        <button class="btn" onclick={() => (expanded = false)}>{stepName(step)}</button>
      </div>
    {/if}
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
    background: var(--panel);
  }
  header {
    display: flex;
    flex: none;
    align-items: center;
    gap: 4px;
    height: 44px;
    padding-right: 12px;
    background: var(--app-bg);
  }
  header:has(> button) {
    padding-right: 0;
  }
  .icon-btn {
    display: grid;
    flex: none;
    place-items: center;
    width: 44px;
    height: 44px;
    padding: 0;
    border: none;
    background: none;
    color: var(--text-muted);
    cursor: pointer;
  }
  .icon-btn[aria-pressed="true"] {
    color: var(--accent);
  }
  .progress {
    display: flex;
    flex: 1;
    gap: 4px;
  }
  .progress span {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--border);
  }
  .progress span.done {
    background: var(--accent);
  }
  .progress span.current {
    background: var(--accent-soft);
  }
  .rink-box {
    display: grid;
    flex: none;
    background: linear-gradient(var(--app-bg) 50%, var(--panel) 50%);
    touch-action: none;
  }
  .rink-box.expanded {
    flex: 1;
    min-height: 0;
  }
  .under {
    flex: none;
  }
  .subtitle {
    margin: 0;
    padding: 8px 16px;
    overflow: hidden;
    color: var(--text-soft);
    font-size: 13px;
    text-align: center;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .caption-line {
    display: flex;
    gap: 8px;
    align-items: baseline;
    margin: 0;
    padding: 8px 16px;
    font-size: 13px;
    line-height: 1.4;
  }
  .time {
    flex: none;
    color: var(--text-muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .legend {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
    margin: 0;
    padding: 8px 16px;
    color: var(--text-muted);
    font-size: 13px;
    list-style: none;
  }
  .legend li {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dot {
    width: 10px;
    height: 10px;
    border-radius: 50%;
    background: var(--dot);
  }
  .dot.ring {
    border: 1.5px dashed var(--feedback);
    background: none;
  }
  .panel {
    display: flex;
    flex: 1;
    flex-direction: column;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
  }
  .panel[hidden] {
    display: none;
  }
  .status {
    padding: 16px;
    color: var(--text-muted);
  }
</style>
