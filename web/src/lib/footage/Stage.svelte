<script lang="ts">
  import type { Box, Track } from "./api";
  import type { Review } from "./review.svelte";

  let { review, original, preview }: { review: Review; original: string; preview: string } = $props();

  const COLOR = { track: "#b7e5ff", selected: "#7df0b0", partner: "#c9baff", me: "#ffd260", ink: "#11161c" };

  let viewport = $state<HTMLDivElement>();
  let video = $state<HTMLVideoElement>();
  let canvas = $state<HTMLCanvasElement>();
  let box = $state({ width: 0, height: 0 });
  let zoom = $state(1);
  let offset = $state({ x: 0, y: 0 });
  /** The natural size of the video being played, once it has loaded. */
  let shown = $state({ width: 0, height: 0 });
  let failed = $state(false);
  /** Play the uploaded clip itself until the browser turns out not to be able to. */
  let full = $state(true);
  const src = $derived(full ? original : preview);

  function unplayable() {
    if (full) full = false;
    else failed = true;
  }
  /** Where each drawn name tag sits, so a click on the tag selects its skater. */
  let tags = new Map<number, Box>();

  /**
   * The frame size the tracker worked at. Boxes are in these pixels, whatever size the video being
   * played is, so the overlay maps them onto the video's rectangle as fractions of this size.
   */
  const size = $derived({ width: review.frames[0].width, height: review.frames[0].height });
  /** The largest rectangle of the playing video's own shape that fits the viewport. */
  const fitted = $derived.by(() => {
    const natural = shown.width ? shown : size;
    const scale = Math.min(box.width / natural.width, box.height / natural.height) || 0;
    return { width: natural.width * scale, height: natural.height * scale };
  });

  /**
   * The overlay is drawn at screen resolution, so boxes and names stay sharp and the same size
   * however far the video is zoomed. `unit` is one CSS pixel in frame coordinates.
   */
  const backing = $derived.by(() => {
    const density = (globalThis.devicePixelRatio || 1) * Math.min(zoom, 3);
    return { width: Math.max(1, Math.round(fitted.width * density)), height: Math.max(1, Math.round(fitted.height * density)) };
  });
  const unit = $derived(size.width / (fitted.width * zoom || 1));

  export function fit() {
    zoom = 1;
    offset = { x: 0, y: 0 };
  }

  function clampOffset(x: number, y: number) {
    const xLimit = Math.max(0, (fitted.width * zoom - box.width) / 2);
    const yLimit = Math.max(0, (fitted.height * zoom - box.height) / 2);
    offset = { x: Math.max(-xLimit, Math.min(xLimit, x)), y: Math.max(-yLimit, Math.min(yLimit, y)) };
  }

  /** Zoom while keeping the point under the cursor in place. */
  function zoomAt(next: number, event: { clientX: number; clientY: number }) {
    const scale = Math.max(1, Math.min(6, next));
    const ratio = scale / zoom;
    const bounds = viewport!.getBoundingClientRect();
    const x = event.clientX - (bounds.left + bounds.width / 2);
    const y = event.clientY - (bounds.top + bounds.height / 2);
    zoom = scale;
    clampOffset((Number.isFinite(x) ? x : 0) * (1 - ratio) + offset.x * ratio, (Number.isFinite(y) ? y : 0) * (1 - ratio) + offset.y * ratio);
  }

  function onwheel(event: WheelEvent) {
    if (!event.ctrlKey && zoom === 1) return;
    event.preventDefault();
    const unit = event.deltaMode === 1 ? 16 : event.deltaMode === 2 ? box.height : 1;
    // A trackpad pinch arrives as a wheel event with ctrlKey set.
    if (event.ctrlKey) zoomAt(zoom * Math.exp((-event.deltaY * unit) / 200), event);
    else clampOffset(offset.x - event.deltaX * unit, offset.y - event.deltaY * unit);
  }

  // Safari reports a pinch as gesture events with a cumulative scale.
  let gestureZoom: number | undefined;
  type GestureEvent = Event & { scale: number; clientX: number; clientY: number };
  function ongesture(event: Event) {
    event.preventDefault();
    if (event.type === "gesturestart") gestureZoom = zoom;
    else if (event.type === "gestureend") gestureZoom = undefined;
    else if (gestureZoom !== undefined) zoomAt(gestureZoom * (event as GestureEvent).scale, event as GestureEvent);
  }

  // A press that moves pans the zoomed frame; one that stays put selects a skater.
  let press: { id: number; x: number; y: number; moved: boolean } | undefined;
  function onpointerdown(event: PointerEvent) {
    if (event.button !== 0) return;
    press = { id: event.pointerId, x: event.clientX, y: event.clientY, moved: false };
    viewport!.setPointerCapture(event.pointerId);
  }
  function onpointermove(event: PointerEvent) {
    if (press?.id !== event.pointerId) return;
    const dx = event.clientX - press.x;
    const dy = event.clientY - press.y;
    if (!press.moved && Math.hypot(dx, dy) < 4) return;
    press.moved = true;
    if (zoom > 1) clampOffset(offset.x + dx, offset.y + dy);
    press.x = event.clientX;
    press.y = event.clientY;
  }
  function onpointerup(event: PointerEvent) {
    if (press?.id !== event.pointerId) return;
    const moved = press.moved;
    press = undefined;
    if (!moved) pick(event);
  }

  function pick(event: PointerEvent) {
    if (!canvas || video?.seeking) return;
    const bounds = canvas.getBoundingClientRect();
    const x = ((event.clientX - bounds.left) * size.width) / bounds.width;
    const y = ((event.clientY - bounds.top) * size.height) / bounds.height;
    const inside = ([l, t, r, b]: Box) => x >= l && x <= r && y >= t && y <= b;
    const detected = review.frame.tracks.filter((track) => !track.predicted);
    const tagged = detected.filter((track) => tags.has(track.id) && inside(tags.get(track.id)!));
    const hits = tagged.length ? tagged : detected.filter((track) => inside(track.xyxy));
    if (!hits.length) {
      if (!event.shiftKey) review.select(null);
      return;
    }
    // Where boxes overlap, the smallest is the skater in front of the click.
    const area = ({ xyxy: [l, t, r, b] }: Track) => (r - l) * (b - t);
    hits.sort((a, b) => area(a) - area(b));
    review.seek(review.index);
    review.selectTrack(hits[0], event.shiftKey);
  }

  function drawBox(context: CanvasRenderingContext2D, track: Track, color: string, width: number, text: string) {
    const [left, top, right, bottom] = track.xyxy;
    context.setLineDash(track.predicted ? [7 * unit, 5 * unit] : []);
    for (const [stroke, line] of [[COLOR.ink, width + 2], [color, width]] as const) {
      context.strokeStyle = stroke;
      context.lineWidth = line * unit;
      context.strokeRect(left, top, right - left, bottom - top);
    }
    context.setLineDash([]);
    context.font = `600 ${13 * unit}px system-ui, sans-serif`;
    context.textBaseline = "middle";
    const tagWidth = context.measureText(text).width + 10 * unit;
    const tagHeight = 18 * unit;
    const x = Math.max(0, Math.min(left, size.width - tagWidth));
    const y = Math.max(0, top - tagHeight - unit);
    context.fillStyle = COLOR.ink;
    context.fillRect(x, y, tagWidth, tagHeight);
    context.fillStyle = color;
    context.fillText(text, x + 5 * unit, y + tagHeight / 2);
    tags.set(track.id, [x, y, x + tagWidth, y + tagHeight]);
  }

  $effect(() => {
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;
    const frame = review.frame;
    canvas.width = backing.width;
    canvas.height = backing.height;
    context.setTransform(backing.width / size.width, 0, 0, backing.height / size.height, 0, 0);
    tags = new Map();
    const selected = review.resolve(frame, review.selected);
    const companion = review.companion(frame);
    const partner = companion?.id === selected?.id ? undefined : companion;
    const me = review.me.track;
    // Older previews have every box burned into the video; only the highlights are drawn over those.
    if (full || frame.interactive) {
      for (const track of frame.tracks) {
        if (track !== selected && track !== partner && track !== me) {
          drawBox(context, track, COLOR.track, 1.5, review.tag(frame, track));
        }
      }
    }
    if (selected && partner && !selected.predicted && !partner.predicted) {
      // A line between the two skaters' feet shows the gap between them.
      context.beginPath();
      context.moveTo((selected.xyxy[0] + selected.xyxy[2]) / 2, selected.xyxy[3]);
      context.lineTo((partner.xyxy[0] + partner.xyxy[2]) / 2, partner.xyxy[3]);
      context.setLineDash([5 * unit, 4 * unit]);
      context.strokeStyle = COLOR.selected;
      context.lineWidth = 2 * unit;
      context.stroke();
      context.setLineDash([]);
    }
    if (partner && partner !== me) drawBox(context, partner, COLOR.partner, 3, review.tag(frame, partner));
    if (me && me !== selected) drawBox(context, me, COLOR.me, 3, "You");
    if (selected) {
      const name = selected === me ? "You" : review.tag(frame, selected);
      drawBox(context, selected, selected === me ? COLOR.me : COLOR.selected, 3.5, name);
    }
  });

  $effect(() => {
    if (!video) return;
    review.attach(video, full ? 0 : review.first);
    const element = video;
    let handle = 0;
    const present = (_: number, metadata: { mediaTime: number }) => {
      if (!element.seeking) review.tick(metadata.mediaTime);
      handle = element.requestVideoFrameCallback(present);
    };
    handle = element.requestVideoFrameCallback(present);
    return () => {
      element.cancelVideoFrameCallback(handle);
      review.attach(undefined);
    };
  });

  $effect(() => {
    if (!viewport) return;
    const element = viewport;
    const observer = new ResizeObserver(() => {
      box = { width: element.clientWidth, height: element.clientHeight };
      clampOffset(offset.x, offset.y);
    });
    observer.observe(element);
    // These must be able to cancel the browser's own page zoom, so they cannot be passive.
    const gestures = ["gesturestart", "gesturechange", "gestureend"];
    element.addEventListener("wheel", onwheel, { passive: false });
    for (const name of gestures) element.addEventListener(name, ongesture, { passive: false });
    return () => {
      observer.disconnect();
      element.removeEventListener("wheel", onwheel);
      for (const name of gestures) element.removeEventListener(name, ongesture);
    };
  });
