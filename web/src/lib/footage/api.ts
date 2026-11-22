/** The hosted footage service on the Pi, same-origin under /api/footage. */

const API = "/api/footage";

export type Clip = { id: string; name: string; duration_ms: number; preview_ms: number };
export type Session = { id: string; created_ms: number; clips: Clip[]; source_url?: string };
export type RecentSession = {
  id: string;
  name: string;
  created_ms: number;
  clip_count: number;
  duration_ms: number;
  preview_clips: string[];
};

export type Tracking = "queued" | "processing" | "ready" | "failed" | "disabled";

/** Color counts for helmet, jersey and socks; five color bins each. */
export type Gear = number[][];
export type Box = [number, number, number, number];
export type Track = { id: number; predicted: boolean; xyxy: Box; gear?: Gear | null };
export type Frame = {
  source_ms: number;
  width: number;
  height: number;
  tracks: Track[];
  /** The preview video is clean and the overlay draws every box. Older previews have boxes burned in. */
  interactive?: boolean;
  shot?: number | null;
};

/** One track on one frame of one tracking run. */
export type FrameKey = { clip: string; run: string; source_ms: number; track_id: number };
export type View = FrameKey & { size: [number, number]; xyxy: Box; gear: Gear | null };
export type Assignment = { edit: string; player: string; view: View };
export type Compared = { player: string | null; view: View };
export type Moment = { id: string; player: string; view: View; compare: Compared | null; text: string };
export type PlayLabel = { id: string; name: string; description: string };
export type Play = {
  id: string;
  clip: string;
  run: string;
  start_ms: number;
  decision_ms: number;
  end_ms: number;
  primary: string;
  secondary: string | null;
  cue: string;
  track: number | null;
};
export type SavedPlay = Play & { player: Compared | null };
export type PlayerDetails = { team: string; jersey: string };

/** Everything the session knows about who is who, replayed from its edit ledger. */
export type Identity = {
  revision: number;
  /** Clip id to its current tracking run. Assignments from an older run no longer apply. */
  runs: Record<string, string>;
  players: Record<string, string>;
  player_details: Record<string, PlayerDetails>;
  assignments: Assignment[];
  moments: Moment[];
  plays: SavedPlay[];
  play_labels: PlayLabel[];
  undo: string | null;
};

export type IdentityAction =
  | { action: "assign"; frame: FrameKey; player: string; name: string | null; details?: PlayerDetails; shown: string[] }
  | { action: "reject"; frame: FrameKey; player: string; shown: string[] }
  | { action: "rename"; frame: FrameKey; player: string; name: string; details: PlayerDetails }
  | { action: "undo"; target: string }
  | { action: "save_moment"; frame: FrameKey; player: string; compare: FrameKey | null; text: string }
  | { action: "remove_moment"; target: string }
  | { action: "save_play"; play: Play }
  | { action: "remove_play"; target: string };

export type Candidate = { player: string; distance: number; reference: View };
export type Suggestions = { revision: number; clear: string | null; candidates: Candidate[] };

/** A "this is me" marker: from this frame on, the user is this track. */
export type Anchor = { source_ms: number; track_id: number; gear?: Gear };
export type Selections = Record<string, Anchor[]>;

export type Room = { state: "choose" | "review" | "changed" | "identity_changed" | "ready"; url: string | null };
export type YoutubeJob = { id: string; state: "queued" | "fetching" | "ready" | "failed"; error: string | null };
export type Imported = { session: Session | null; failures: string[]; error?: string };

export class ApiError extends Error {
  constructor(
    message: string,
    readonly status: number,
  ) {
    super(message);
  }
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const response = await fetch(API + path, init);
  if (!response.ok) {
    const payload = await response.json().catch(() => ({}));
    throw new ApiError(payload.error ?? "The Pi could not complete this request.", response.status);
  }
  return response.status === 204 ? (undefined as T) : response.json();
}

const body = (method: string, payload: unknown): RequestInit => ({
  method,
  headers: { "Content-Type": "application/json" },
  body: JSON.stringify(payload),
});

