<script lang="ts">
  import type { LessonSession } from "./session.svelte";
  import { seconds } from "./text";
  import type { Step } from "./types";

  let { session, step }: { session: LessonSession; step: Extract<Step, { type: "watch" }> } = $props();

  /** How close the playhead must be to a decision point to light it, in ms. */
  const ON_EVENT_MS = 60;
  const KEY_STEP_MS = 100;

  let track: HTMLDivElement;
  let dragging = false;

  const duration = $derived(session.duration());
  const at = (ms: number) => `${(ms / duration) * 100}%`;
  const label = (kind: string, note?: string) => [kind.replaceAll("_", " "), note].filter(Boolean).join(": ");

  function scrub(ms: number) {
    session.pause();
    session.seek(Math.min(Math.max(ms, step.from_ms), step.to_ms));
  }

  function scrubTo(e: PointerEvent) {
    const rect = track.getBoundingClientRect();
    scrub(((e.clientX - rect.left) / rect.width) * duration);
  }

  function onpointerdown(e: PointerEvent) {
    track.setPointerCapture(e.pointerId);
    dragging = true;
    scrubTo(e);
  }

  function onpointermove(e: PointerEvent) {
    if (dragging) scrubTo(e);
  }

  function onpointerup() {
    dragging = false;
  }

  function onkeydown(e: KeyboardEvent) {
    const by = { ArrowLeft: -KEY_STEP_MS, ArrowRight: KEY_STEP_MS }[e.key];
    if (by === undefined) return;
    e.preventDefault();
    scrub(session.t + by);
  }
</script>

<div class="lane">
  <span class="time">{seconds(session.t)}</span>
  <div
    class="track"
    bind:this={track}
    role="slider"
    tabindex="0"
    aria-label="Playhead"
    aria-valuemin={step.from_ms}
    aria-valuemax={step.to_ms}
    aria-valuenow={Math.round(session.t)}
    aria-valuetext={seconds(session.t)}
    {onkeydown}
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
  >
    <div class="line"></div>
    <div class="segment" style:left={at(step.from_ms)} style:width={at(step.to_ms - step.from_ms)}></div>
    <div class="played" style:width={at(session.t)}></div>
    {#each session.events as event, i (i)}
      <span
        class="mark"
        class:decision={event.kind === "decision_point"}
        class:on={event.kind === "decision_point" && Math.abs(session.t - event.t_ms) <= ON_EVENT_MS}
        style:left={at(event.t_ms)}
        title={label(event.kind, event.note)}
      ></span>
    {/each}
    <div class="knob" style:left={at(session.t)}></div>
  </div>
  <span class="speed">{session.speed}x</span>
</div>

<style>
  .lane {
    display: flex;
    align-items: center;
    gap: 12px;
    height: 44px;
    padding: 0 16px;
  }
  .time {
    flex: none;
    min-width: 3.2em;
    color: var(--text-muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
  .track {
    position: relative;
    flex: 1;
    align-self: stretch;
    touch-action: none;
    cursor: pointer;
  }
  .line,
  .segment,
  .played {
    position: absolute;
    top: calc(50% - 2px);
    height: 4px;
    border-radius: 2px;
  }
  .line {
    right: 0;
    left: 0;
    background: var(--border);
  }
  .segment {
    background: #3a4150;
  }
  .played {
    left: 0;
    background: var(--accent);
  }
  .mark {
    position: absolute;
    top: calc(50% - 14px);
    width: 6px;
    height: 8px;
    margin-left: -3px;
    border-radius: 1px;
    background: var(--accent);
  }
  .mark.decision {
    background: var(--warn);
  }
  .mark.on {
    box-shadow: 0 0 8px 2px color-mix(in srgb, var(--warn) 60%, transparent);
  }
  .knob {
    position: absolute;
    top: calc(50% - 8px);
    width: 16px;
    height: 16px;
    margin-left: -8px;
    border-radius: 50%;
    background: var(--text);
    box-shadow: 0 1px 3px rgb(0 0 0 / 0.5);
  }
  .track:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: -2px;
    border-radius: 6px;
  }
  .speed {
    flex: none;
    padding: 2px 6px;
    border: 1px solid var(--border);
    border-radius: 6px;
    color: var(--text-muted);
    font-size: 12px;
    font-variant-numeric: tabular-nums;
  }
</style>
