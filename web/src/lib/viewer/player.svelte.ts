import { FIT, type View } from "../rink/paint";
import type { PlayHandle } from "../wasm/rink_wasm.js";

export type Caption = { t_ms: number; text: string };
export type PlayEvent = { t_ms: number; kind: string; actors: string[]; note?: string };

export const SPEEDS = [0.25, 0.5, 1, 2] as const;
export type Speed = (typeof SPEEDS)[number];

export class PlayerState {
  readonly handle: PlayHandle;
  readonly reviewJson: string | null;
  readonly duration: number;
  readonly title: string;
  readonly captions: Caption[];
  readonly events: PlayEvent[];

  t = $state(0);
  playing = $state(false);
  speed = $state<Speed>(1);
  selected = $state<string | null>(null);
  view = $state<View>(FIT);
  caption = $derived.by(() => this.captions.findLast((c) => c.t_ms <= this.t) ?? null);

  #frame = 0;

  constructor(handle: PlayHandle, reviewJson: string | null) {
    this.handle = handle;
    this.reviewJson = reviewJson;
    this.duration = handle.duration_ms();
    this.title = handle.title();
    this.captions = (handle.captions() as Caption[]).toSorted((a, b) => a.t_ms - b.t_ms);
    this.events = handle.events() as PlayEvent[];
  }

  seek(t: number) {
    this.t = Math.min(Math.max(t, 0), this.duration);
  }

  toggle() {
    if (this.playing) this.pause();
    else this.play();
  }

  play() {
    if (this.playing) return;
    if (this.t >= this.duration) this.t = 0;
    this.playing = true;
    let last = performance.now();
    const step = (now: number) => {
      this.seek(this.t + (now - last) * this.speed);
      last = now;
      if (this.t >= this.duration) this.playing = false;
      else this.#frame = requestAnimationFrame(step);
    };
    this.#frame = requestAnimationFrame(step);
  }

  pause() {
    this.playing = false;
    cancelAnimationFrame(this.#frame);
  }

  fitView() {
    this.view = { ...FIT, flipped: this.view.flipped };
  }

  cycleSpeed() {
    this.speed = SPEEDS[(SPEEDS.indexOf(this.speed) + 1) % SPEEDS.length];
  }
}
