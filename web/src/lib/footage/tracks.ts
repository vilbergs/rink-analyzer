/** Pure helpers over a clip's tracked frames. No DOM, no state. */

import type { Frame, Gear, PlayerDetails } from "./api";

export const trackLabel = (id: number) => "S" + String(id).padStart(2, "0");

export function clock(ms: number): string {
  const tenths = Math.floor(ms / 100);
  return `${Math.floor(tenths / 600)}:${String(Math.floor(tenths / 10) % 60).padStart(2, "0")}.${tenths % 10}`;
}

export function duration(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  return `${Math.floor(seconds / 60)}:${String(seconds % 60).padStart(2, "0")}`;
}

/** The last frame at or before `ms`, or the first frame. */
export function indexAtMs(frames: Frame[], ms: number): number {
  let low = 0;
  let high = frames.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (frames[middle].source_ms <= ms) low = middle + 1;
    else high = middle;
  }
  return Math.max(0, low - 1);
}

/** Frames where the camera cut to a new shot. */
export function cuts(frames: Frame[]): number[] {
  const found: number[] = [];
  for (let i = 1; i < frames.length; i++) {
    if (frames[i].shot != null && frames[i].shot !== frames[i - 1].shot) found.push(i);
  }
  return found;
}

export type Run = { start: number; end: number };

/** Unbroken stretches of frames that satisfy `visible`, as inclusive frame indexes. */
export function runs(frames: Frame[], visible: (frame: Frame) => boolean): Run[] {
  const found: Run[] = [];
  let last: Run | undefined;
  for (let i = 0; i < frames.length; i++) {
    if (!visible(frames[i])) continue;
    if (last && last.end === i - 1) last.end = i;
    else found.push((last = { start: i, end: i }));
  }
  return found;
}

export type TrackSummary = { frames: number; jersey: string | null };

const JERSEYS = ["red/orange", "blue", "dark", "light", null];

/** How long each track is detected, and its jersey color when one color clearly dominates. */
export function summarize(frames: Frame[]): Map<number, TrackSummary> {
  const colors = new Map<number, number[]>();
  const summary = new Map<number, TrackSummary>();
  for (const frame of frames) {
    for (const track of frame.tracks) {
      if (track.predicted) continue;
      let entry = summary.get(track.id);
      if (!entry) {
        summary.set(track.id, (entry = { frames: 0, jersey: null }));
        colors.set(track.id, [0, 0, 0, 0, 0]);
      }
      entry.frames++;
      const counts = colors.get(track.id)!;
      track.gear?.[1]?.forEach((count, i) => (counts[i] += count));
    }
  }
  for (const [id, counts] of colors) {
    const peak = Math.max(...counts);
    const total = counts.reduce((sum, n) => sum + n, 0);
    if (peak > total * 0.55) summary.get(id)!.jersey = JERSEYS[counts.indexOf(peak)];
  }
  return summary;
}

function gearDistance(a: Gear, b: Gear): number {
  return [0.4, 0.4, 0.2].reduce(
    (total, weight, zone) =>
      total + (weight * a[zone].reduce((sum, count, color) => sum + Math.abs(count - b[zone][color]), 0)) / 40,
    0,
  );
}

/**
 * The track whose gear colors clearly match `gear`, with its best frame.
 * Null unless one track is close and well ahead of the runner-up.
 */
export function gearMatch(frames: Frame[], gear: Gear): { source_ms: number; id: number } | null {
  type Sample = { source_ms: number; id: number; score: number };
  const samples = new Map<number, Sample[]>();
  for (const frame of frames) {
    for (const track of frame.tracks) {
      if (track.predicted || !track.gear) continue;
      const list = samples.get(track.id) ?? [];
      samples.set(track.id, list);
      if (list.length && frame.source_ms - list.at(-1)!.source_ms < 250) continue;
      list.push({ source_ms: frame.source_ms, id: track.id, score: gearDistance(gear, track.gear) });
    }
  }
  const ranking = [...samples.values()]
    .filter((list) => list.length >= 3)
    .map((list) => {
      list.sort((a, b) => a.score - b.score);
      return { best: list[0], score: (list[0].score + list[1].score + list[2].score) / 3 };
    })
    .sort((a, b) => a.score - b.score);
  const [best, second] = ranking;
  return best && best.score <= 0.35 && (!second || second.score - best.score >= 0.08) ? best.best : null;
}

/** "Team #12", cut to the 40 characters a player name may have. */
export function jerseyLabel(details: PlayerDetails | undefined): string {
  if (!details?.jersey) return "";
  const prefix = [...details.team]
    .slice(0, 38 - details.jersey.length)
    .join("")
    .trimEnd();
  return [prefix, "#" + details.jersey].filter(Boolean).join(" ");
}
