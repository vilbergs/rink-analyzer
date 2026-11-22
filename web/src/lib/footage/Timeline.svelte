<script lang="ts">
  import type { Review } from "./review.svelte";
  import { clock, type Run } from "./tracks";

  let { review }: { review: Review } = $props();

  let area = $state<HTMLDivElement>();
  let drag:
    | { kind: "scrub" }
    | { kind: "range"; from: number; x: number; moved: boolean }
    | { kind: "trim"; edge: "start" | "end" | "decision" }
    | undefined;

  const percent = (ms: number) => ((ms - review.first) / review.span) * 100;
  const at = (index: number) => percent(review.frames[index].source_ms);
  const span = (run: Run) => `left:${at(run.start)}%;width:${Math.max(0.15, at(run.end) - at(run.start))}%`;
  const labelName = (id: string) => review.identity.play_labels.find((label) => label.id === id)?.name ?? id;

  /** Second marks along the ruler, spaced so about ten fit. */
  const marks = $derived.by(() => {
    const step = [1, 2, 5, 10, 15, 30, 60, 120, 300].find((s) => review.span / 1000 / s <= 10) ?? 600;
    const list: number[] = [];
    for (let s = 0; s * 1000 <= review.span; s += step) list.push(review.first + s * 1000);
    return list;
  });

  const notes = $derived([
    ...review.moments
      .filter((moment) => moment.view.clip === review.clip.id)
      .map((moment) => ({ ms: moment.view.source_ms, text: moment.text, open: () => review.openMoment(moment) })),
    ...review.localNotes.map((note) => ({
      ms: review.frames[note.index].source_ms,
      text: note.text,
      open: () => review.openLocalNote(note),
    })),
  ]);

  function msAt(event: PointerEvent | MouseEvent): number {
    const bounds = area!.getBoundingClientRect();
    const fraction = Math.max(0, Math.min(1, (event.clientX - bounds.left) / bounds.width));
    return review.first + fraction * review.span;
  }

  function onpointerdown(event: PointerEvent) {
    if (event.button !== 0) return;
    const target = event.target as HTMLElement;
    if (target.closest("button")) return;
    const edge = target.closest<HTMLElement>("[data-edge]")?.dataset.edge as "start" | "end" | "decision" | undefined;
    if (edge) drag = { kind: "trim", edge };
    else if (target.closest("[data-lane=plays]") && review.run) {
      drag = { kind: "range", from: msAt(event), x: event.clientX, moved: false };
    } else {
      drag = { kind: "scrub" };
      review.seekMs(msAt(event));
    }
    area!.setPointerCapture(event.pointerId);
    event.preventDefault();
  }

  function onpointermove(event: PointerEvent) {
    if (!drag) return;
    if (drag.kind === "scrub") review.seekMs(msAt(event));
    else if (drag.kind === "trim") review.trim(drag.edge, msAt(event));
    else if (drag.moved || Math.abs(event.clientX - drag.x) > 4) {
      if (!drag.moved) review.draft = null;
      drag.moved = true;
      review.range(drag.from, msAt(event));
    }
  }

  function onpointerup(event: PointerEvent) {
    // A click on the plays lane that never became a drag is a seek, like anywhere else.
    if (drag?.kind === "range" && !drag.moved) review.seekMs(msAt(event));
    drag = undefined;
  }

  /** Double-click a stretch where a skater is in view to watch just that. */
  function ondblclick(event: MouseEvent) {
    const lane = (event.target as HTMLElement).closest<HTMLElement>("[data-lane]")?.dataset.lane;
    const list = lane === "selection" ? review.appearances : lane === "you" ? review.myAppearances : null;
    const index = review.indexAt(msAt(event));
    const run = list?.find((r) => r.start <= index && index <= r.end);
    if (run) review.playRange(run.start, run.end);
  }
</script>

