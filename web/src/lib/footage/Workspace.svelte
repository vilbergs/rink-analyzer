<script lang="ts">
  import "./footage.css";
  import { router } from "../router.svelte";
  import * as api from "./api";
  import type { Moment, Room, Session, Tracking } from "./api";
  import NotesPanel from "./NotesPanel.svelte";
  import PlayerList from "./PlayerList.svelte";
  import PlayPanel from "./PlayPanel.svelte";
  import { Review } from "./review.svelte";
  import SkaterPanel from "./SkaterPanel.svelte";
  import Stage from "./Stage.svelte";
  import Timeline from "./Timeline.svelte";
  import { duration } from "./tracks";

  let { sessionId, clipId }: { sessionId: string; clipId?: string } = $props();

  const TRACKING: Record<Tracking, string> = {
    queued: "Queued for tracking",
    processing: "Tracking skaters…",
    ready: "Ready",
    failed: "Tracking could not finish",
    disabled: "Tracking unavailable",
  };
  const COACHING: Partial<Record<Room["state"], string>> = {
    review: "Your skater is saved. This clip still needs rink alignment and a coaching review.",
    changed: "Your skater selection changed, so the coaching review needs rebuilding.",
    identity_changed: "The track marked as you covers more than one named player. Correct it at the switch.",
  };
  const SHORTCUTS = [
    ["Space", "Play or pause"],
    ["← →", "Step one frame; with Shift, one second"],
    ["[ ]", "Previous or next camera cut"],
    ["Click", "Select a skater; Shift-click to compare"],
    ["S", "Next skater in view; Shift-S for the previous"],
    ["P", "Label a play around this frame"],
    ["I O", "Set the play's start or end here"],
    ["1–7", "Choose the play's label"],
    ["M", "Write a note"],
    ["⌘Z", "Undo the last identity edit"],
    ["Pinch, 0", "Zoom the video; fit it again"],
    ["Esc", "Close the play, then clear the selection"],
  ];

  let session = $state.raw<Session>();
  let states = $state.raw<Record<string, Tracking>>({});
  let rooms = $state.raw<Record<string, Room>>({});
  let review = $state.raw<Review>();
  let problem = $state("");
  let tab = $state<"browse" | "inspect">("browse");
  let stage = $state<Stage>();
  let notes = $state<NotesPanel>();
  let help = $state<HTMLDialogElement>();
  /** A note in another clip to jump to once that clip is open. */
  let pending: Moment | undefined;

  const clip = $derived(session?.clips.find((c) => c.id === clipId));
  const title = $derived.by(() => {
    if (!session) return "";
    if (session.clips.length === 1) return session.clips[0].name;
    return new Intl.DateTimeFormat(undefined, { month: "long", day: "numeric" }).format(session.created_ms) + " session";
  });
  const room = $derived(clip ? rooms[clip.id] : undefined);

  $effect(() => {
    const id = sessionId;
    session = undefined;
    states = {};
    problem = "";
    let timer = 0;
    let live = true;
    async function poll() {
      try {
        const list = await api.tracking(id);
        if (!live) return;
        states = Object.fromEntries(list.map((item) => [item.clip, item.tracking]));
        if (list.some((item) => item.tracking === "queued" || item.tracking === "processing")) {
          timer = setTimeout(poll, 3000);
        }
      } catch {
        if (live) timer = setTimeout(poll, 3000);
      }
    }
    api
      .session(id)
      .then((loaded) => {
        if (!live) return;
        session = loaded;
        poll();
        refreshCoaching();
      })
      .catch((error) => live && (problem = error.status === 404 ? "This session no longer exists." : error.message));
    return () => {
      live = false;
      clearTimeout(timer);
    };
  });

  // Without a clip in the address, open the first one that has tracking.
  $effect(() => {
    if (!session || clip || !Object.keys(states).length) return;
    const first = session.clips.find((c) => states[c.id] === "ready") ?? session.clips[0];
    if (first) router.go(`/footage/${session.id}/${first.id}`, true);
  });

  const ready = $derived(!!clip && states[clip.id] === "ready");
  $effect(() => {
    review = undefined;
    if (!session || !clip || !ready) return;
    let live = true;
    Review.open(session, clip)
      .then((opened) => {
        if (!live) return;
        opened.onsaved = refreshCoaching;
        opened.onopen = (other, moment) => {
          pending = moment;
          router.go(`/footage/${sessionId}/${other}`);
        };
        review = opened;
        if (pending?.view.clip === opened.clip.id) opened.openMoment(pending);
        pending = undefined;
      })
      .catch((error) => live && (problem = error.message));
    return () => (live = false);
  });

  async function refreshCoaching() {
    try {
      rooms = await api.coaching(sessionId);
    } catch {
      // Keep the current state during a connection interruption.
    }
  }

  async function remove() {
    if (!session || !confirm("Delete this session and its uploaded clips from the Pi?")) return;
    try {
      await api.removeSession(session.id);
      router.go("/footage");
    } catch {
      problem = "Could not delete the session.";
    }
  }

  function onkeydown(event: KeyboardEvent) {
    const target = event.target instanceof HTMLElement ? event.target : null;
    // A focused checkbox is not a text field: shortcuts keep working after ticking one.
    const typing = !!target && (target.isContentEditable || !!target.closest("textarea, select, input:not([type=checkbox])"));
    if (event.key === "Escape" && typing) {
      target!.blur();
      return;
    }
    if (!review || typing || event.isComposing || help?.open) return;
    if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "z" && !event.shiftKey) {
      event.preventDefault();
      review.undo();
      return;
    }
    if (event.metaKey || event.ctrlKey || event.altKey) return;
    const second = review.frame.source_ms;
    const actions: Record<string, () => void> = {
      " ": () => review!.toggle(),
      k: () => review!.toggle(),
      arrowleft: () => (event.shiftKey ? review!.seekMs(second - 1000) : review!.seek(review!.index - 1)),
      arrowright: () => (event.shiftKey ? review!.seekMs(second + 1000) : review!.seek(review!.index + 1)),
      home: () => review!.seek(0),
      end: () => review!.seek(review!.frames.length - 1),
      "[": () => review!.jumpCut(-1),
      "]": () => review!.jumpCut(1),
      s: () => review!.selectNext(event.shiftKey ? -1 : 1),
      p: () => review!.run && review!.startPlay(),
      i: () => review!.run && review!.mark("start"),
      o: () => review!.run && review!.mark("end"),
      m: () => {
        tab = "inspect";
        notes?.focus();
      },
      "0": () => stage?.fit(),
      "?": () => help?.showModal(),
      escape: () => (review!.draft ? (review!.draft = null) : review!.select(null)),
    };
    const label = review.draft && review.identity.play_labels[Number(event.key) - 1];
    const action = label ? () => (review!.draft!.primary = label.id) : actions[event.key.toLowerCase()];
    if (!action) return;
    // Space always plays, even when a button has focus.
    event.preventDefault();
    if (!event.repeat || event.key.startsWith("Arrow")) action();
  }

  function onkeyup(event: KeyboardEvent) {
    if (event.key === " " && event.target instanceof HTMLElement && event.target.closest("button")) event.preventDefault();
  }
