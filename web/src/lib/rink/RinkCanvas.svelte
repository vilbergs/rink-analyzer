<script lang="ts">
  import type { PlayerState, View } from "../player.svelte";
  import { paint, type Scene } from "./paint";

  const MAX_ZOOM = 8;

  let { player }: { player: PlayerState } = $props();

  let canvas: HTMLCanvasElement;
  let width = $state(0);
  let height = $state(0);
  let dpr = $state(1);
  let drag: { x: number; y: number } | null = null;

  $effect(() => {
    const { t, selected, reviewJson, view } = player;
    const ctx = canvas.getContext("2d");
    if (!ctx || width === 0 || height === 0) return;
    canvas.width = Math.round(width * dpr);
    canvas.height = Math.round(height * dpr);
    const scene: Scene = player.handle.scene(Math.round(t), { ...viewOpts(view), selected, review: reviewJson });
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    paint(ctx, scene);
  });

  function viewOpts({ zoom, pan, flipped }: View) {
    return { width, height, zoom, pan: [pan.x, pan.y], flipped };
  }

  function onclick(e: MouseEvent) {
    const t = Math.round(player.t);
    player.selected = player.handle.hit_test(t, e.offsetX, e.offsetY, viewOpts(player.view)) ?? null;
  }

  function onwheel(e: WheelEvent) {
    e.preventDefault();
    const { zoom, pan } = player.view;
    if (e.shiftKey) {
      panBy(-e.deltaX, -e.deltaY);
      return;
    }
    const next = Math.min(Math.max(zoom * Math.exp(-e.deltaY * 0.002), 1), MAX_ZOOM);
    if (next === 1) {
      player.fitView();
      return;
    }
    const k = next / zoom;
    const ax = e.offsetX - width / 2;
    const ay = e.offsetY - height / 2;
    player.view = { ...player.view, zoom: next, pan: { x: ax - (ax - pan.x) * k, y: ay - (ay - pan.y) * k } };
  }

  function panBy(dx: number, dy: number) {
    const { pan } = player.view;
    player.view = { ...player.view, pan: { x: pan.x + dx, y: pan.y + dy } };
  }

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 1) return;
    e.preventDefault();
    canvas.setPointerCapture(e.pointerId);
    drag = { x: e.clientX, y: e.clientY };
  }

  function onpointermove(e: PointerEvent) {
    if (!drag) return;
    panBy(e.clientX - drag.x, e.clientY - drag.y);
    drag = { x: e.clientX, y: e.clientY };
  }

  function onpointerup() {
    drag = null;
  }
</script>

<svelte:window bind:devicePixelRatio={dpr} />

<div class="rink" bind:clientWidth={width} bind:clientHeight={height}>
  <canvas
    bind:this={canvas}
    {onclick}
    {onwheel}
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    onpointercancel={onpointerup}
  ></canvas>
</div>

<style>
  .rink {
    position: relative;
    min-height: 0;
    overflow: hidden;
  }
  canvas {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    display: block;
  }
</style>