<div class="timeline">
  <div class="transport">
    <div class="group">
      <button title="Previous camera cut ([)" aria-label="Previous camera cut" disabled={!review.cuts.length} onclick={() => review.jumpCut(-1)}>
        <svg viewBox="0 0 16 16"><path d="M3 3v10M13 3 6 8l7 5z" /></svg>
      </button>
      <button title="Back one frame (←)" aria-label="Back one frame" onclick={() => review.seek(review.index - 1)}>
        <svg viewBox="0 0 16 16"><path d="M11 3 5 8l6 5z" /></svg>
      </button>
      <button class="play" title="Play or pause (Space)" aria-label={review.playing ? "Pause" : "Play"} onclick={() => review.toggle()}>
        {#if review.playing}
          <svg viewBox="0 0 16 16"><path d="M4 3h3v10H4zM9 3h3v10H9z" /></svg>
        {:else}
          <svg viewBox="0 0 16 16"><path d="M4 2.5 13 8l-9 5.5z" /></svg>
        {/if}
      </button>
      <button title="Forward one frame (→)" aria-label="Forward one frame" onclick={() => review.seek(review.index + 1)}>
        <svg viewBox="0 0 16 16"><path d="m5 3 6 5-6 5z" /></svg>
      </button>
      <button title="Next camera cut (])" aria-label="Next camera cut" disabled={!review.cuts.length} onclick={() => review.jumpCut(1)}>
        <svg viewBox="0 0 16 16"><path d="M13 3v10M3 3l7 5-7 5z" /></svg>
      </button>
    </div>
    <span class="time"><b>{clock(review.frame.source_ms)}</b> / {clock(review.first + review.span)}</span>
    <button class="text" title="Playback speed" onclick={() => review.cycleRate()}>{review.rate}×</button>
    <span class="count">
      {review.frame.tracks.filter((track) => !track.predicted).length} skaters in view
    </span>
    <span class="spacer"></span>
    {#if review.run}
      <button class="text" title="Label a play around this frame (P). Or drag across the Plays lane." onclick={() => review.startPlay()}>
        + Play
      </button>
    {/if}
    <button class="text" title="Undo the last identity edit (⌘Z)" disabled={!review.identity.undo || review.saving} onclick={() => review.undo()}>
      Undo
    </button>
  </div>

  <div class="lanes">
    <div class="labels">
      <span></span>
      <span>Plays</span>
      <span class="selection" title={review.name(review.selected)}>{review.name(review.selected) || "Selection"}</span>
      <span class="you">You</span>
      <span>Notes</span>
    </div>
    <!-- svelte-ignore a11y_no_static_element_interactions -->
    <div class="area" bind:this={area} {onpointerdown} {onpointermove} {onpointerup} onpointercancel={() => (drag = undefined)} {ondblclick}>
      <div class="lane ruler">
        {#each marks as ms (ms)}
          <span class="mark" style:left="{percent(ms)}%">{clock(ms).slice(0, -2)}</span>
        {/each}
        {#each review.cuts as cut (cut)}
          <i class="cut" style:left="{at(cut)}%" title="Camera cut at {clock(review.frames[cut].source_ms)}"></i>
        {/each}
      </div>

      <div class="lane" data-lane="plays">
        {#each review.plays as play (play.id)}
          {#if play.id !== review.draft?.id}
            <button
              class="bar"
              class:changed={review.playChanged(play)}
              style="left:{percent(play.start_ms)}%;width:{percent(play.end_ms) - percent(play.start_ms)}%"
              title="{labelName(play.primary)}{play.cue ? ' · ' + play.cue : ''}"
              onclick={() => review.editPlay(play)}
            >
              <span>{labelName(play.primary)}</span>
              <i class="decision" style:left="{((play.decision_ms - play.start_ms) / (play.end_ms - play.start_ms)) * 100}%"></i>
            </button>
          {/if}
        {/each}
        {#if review.draft}
          {@const draft = review.draft}
          <div class="bar draft" style="left:{percent(draft.start_ms)}%;width:{percent(draft.end_ms) - percent(draft.start_ms)}%">
            <i class="handle start" data-edge="start" title="Drag to move the start"></i>
            <span>{draft.primary ? labelName(draft.primary) : "New play"}</span>
            <i
              class="decision grab"
              data-edge="decision"
              title="Decision frame; drag to move"
              style:left="{((draft.decision_ms - draft.start_ms) / (draft.end_ms - draft.start_ms)) * 100}%"
            ></i>
            <i class="handle end" data-edge="end" title="Drag to move the end"></i>
          </div>
        {/if}
      </div>

      <div class="lane" data-lane="selection">
        {#each review.appearances as run (run.start)}
          <i class="seen" style={span(run)}></i>
        {/each}
      </div>
      <div class="lane" data-lane="you">
        {#each review.myAppearances as run (run.start)}
          <i class="seen you" style={span(run)}></i>
        {/each}
      </div>
      <div class="lane">
        {#each notes as note (note.ms + note.text)}
          <button class="note" style:left="{percent(note.ms)}%" title={note.text} aria-label="Note: {note.text}" onclick={note.open}></button>
        {/each}
      </div>

      <div class="playhead" style:left="{percent(review.frame.source_ms)}%"></div>
    </div>
  </div>
</div>

<style>
  .timeline {
    display: grid;
    grid-template-rows: auto auto;
    border-top: 1px solid var(--border);
    min-width: 0;
    background: var(--panel);
    user-select: none;
  }
  .transport {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 6px 12px;
    border-bottom: 1px solid var(--border);
  }
  .group {
    display: flex;
    gap: 2px;
  }
  button {
    font: inherit;
    color: var(--text-soft);
    background: none;
    border: 0;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .group button {
    display: grid;
    place-items: center;
    width: 30px;
    height: 28px;
    border-radius: 6px;
  }
  .group button:hover:not(:disabled),
  .text:hover:not(:disabled) {
    background: var(--raised);
    color: var(--text);
  }
  .group .play {
    width: 38px;
    background: var(--raised);
    color: var(--text);
  }
  svg {
    width: 14px;
    height: 14px;
    fill: currentColor;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linejoin: round;
  }
  .time,
  .text {
    white-space: nowrap;
  }
  .time {
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .time b {
    color: var(--text);
    font-weight: 600;
  }
  .text {
    height: 28px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 6px;
    font-size: 13px;
    font-variant-numeric: tabular-nums;
  }
  .count {
    color: var(--text-dim);
    font-size: 12px;
  }
  .spacer {
    flex: 1;
  }
  @media (max-width: 860px) {
    .transport {
      gap: 8px;
      padding-inline: 8px;
    }
    .count {
      display: none;
    }
    .lanes {
      grid-template-columns: 64px 1fr;
    }
  }

  .lanes {
    display: grid;
    grid-template-columns: 92px 1fr;
    padding: 4px 12px 8px 0;
  }
  .labels,
  .area {
    display: grid;
    grid-template-rows: 20px 26px 16px 16px 18px;
    row-gap: 3px;
  }
  .labels span {
    overflow: hidden;
    padding: 0 8px 0 12px;
    color: var(--text-dim);
    font-size: 11px;
    line-height: 1;
    text-overflow: ellipsis;
    white-space: nowrap;
    align-self: center;
  }
  .labels .selection {
    color: #7df0b0;
  }
  .labels .you {
    color: #ffd260;
  }
  .area {
    position: relative;
    cursor: pointer;
    touch-action: none;
  }
  .lane {
    position: relative;
    border-radius: 3px;
    background: color-mix(in srgb, var(--raised) 55%, transparent);
  }
  .ruler {
    background: none;
    border-bottom: 1px solid var(--border);
  }
  .mark {
    position: absolute;
    top: 2px;
    padding-left: 4px;
    border-left: 1px solid var(--border);
    color: var(--text-dim);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
    line-height: 16px;
    pointer-events: none;
  }
  .cut {
    position: absolute;
    bottom: -1px;
    width: 0;
    height: 0;
    margin-left: -4px;
    border: 4px solid transparent;
    border-bottom-color: var(--warn);
  }
  .bar {
    position: absolute;
    top: 0;
    bottom: 0;
    min-width: 6px;
    padding: 0 6px;
    overflow: hidden;
    border: 1px solid color-mix(in srgb, var(--accent) 70%, transparent);
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 28%, var(--panel));
    color: var(--text);
    font-size: 11px;
    line-height: 24px;
    text-align: left;
    white-space: nowrap;
  }
  .bar.changed {
    border-style: dashed;
    opacity: 0.7;
  }
  .bar.draft {
    overflow: visible;
    border-color: var(--feedback);
    background: color-mix(in srgb, var(--feedback) 30%, var(--panel));
    cursor: default;
  }
  .bar span {
    position: relative;
    pointer-events: none;
  }
  .decision {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: currentColor;
    opacity: 0.6;
    pointer-events: none;
  }
  .decision.grab {
    top: -3px;
    bottom: -3px;
    width: 8px;
    margin-left: -4px;
    border-radius: 2px;
    background: var(--feedback);
    opacity: 1;
    cursor: ew-resize;
    pointer-events: auto;
  }
  .handle {
    position: absolute;
    top: -1px;
    bottom: -1px;
    width: 9px;
    background: var(--feedback);
    cursor: ew-resize;
  }
  .handle.start {
    left: -5px;
    border-radius: 4px 0 0 4px;
  }
  .handle.end {
    right: -5px;
    border-radius: 0 4px 4px 0;
  }
  .seen {
    position: absolute;
    top: 3px;
    bottom: 3px;
    border-radius: 2px;
    background: #7df0b0;
    pointer-events: none;
  }
  .seen.you {
    background: #ffd260;
  }
  .note {
    position: absolute;
    top: 3px;
    width: 12px;
    height: 12px;
    margin-left: -6px;
    padding: 0;
    border-radius: 2px;
    background: var(--accent-soft);
    transform: rotate(45deg);
  }
  .playhead {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: var(--text);
    pointer-events: none;
  }
  .playhead::before {
    content: "";
    position: absolute;
    top: 0;
    left: -4px;
    border: 5px solid transparent;
    border-top-color: var(--text);
  }
</style>