export const sessions = () => request<RecentSession[]>("/sessions");
export const session = (id: string) => request<Session>(`/sessions/${id}`);
export const removeSession = (id: string) => request<void>(`/sessions/${id}`, { method: "DELETE" });
export const tracking = (id: string) => request<{ clip: string; tracking: Tracking }[]>(`/sessions/${id}/tracking`);
export const coaching = (id: string) => request<Record<string, Room>>(`/sessions/${id}/coaching`);
export const selections = (id: string) => request<Selections>(`/sessions/${id}/focus`);
export const identity = (id: string) => request<Identity>(`/sessions/${id}/identity`);

export const setAnchor = (id: string, clip: string, source_ms: number, track_id: number | null) =>
  request<Anchor[]>(`/sessions/${id}/clips/${clip}/focus`, body("PUT", { source_ms, track_id }));

/** `id` makes a retry of the same edit safe; `revision` rejects an edit made against stale state. */
export const editIdentity = (id: string, edit: IdentityAction & { id: string; revision: number }) =>
  request<Identity>(`/sessions/${id}/identity`, body("POST", edit));

export const suggestions = (id: string, frame: FrameKey) =>
  request<Suggestions>(`/sessions/${id}/identity/suggestions`, body("POST", frame));

export const previewUrl = (id: string, clip: string) => `${API}/sessions/${id}/clips/${clip}/preview`;
export const movieUrl = (id: string, clip: string) => `${API}/sessions/${id}/clips/${clip}/tracks.mp4`;
const dataUrl = (id: string, clip: string) => `${API}/sessions/${id}/clips/${clip}/tracks.ndjson`;

export async function tracks(id: string, clip: string): Promise<{ frames: Frame[]; run: string | null }> {
  const response = await fetch(dataUrl(id, clip));
  if (!response.ok) throw new ApiError("Track data is unavailable for this clip.", response.status);
  const frames = (await response.text())
    .split("\n")
    .filter(Boolean)
    .map((line) => JSON.parse(line) as Frame);
  if (!frames.length) throw new ApiError("There are no tracked frames in this clip.", 422);
  return { frames, run: response.headers.get("x-rink-tracking-run") };
}

/** The source time of a run's first frame, read from the first line only. */
export async function firstFrameMs(id: string, clip: string, run: string): Promise<number> {
  const response = await fetch(dataUrl(id, clip), { cache: "no-store" });
  if (!response.ok || !response.body || response.headers.get("x-rink-tracking-run") !== run) {
    throw new ApiError("Tracking changed.", 409);
  }
  const reader = response.body.getReader();
  const decoder = new TextDecoder();
  let text = "";
  try {
    while (!text.includes("\n")) {
      const { value, done } = await reader.read();
      if (done) break;
      text += decoder.decode(value, { stream: true });
    }
    return (JSON.parse(text.split("\n", 1)[0]) as Frame).source_ms;
  } finally {
    await reader.cancel();
  }
}

export const importYoutube = (url: string) => request<YoutubeJob>("/youtube", body("POST", { url }));
export const youtubeJob = (id: string) => request<YoutubeJob>(`/youtube/${id}`);

/** The service only takes .mov and .mp4 names; phones sometimes hand over a bare name with a video type. */
export function uploadName(file: File): string {
  if (/\.(mov|mp4)$/i.test(file.name)) return file.name;
  const extension = { "video/quicktime": ".mov", "video/mp4": ".mp4", "video/x-m4v": ".mp4" }[file.type];
  return extension ? file.name + extension : "";
}

/** fetch has no upload progress, so this one request uses XMLHttpRequest. `progress` is 0 to 1, then null while the Pi decodes. */
export function upload(files: File[], progress: (fraction: number | null) => void): Promise<Imported> {
  const form = new FormData();
  for (const file of files) form.append("clips", file, uploadName(file));
  return new Promise((resolve, reject) => {
    const xhr = new XMLHttpRequest();
    xhr.open("POST", API + "/sessions");
    xhr.responseType = "json";
    xhr.upload.onprogress = (event) => {
      if (event.lengthComputable) progress(event.loaded / event.total);
    };
    xhr.upload.onload = () => progress(null);
    xhr.onload = () => {
      if (xhr.response?.error) reject(new Error(xhr.response.error));
      else if (xhr.response) resolve(xhr.response);
      else if (xhr.status === 413) reject(new Error("This batch is too large. Try a smaller batch."));
      else reject(new Error("The Pi could not accept this upload. Try again."));
    };
    xhr.onerror = () => reject(new Error("Connection to the Pi was interrupted. Keep this page open and try again."));
    xhr.send(form);
  });
}
