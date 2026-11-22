/** Small pictures of a skater cut from a tracking preview, for recognising a confirmed player. */

import { firstFrameMs, movieUrl, type View } from "./api";

const cache = new Map<string, Promise<string>>();
const offsets = new Map<string, Promise<number>>();
// One hidden video does every seek, one at a time: parallel decoders are slow and Safari limits them.
let queue: Promise<unknown> = Promise.resolve();
let video: HTMLVideoElement | undefined;
let loaded = "";

function once(target: HTMLVideoElement, name: string): Promise<void> {
  return new Promise((resolve, reject) => {
    const settle = (event: Event) => {
      target.removeEventListener(name, settle);
      target.removeEventListener("error", settle);
      if (event.type === "error") reject(new Error("The frame could not be loaded."));
      else resolve();
    };
    target.addEventListener(name, settle);
    target.addEventListener("error", settle);
  });
}

async function render(session: string, view: View, first: number): Promise<string> {
  video ??= Object.assign(document.createElement("video"), { muted: true, preload: "auto", playsInline: true });
  const src = movieUrl(session, view.clip);
  if (loaded !== src) {
    loaded = "";
    video.src = src;
    await once(video, "loadeddata");
    loaded = src;
  }
  const time = Math.max(0, (view.source_ms - first) / 1000 + 0.01);
  if (Math.abs(video.currentTime - time) > 0.001) {
    video.currentTime = time;
    await once(video, "seeked");
  }
  const scale = video.videoWidth / view.size[0];
  const [left, top, right, bottom] = view.xyxy;
  const pad = (right - left) * 0.2;
  const x = Math.max(0, left - pad) * scale;
  const y = Math.max(0, top - pad) * scale;
  const width = Math.min(view.size[0], right + pad) * scale - x;
  const height = Math.min(view.size[1], bottom + pad) * scale - y;
  const canvas = document.createElement("canvas");
  canvas.height = 168;
  canvas.width = Math.round((canvas.height * width) / height);
  canvas.getContext("2d")!.drawImage(video, x, y, width, height, 0, 0, canvas.width, canvas.height);
  return canvas.toDataURL("image/jpeg", 0.85);
}

/**
 * A data URL of the skater in `view`. `first` is the source time of the clip's first frame when the
 * caller already has it; other clips are asked for theirs.
 */
export function crop(session: string, view: View, first?: number): Promise<string> {
  const key = [view.clip, view.run, view.source_ms, view.track_id].join("|");
  let result = cache.get(key);
  if (!result) {
    const run = view.clip + "|" + view.run;
    const offset =
      first != null ? Promise.resolve(first) : (offsets.get(run) ?? firstFrameMs(session, view.clip, view.run));
    offsets.set(run, offset);
    result = queue.then(async () => render(session, view, await offset));
    queue = result.catch(() => {});
    cache.set(key, result);
    result.catch(() => cache.delete(key));
  }
  return result;
}
