<script lang="ts">
  import * as api from "./api";
  import type { FeedbackRequest, Selections, Session, Tracking } from "./api";
  import { clock } from "./tracks";

  let { session, states }: { session: Session; states: Record<string, Tracking> } = $props();

  const WAITING: Record<FeedbackRequest["state"], string> = {
    queued: "Waiting for the worker",
    processing: "Picking moments and rendering…",
    ready: "Ready",
    failed: "Could not finish",
  };

  let marks = $state.raw<Selections>({});
  let requests = $state.raw<FeedbackRequest[]>([]);
  let picked = $state<string[]>([]);
  let note = $state("");
  let sending = $state(false);
  let problem = $state("");
  let watching = $state.raw<FeedbackRequest>();
  let dialog = $state<HTMLDialogElement>();
  let player = $state<HTMLVideoElement>();

  /** A clip can go into a video once it is tracked and the user has marked themselves in it. */
  function blocker(clip: string): string {
    if (states[clip] !== "ready") return "not tracked yet";
    if (!marks[clip]?.length) return "mark yourself first";
    return "";
  }
  const names = $derived(Object.fromEntries(session.clips.map((clip) => [clip.id, clip.name])));

  export async function refresh() {
    try {
      [marks, requests] = await Promise.all([api.selections(session.id), api.feedback(session.id)]);
    } catch {
      // Keep what is shown during a connection interruption.
    }
  }

  $effect(() => {
    void session.id;
    refresh();
  });
  // Poll while a video is on its way.
  $effect(() => {
    if (!requests.some((r) => r.state === "queued" || r.state === "processing")) return;
    const timer = setInterval(refresh, 5000);
    return () => clearInterval(timer);
  });

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    sending = true;
    problem = "";
    try {
      await api.requestFeedback(session.id, picked, note);
      picked = [];
      note = "";
      await refresh();
    } catch (error) {
      problem = error instanceof Error ? error.message : "Could not start the feedback video.";
    } finally {
      sending = false;
    }
  }

  function watch(request: FeedbackRequest) {
    watching = request;
    dialog?.showModal();
  }

  function seek(ms: number) {
    if (!player) return;
    player.currentTime = ms / 1000;
    player.play();
  }
</script>

<svelte:window onfocus={refresh} />

<section class="panel">
  <h3>Feedback video</h3>
  <p class="hint">Pick clips you're marked in. Claude picks the moments worth learning from and the worker cuts them into one video with cues.</p>
  <form onsubmit={submit}>
    <ul class="pick">
      {#each session.clips as clip (clip.id)}
        {@const reason = blocker(clip.id)}
        <li>
          <label class:off={!!reason}>
            <input type="checkbox" value={clip.id} bind:group={picked} disabled={!!reason} />
            <span>{clip.name}</span>
            {#if reason}<small>{reason}</small>{/if}
          </label>
        </li>
      {/each}
    </ul>
    <textarea bind:value={note} maxlength="500" rows="2" placeholder="What do you want feedback on? (optional)"></textarea>
    <button class="tool primary" disabled={sending || !picked.length || picked.length > 6}>
      {picked.length > 6 ? "Pick at most 6 clips" : "Make feedback video"}
    </button>
    {#if problem}<p class="hint warn">{problem}</p>{/if}
  </form>

  {#if requests.length}
    <ul class="requests">
      {#each requests as request (request.id)}
        <li>
          <div>
            <b>{request.clips.map((id) => names[id] ?? "Removed clip").join(", ")}</b>
            <small>
              {new Intl.DateTimeFormat(undefined, { dateStyle: "short", timeStyle: "short" }).format(request.created_ms)} ·
              {request.state === "failed" && request.error ? request.error : WAITING[request.state]}
            </small>
            {#if request.note}<small class="note">“{request.note}”</small>{/if}
          </div>
          {#if request.state === "ready"}<button class="tool" onclick={() => watch(request)}>Watch</button>{/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>

<dialog bind:this={dialog} onclose={() => player?.pause()}>
  {#if watching?.feedback}
    <div class="watch">
      <video bind:this={player} src={api.feedbackVideoUrl(session.id, watching.id)} controls playsinline></video>
      <div class="cues">
        <p>{watching.feedback.summary}</p>
        <ol>
          {#each watching.feedback.moments as moment, i (i)}
            <li>
              <button class="open" onclick={() => seek(moment.video_ms)}>
                <b>{moment.title}</b>
                <small>{moment.clip_name} · {clock(moment.start_ms)}</small>
                <span>{moment.observation}</span>
                <span class="try">Try: {moment.suggestion}</span>
              </button>
            </li>
          {/each}
        </ol>
      </div>
    </div>
  {/if}
  <button class="tool close" onclick={() => dialog?.close()}>Close</button>
</dialog>

<style>
  form {
    display: grid;
    gap: 8px;
    justify-items: start;
  }
  ul {
    display: grid;
    gap: 2px;
    width: 100%;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .pick label {
    display: grid;
    grid-template-columns: auto 1fr;
    align-items: center;
    gap: 0 8px;
    padding: 3px 2px;
    color: var(--text-soft);
    cursor: pointer;
  }
  .pick label.off {
    color: var(--text-dim);
    cursor: default;
  }
  .pick span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pick small {
    grid-column: 2;
    color: var(--text-dim);
    font-size: 11px;
  }
  .requests {
    gap: 6px;
  }
  .requests li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border-radius: 6px;
    background: var(--raised);
  }
  .requests div {
    display: grid;
    flex: 1;
    gap: 2px;
    min-width: 0;
  }
  .requests b {
    overflow: hidden;
    font-weight: 500;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .requests small {
    color: var(--text-muted);
    font-size: 11px;
    line-height: 1.4;
  }
  .note {
    font-style: italic;
  }
  .warn {
    color: var(--danger);
  }

  dialog {
    width: min(1100px, calc(100vw - 32px));
    max-height: calc(100dvh - 32px);
    padding: 16px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--panel);
    color: var(--text);
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.6);
  }
  .watch {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 300px;
    gap: 16px;
  }
  video {
    width: 100%;
    border-radius: 8px;
    background: #000;
  }
  .cues {
    display: grid;
    align-content: start;
    gap: 10px;
    max-height: 70dvh;
    overflow-y: auto;
  }
  .cues p {
    margin: 0;
    line-height: 1.45;
  }
  ol {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .open {
    display: grid;
    gap: 3px;
    width: 100%;
    padding: 8px 10px;
    border: 0;
    border-radius: 6px;
    background: var(--raised);
    color: var(--text);
    font: inherit;
    line-height: 1.4;
    text-align: left;
    cursor: pointer;
  }
  .open small {
    color: var(--text-muted);
    font-size: 11px;
  }
  .open span {
    color: var(--text-soft);
  }
  .open .try {
    color: var(--accent-soft);
  }
  .close {
    margin-top: 12px;
  }
  @media (max-width: 860px) {
    .watch {
      grid-template-columns: minmax(0, 1fr);
    }
  }
</style>
