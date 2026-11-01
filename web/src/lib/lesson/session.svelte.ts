import type { ProgressStore } from "../progress/store";
import type { Mark, SceneOpts } from "../rink/paint";
import type { LessonHandle } from "../wasm/rink_wasm.js";
import type { LoadedLesson } from "./content";
import type { Answer, Caption, Graded, LessonMeta, PlayEntity, Step, Which } from "./types";

/** What the user answered on a graded step, and how it was graded. */
export type Answered = { answer: Answer; graded: Graded };

/** A stretch of a play that plays to `to` and stops there. */
export type Segment = { from: number; to: number };

/** One open lesson: which step, what the rink shows, and this run's answers. */
export class LessonSession {
  readonly handle: LessonHandle;
  readonly meta: LessonMeta;
  readonly steps: Step[];
  /** The user's player. */
  readonly me: string;
  readonly hasWrong: boolean;
  readonly #entities: Map<string, PlayEntity>;
  readonly #captions: Record<Which, Caption[]>;
  readonly #progress: ProgressStore;

  index = $state(0);
  finished = $state(false);
  which = $state<Which>("main");
  t = $state(0);
  playing = $state(false);
  speed = $state(0.5);
  segment = $state<Segment | null>(null);
  /** This run's responses, by step id. */
  responses = $state.raw<Record<string, Answered>>({});

  step: Step;
  response: Answered | null;
  /** The user's player is hidden while a spot step waits for the tap. */
  hidden: string[];
  marks: Mark[];
  caption: string | null;

  #frame = 0;

  constructor({ handle, entities, hasWrong }: LoadedLesson, progress: ProgressStore) {
    this.handle = handle;
    this.#progress = progress;
    this.meta = handle.meta();
    this.steps = handle.steps();
    this.hasWrong = hasWrong;
    this.#entities = new Map(entities.map((e) => [e.id, e]));
    const me = entities.find((e) => e.is_me);
    if (!me) throw new Error(`${this.meta.id}: the play has no player marked as me`);
    this.me = me.id;
    const captions = (which: Which) =>
      (handle.captions(which) as Caption[]).toSorted((a, b) => a.t_ms - b.t_ms);
    this.#captions = { main: captions("main"), wrong: this.hasWrong ? captions("wrong") : [] };

    this.step = $derived(this.steps[this.index]);
    this.response = $derived(this.responses[this.step.id] ?? null);
    this.hidden = $derived(this.step.type === "spot" && !this.response ? [this.me] : []);
    this.marks = $derived(this.response?.graded.marks ?? []);
    this.caption = $derived(this.#captions[this.which].findLast((c) => c.t_ms <= this.t)?.text ?? null);
    this.enter();
  }

  duration(which: Which = this.which): number {
    return this.handle.duration_ms(which);
  }

  /** "their LD", "our C": how the user names a player. */
  name(id: string): string {
    const e = this.#entities.get(id);
    if (!e) return id;
    const mine = this.#entities.get(this.me)?.team;
    return `${e.team === mine ? "our" : "their"} ${e.role ?? `#${e.number}`}`;
  }

  /** Grade the current step's answer and record the attempt. A step is answered once per run. */
  respond(answer: Answer) {
    const { id } = this.step;
    if (this.responses[id]) return;
    const graded: Graded = this.handle.grade(id, answer);
    this.responses = { ...this.responses, [id]: { answer, graded } };
    void this.#progress.record({
      stepId: id,
      lessonId: this.meta.id,
      grade: graded.grade,
      answeredAt: new Date().toISOString(),
    });
  }

  /** A tap on the rink answers a spot step that is still waiting for one. */
  tap(xPx: number, yPx: number, opts: SceneOpts) {
    if (this.step.type !== "spot" || this.response) return;
    this.respond({ kind: "spot", at: this.handle.to_rink("main", xPx, yPx, opts) });
  }

  /** Play `which` on from `from` to its end, as a choice's "watch what happens". */
  watchOn(which: Which, from: number) {
    this.playSegment({ from, to: this.handle.duration_ms(which) }, which);
  }

  next() {
    if (this.index === this.steps.length - 1) {
      this.pause();
      this.finished = true;
      void this.#progress.markStudied(this.meta.id, new Date().toISOString());
      return;
    }
    this.index += 1;
    this.enter();
  }

  back() {
    if (this.finished) this.finished = false;
    else if (this.index === 0) return;
    else this.index -= 1;
    this.enter();
  }

  /** Set the rink up for the current step: which play, where, and whether it runs. */
  enter() {
    const step = this.step;
    this.pause();
    this.segment = null;
    this.speed = 0.5;
    switch (step.type) {
      case "card":
        break;
      case "watch":
        this.which = step.play;
        this.speed = step.speed;
        this.playSegment({ from: step.from_ms, to: step.to_ms });
        break;
      case "choice":
      case "spot":
        this.which = "main";
        this.seek(step.at_ms);
        break;
      case "contrast":
        this.which = this.hasWrong ? "wrong" : "main";
        this.playSegment({ from: step.at_ms, to: step.to_ms });
        break;
    }
  }

  /** Show `which` and play `segment` from its start. */
  playSegment(segment: Segment, which: Which = this.which) {
    this.pause();
    this.which = which;
    this.segment = segment;
    this.seek(segment.from);
    this.play();
  }

  seek(t: number) {
    this.t = Math.min(Math.max(t, 0), this.duration());
  }

  play() {
    if (this.playing) return;
    const from = this.segment?.from ?? 0;
    const end = () => Math.min(this.segment?.to ?? Infinity, this.duration());
    if (this.t >= end()) this.t = from;
    this.playing = true;
    let last = performance.now();
    const tick = (now: number) => {
      this.t = Math.min(this.t + (now - last) * this.speed, end());
      last = now;
      if (this.t >= end()) this.playing = false;
      else this.#frame = requestAnimationFrame(tick);
    };
    this.#frame = requestAnimationFrame(tick);
  }

  pause() {
    this.playing = false;
    cancelAnimationFrame(this.#frame);
  }

  /** Stop playback and release the wasm lesson. */
  close() {
    this.pause();
    this.handle.free();
  }
}
