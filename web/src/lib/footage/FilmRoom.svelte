<script lang="ts">
  import "./footage.css";
  import * as api from "./api";
  import type { FeedbackRequest, Session } from "./api";
  import { clock } from "./tracks";

  let { sessionId }: { sessionId: string } = $props();

  let session = $state.raw<Session>();
  let room = $state.raw<FeedbackRequest>();
  let problem = $state("");
  let player = $state<HTMLVideoElement>();
  let now = $state(0);
  let length = $state(0);

  $effect(() => {
    const id = sessionId;
    let live = true;
    Promise.all([api.session(id), api.feedback(id)])
      .then(([loaded, builds]) => {
        if (!live) return;
        session = loaded;
        room = builds.find((b) => b.state === "ready");
        if (!room) problem = "This session has no film room yet. Build one from the session's Film room panel.";
      })
      .catch((error) => live && (problem = error.status === 404 ? "This session no longer exists." : error.message));
    return () => (live = false);
  });

  const moments = $derived(room?.feedback?.moments ?? []);
  /** The moment playing now: the last one that has started. */
  const current = $derived(moments.findLastIndex((m) => m.video_ms <= now * 1000));
  const title = $derived(
    session ? new Intl.DateTimeFormat(undefined, { month: "long", day: "numeric" }).format(session.created_ms) + " session" : "Session",
  );

  function seek(ms: number) {
    if (!player) return;
    player.currentTime = ms / 1000;
    player.play();
  }

  /** Where moment `i` starts and how much of the film it fills, as percentages for the timeline. */
  function span(i: number): string {
    if (!length) return "display: none";
    const start = moments[i].video_ms / 1000;
    const end = i + 1 < moments.length ? moments[i + 1].video_ms / 1000 : length;
    return `left: ${(start / length) * 100}%; width: ${((end - start) / length) * 100}%`;
  }
</script>

<div class="footage room">
  <header>
    <nav>
      <a href="/">Lessons</a> / <a href="/footage">Footage</a> / <a href="/footage/{sessionId}">{title}</a> /
      <b>Film room</b>
    </nav>
  </header>

  {#if room?.feedback}
    <main>
      <div class="film">
        <video
          bind:this={player}
          bind:currentTime={now}
          bind:duration={length}
          src={api.feedbackVideoUrl(sessionId, room.id)}
          controls
          playsinline
        ></video>
        <div class="timeline" aria-label="Moments">
          {#each moments as moment, i (i)}
            <button style={span(i)} class:on={i === current} title={moment.title} onclick={() => seek(moment.video_ms)}>
              {i + 1}
            </button>
          {/each}
          <i style="left: {length ? (now / length) * 100 : 0}%"></i>
        </div>
      </div>
      <aside>
        <p class="summary">{room.feedback.summary}</p>
        {#if room.note}<p class="asked">You asked: “{room.note}”</p>{/if}
        <ol>
          {#each moments as moment, i (i)}
            <li>
              <button class="moment" class:on={i === current} onclick={() => seek(moment.video_ms)}>
                <span class="n">{i + 1}</span>
                <b>{moment.title}</b>
                <small>{moment.clip_name} at {clock(moment.start_ms)}</small>
                <span class="did">{moment.observation}</span>
                <span class="try">Try: {moment.suggestion}</span>
              </button>
            </li>
          {/each}
        </ol>
      </aside>
    </main>
  {:else}
    <p class="waiting">{problem || "Loading the film room…"}</p>
  {/if}
</div>

<style>
  .room {
    display: grid;
    grid-template-rows: 42px minmax(0, 1fr);
    height: 100dvh;
    background: var(--app-bg);
    color: var(--text);
  }
  header {
    display: flex;
    align-items: center;
    padding: 0 14px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
    white-space: nowrap;
  }
  header nav {
    overflow: hidden;
    color: var(--text-muted);
    text-overflow: ellipsis;
  }
  header b {
    color: var(--text);
  }
  main {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 360px;
    min-height: 0;
  }
  .film {
    display: grid;
    grid-template-rows: minmax(0, 1fr) auto;
    gap: 10px;
    min-width: 0;
    min-height: 0;
    padding: 14px;
    background: #000;
  }
  video {
    width: 100%;
    height: 100%;
    min-height: 0;
    object-fit: contain;
  }
  .timeline {
    position: relative;
    height: 30px;
    border-radius: 6px;
    background: var(--raised);
  }
  .timeline button {
    position: absolute;
    top: 3px;
    bottom: 3px;
    min-width: 18px;
    border: 0;
    border-left: 2px solid var(--panel);
    border-radius: 4px;
    background: color-mix(in srgb, var(--accent) 35%, transparent);
    color: var(--text);
    font: inherit;
    font-size: 12px;
    cursor: pointer;
  }
  .timeline button.on {
    background: var(--accent);
    color: #fff;
  }
  .timeline i {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    background: #ffd600;
    pointer-events: none;
  }
  aside {
    display: grid;
    align-content: start;
    gap: 12px;
    min-height: 0;
    padding: 14px;
    overflow-y: auto;
    border-left: 1px solid var(--border);
    background: var(--panel);
  }
  .summary {
    margin: 0;
    line-height: 1.5;
  }
  .asked {
    margin: 0;
    color: var(--text-muted);
    font-style: italic;
  }
  ol {
    display: grid;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .moment {
    display: grid;
    grid-template-columns: 22px 1fr;
    gap: 3px 8px;
    width: 100%;
    padding: 9px 10px;
    border: 1px solid transparent;
    border-radius: 8px;
    background: var(--raised);
    color: var(--text);
    font: inherit;
    line-height: 1.45;
    text-align: left;
    cursor: pointer;
  }
  .moment.on {
    border-color: var(--accent);
  }
  .moment .n {
    grid-row: span 4;
    color: var(--text-muted);
    font-variant-numeric: tabular-nums;
  }
  .moment small {
    color: var(--text-muted);
    font-size: 11px;
  }
  .did {
    color: var(--text-soft);
  }
  .try {
    color: var(--accent-soft);
  }
  .waiting {
    place-self: center;
    max-width: 420px;
    padding: 24px;
    color: var(--text-muted);
    text-align: center;
  }
  @media (max-width: 860px) {
    main {
      grid-template-columns: minmax(0, 1fr);
      grid-template-rows: auto minmax(0, 1fr);
    }
    video {
      max-height: 45dvh;
    }
    aside {
      border-left: 0;
      border-top: 1px solid var(--border);
    }
  }
</style>