</script>

<!-- svelte-ignore a11y_no_static_element_interactions -->
<div
  class="viewport"
  class:zoomed={zoom > 1}
  bind:this={viewport}
  {onpointerdown}
  {onpointermove}
  {onpointerup}
  onpointercancel={() => (press = undefined)}
  ondblclick={fit}
>
  <div
    class="frame"
    style:width="{fitted.width}px"
    style:height="{fitted.height}px"
    style:transform="translate({offset.x}px, {offset.y}px) scale({zoom})"
  >
    <!-- The tracking preview has no audio track. -->
    <!-- svelte-ignore a11y_media_has_caption -->
    <video
      bind:this={video}
      {src}
      playsinline
      muted
      preload="auto"
      onplay={() => review.setPlaying(true)}
      onpause={() => review.setPlaying(false)}
      onended={() => review.setPlaying(false)}
      onloadedmetadata={(event) => {
        const { videoWidth: width, videoHeight: height } = event.currentTarget;
        shown = { width, height };
        // Some browsers load a clip they cannot decode and show nothing.
        if (!width) unplayable();
        else review.resync();
      }}
      onerror={unplayable}
    ></video>
    <canvas bind:this={canvas} aria-label="Tracked skaters; click one to select"
    ></canvas>
  </div>
  {#if failed}
    <p class="notice" role="alert">This browser could not play the tracking preview.</p>
  {/if}
  {#if zoom > 1}
    <button class="fit" onclick={fit} onpointerdown={(e) => e.stopPropagation()}>{Math.round(zoom * 100)}% · Fit</button>
  {/if}
  {#if shown.width}
    <span class="legacy">
      {#if full}
        Original · {shown.width}×{shown.height}
      {:else}
        Preview · {shown.width}×{shown.height} · this browser cannot play the original
      {/if}
    </span>
  {/if}
</div>

<style>
  .viewport {
    position: relative;
    display: grid;
    place-items: center;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    background: #000;
    cursor: crosshair;
    touch-action: none;
    user-select: none;
  }
  .viewport.zoomed {
    cursor: grab;
  }
  .frame {
    position: relative;
    flex: none;
    transform-origin: center;
    will-change: transform;
  }
  video,
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
  }
  video {
    pointer-events: none;
  }
  .notice {
    position: absolute;
    margin: 0;
    padding: 8px 12px;
    border-radius: 6px;
    background: var(--panel);
    color: var(--danger);
  }
  .fit,
  .legacy {
    position: absolute;
    right: 10px;
    bottom: 10px;
    padding: 4px 9px;
    border: 1px solid var(--border);
    border-radius: 6px;
    background: color-mix(in srgb, var(--panel) 85%, transparent);
    color: var(--text-soft);
    font: inherit;
    font-size: 12px;
  }
  .fit {
    cursor: pointer;
  }
  .legacy {
    right: auto;
    left: 10px;
  }
</style>