</script>

<svelte:window {onkeydown} {onkeyup} onfocus={() => (review?.refresh(), refreshCoaching())} />

<div class="footage workspace" data-tab={tab}>
  <header>
    <nav>
      <a href="/">Lessons</a> / <a href="/footage">Footage</a> /
      <b>{title || "Session"}</b>
    </nav>
    <span class="status" role="status">{problem || review?.status}</span>
    {#if room?.url}<a href={room.url}>Coaching Film Room</a>{/if}
    {#if session?.source_url}<a href={session.source_url} target="_blank" rel="noreferrer">YouTube</a>{/if}
    <button class="tool quiet" onclick={() => help?.showModal()}>Shortcuts</button>
    <button class="tool quiet danger" disabled={!session} onclick={remove}>Delete session</button>
  </header>

  <aside class="browse">
    <section class="clips">
      <h3>Clips</h3>
      {#each session?.clips ?? [] as item (item.id)}
        <a class="clip" href="/footage/{sessionId}/{item.id}" aria-current={item.id === clipId ? "true" : undefined}>
          <img src={api.previewUrl(sessionId, item.id)} alt="" loading="lazy" />
          <span>
            <b>{item.name}</b>
            <small>{duration(item.duration_ms)} · {states[item.id] ? TRACKING[states[item.id]] : "Checking…"}</small>
          </span>
        </a>
      {/each}
      {#if room && COACHING[room.state]}<p>{COACHING[room.state]}</p>{/if}
    </section>
    {#if review}<PlayerList {review} />{/if}
  </aside>

  <main>
    {#if review && clip}
      {#key review}
        <Stage bind:this={stage} {review} src={api.movieUrl(sessionId, clip.id)} />
      {/key}
    {:else if clip}
      <div class="waiting">
        <img src={api.previewUrl(sessionId, clip.id)} alt="" />
        <p>
          {#if problem}
            {problem}
          {:else if !states[clip.id] || ready}
            Loading skater tracks…
          {:else}
            {TRACKING[states[clip.id]]}. The clip opens here when its skaters are tracked.
          {/if}
        </p>
      </div>
    {:else}
      <div class="waiting"><p>{problem || "Loading session…"}</p></div>
    {/if}
  </main>

  <aside class="inspect">
    {#if review}
      <PlayPanel {review} />
      <SkaterPanel {review} />
      <NotesPanel bind:this={notes} {review} />
    {/if}
  </aside>

  <footer>
    {#if review}<Timeline {review} />{/if}
  </footer>

  <nav class="tabs">
    <button aria-pressed={tab === "browse"} onclick={() => (tab = "browse")}>Clips and players</button>
    <button aria-pressed={tab === "inspect"} onclick={() => (tab = "inspect")}>Inspect</button>
  </nav>

  <dialog bind:this={help}>
    <h2>Shortcuts</h2>
    <dl>
      {#each SHORTCUTS as [keys, action] (keys)}
        <dt>{keys}</dt>
        <dd>{action}</dd>
      {/each}
    </dl>
    <p>Drag across the Plays lane to label a stretch. Double-click a green or gold stretch to watch just that appearance.</p>
    <button class="tool" onclick={() => help?.close()}>Close</button>
  </dialog>
</div>

<style>
  /* The page never scrolls. The two side panels scroll inside themselves. */
  .workspace {
    display: grid;
    grid-template: "top top top" 42px "browse stage inspect" minmax(0, 1fr) "time time time" auto / 236px minmax(0, 1fr) 316px;
    height: 100dvh;
    overflow: hidden;
  }
  header {
    grid-area: top;
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
    padding: 0 8px 0 14px;
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
  .status {
    flex: 1;
    overflow: hidden;
    color: var(--text-muted);
    text-align: right;
    text-overflow: ellipsis;
  }
  aside {
    min-height: 0;
    overflow-y: auto;
    background: var(--panel);
    scrollbar-width: thin;
  }
  .browse {
    grid-area: browse;
    border-right: 1px solid var(--border);
  }
  .inspect {
    grid-area: inspect;
    border-left: 1px solid var(--border);
  }
  main {
    grid-area: stage;
    display: grid;
    min-width: 0;
    min-height: 0;
    background: #000;
  }
  footer {
    grid-area: time;
    min-width: 0;
  }
  .tabs {
    display: none;
  }

  .clips {
    display: grid;
    gap: 2px;
    padding: 4px 6px 8px;
    border-bottom: 1px solid var(--border);
  }
  .clips h3 {
    margin: 0;
    padding: 12px 6px 6px;
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .clips p {
    margin: 6px;
    color: var(--text-muted);
    line-height: 1.45;
  }
  .clip {
    display: grid;
    grid-template-columns: 64px 1fr;
    align-items: center;
    gap: 8px;
    padding: 4px;
    border-radius: 6px;
    color: var(--text-soft);
    text-decoration: none;
  }
  .clip:hover {
    background: var(--raised);
  }
  .clip[aria-current] {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
    color: var(--text);
  }
  .clip img {
    width: 64px;
    aspect-ratio: 16 / 9;
    border-radius: 4px;
    background: #000;
    object-fit: cover;
  }
  .clip span {
    display: grid;
    gap: 2px;
    min-width: 0;
  }
  .clip b,
  .clip small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .clip b {
    font-weight: 500;
  }
  .clip small {
    color: var(--text-dim);
    font-size: 11px;
  }

  .waiting {
    display: grid;
    place-content: center;
    justify-items: center;
    gap: 14px;
    padding: 24px;
    color: var(--text-muted);
    text-align: center;
  }
  .waiting img {
    max-width: min(480px, 100%);
    max-height: 40dvh;
    border-radius: 8px;
    opacity: 0.6;
  }
  .waiting p {
    margin: 0;
  }

  dialog {
    width: min(440px, calc(100vw - 32px));
    padding: 18px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--panel);
    color: var(--text);
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.55);
  }
  dialog h2 {
    margin: 0 0 12px;
    font-size: 16px;
  }
  dl {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 6px 14px;
    margin: 0;
  }
  dt {
    color: var(--text);
    font-weight: 600;
    white-space: nowrap;
  }
  dd {
    margin: 0;
    color: var(--text-muted);
  }
  dialog p {
    color: var(--text-muted);
    line-height: 1.45;
  }

  /* On a phone the video and timeline stay put and one panel at a time scrolls beneath them. */
  @media (max-width: 860px) {
    .workspace {
      grid-template: "top" 42px "stage" minmax(150px, 38dvh) "time" auto "tabs" auto "panel" minmax(0, 1fr) / minmax(0, 1fr);
    }
    .browse,
    .inspect {
      grid-area: panel;
      border: 0;
    }
    .workspace[data-tab="browse"] .inspect,
    .workspace[data-tab="inspect"] .browse {
      display: none;
    }
    .tabs {
      grid-area: tabs;
      display: grid;
      grid-template-columns: 1fr 1fr;
      border-block: 1px solid var(--border);
      background: var(--panel);
    }
    .tabs button {
      padding: 9px;
      border: 0;
      border-bottom: 2px solid transparent;
      background: none;
      color: var(--text-muted);
      font: inherit;
    }
    .tabs button[aria-pressed="true"] {
      border-bottom-color: var(--accent);
      color: var(--text);
    }
    header .tool,
    .status {
      display: none;
    }
  }
</style>
