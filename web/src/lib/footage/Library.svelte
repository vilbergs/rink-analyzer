<script lang="ts">
  import "./footage.css";
  import * as api from "./api";
  import type { RecentSession } from "./api";
  import ImportDialog from "./ImportDialog.svelte";
  import { duration } from "./tracks";

  const YOUTUBE_KEY = "rink.youtube-import";

  let sessions = $state.raw<RecentSession[]>();
  let failed = $state(false);
  let query = $state("");
  let importer = $state<ImportDialog>();
  /** A YouTube import in progress, and what to tell the user about it. */
  let youtube = $state<{ id: string; text: string; done?: boolean } | null>(null);

  const matches = $derived(
    (sessions ?? []).filter((session) => session.name.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())),
  );
  const date = (ms: number) => new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(ms);

  async function load() {
    try {
      sessions = await api.sessions();
      failed = false;
    } catch {
      failed = true;
    }
  }

  function remember(id: string | null) {
    try {
      if (id) localStorage.setItem(YOUTUBE_KEY, id);
      else localStorage.removeItem(YOUTUBE_KEY);
    } catch {
      // Without storage the import still runs; it just is not picked up again after a reload.
    }
  }

  /** Follow a YouTube import until the worker has fetched it or given up. */
  function watch(id: string) {
    remember(id);
    youtube = { id, text: "YouTube video queued…" };
    const check = async () => {
      if (youtube?.id !== id || youtube.done) return;
      try {
        const job = await api.youtubeJob(id);
        if (youtube?.id !== id) return;
        if (job.state === "ready" || job.state === "failed") {
          remember(null);
          youtube = { id, done: true, text: job.state === "ready" ? "YouTube video saved." : (job.error ?? "The import failed.") };
          if (job.state === "ready") load();
          return;
        }
        youtube = { id, text: job.state === "fetching" ? "Fetching the YouTube video…" : "YouTube video queued…" };
      } catch (error) {
        if ((error as api.ApiError).status === 404) {
          remember(null);
          youtube = null;
          return;
        }
      }
      setTimeout(check, 4000);
    };
    check();
  }

  $effect(() => {
    load();
    try {
      const pending = localStorage.getItem(YOUTUBE_KEY);
      if (pending) watch(pending);
    } catch {
      // Storage is blocked; nothing to resume.
    }
    return () => (youtube = null);
  });
</script>

<svelte:window onfocus={load} />

<div class="footage library">
  <header>
    <nav><a href="#/">Lessons</a> / <b>Footage</b></nav>
    <input type="search" placeholder="Filter sessions" aria-label="Filter sessions by name" bind:value={query} />
    <button class="tool primary" onclick={() => importer?.open()}>Import footage</button>
  </header>

  {#if youtube}
    <p class="banner" role="status">
      {youtube.text}
      {#if youtube.done && sessions?.some((session) => session.id === youtube?.id)}
        <a href="#/footage/{youtube.id}">Open session</a>
      {/if}
    </p>
  {/if}

  <main>
    {#if failed && !sessions}
      <p class="empty">The session library could not be loaded. <button class="tool" onclick={load}>Retry</button></p>
    {:else if sessions && !sessions.length}
      <p class="empty">Import game footage to start your first session.</p>
    {:else if sessions && !matches.length}
      <p class="empty">No sessions match this name.</p>
    {:else}
      <ul>
        {#each matches as session (session.id)}
          <li>
            <a href="#/footage/{session.id}">
              <span class="sheet" data-count={session.preview_clips.length}>
                {#each session.preview_clips as clip (clip)}
                  <img src={api.previewUrl(session.id, clip)} alt="" loading="lazy" />
                {/each}
                <small>{session.clip_count} {session.clip_count === 1 ? "clip" : "clips"} · {duration(session.duration_ms)}</small>
              </span>
              <b>{session.name}</b>
              <span class="when">{date(session.created_ms)}</span>
            </a>
          </li>
        {/each}
      </ul>
    {/if}
  </main>

  <ImportDialog
    bind:this={importer}
    onimported={(session) => (location.hash = `#/footage/${session.id}`)}
    onqueued={watch}
  />
</div>

<style>
  .library {
    display: grid;
    grid-template-rows: 42px auto minmax(0, 1fr);
    height: 100dvh;
  }
  header {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 0 12px 0 14px;
    border-bottom: 1px solid var(--border);
    background: var(--panel);
  }
  nav {
    flex: 1;
    color: var(--text-muted);
  }
  nav b {
    color: var(--text);
  }
  header input {
    max-width: 220px;
  }
  .banner {
    margin: 0;
    padding: 8px 14px;
    border-bottom: 1px solid var(--border);
    color: var(--text-soft);
  }
  main {
    grid-row: 3;
    overflow-y: auto;
    padding: 18px;
  }
  .empty {
    margin: 15dvh auto;
    color: var(--text-muted);
    text-align: center;
  }
  ul {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(230px, 1fr));
    gap: 18px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li a {
    display: grid;
    gap: 6px;
    color: var(--text);
    text-decoration: none;
  }
  .sheet {
    position: relative;
    display: grid;
    grid-template-columns: 2fr 1fr;
    grid-template-rows: 1fr 1fr;
    gap: 2px;
    aspect-ratio: 16 / 9;
    overflow: hidden;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: #000;
  }
  li a:hover .sheet {
    border-color: var(--accent);
  }
  .sheet img {
    width: 100%;
    height: 100%;
    min-height: 0;
    object-fit: cover;
  }
  .sheet img:first-child {
    grid-row: 1 / -1;
  }
  .sheet[data-count="1"] img {
    grid-column: 1 / -1;
  }
  .sheet[data-count="2"] img:last-of-type {
    grid-row: 1 / -1;
  }
  .sheet small {
    position: absolute;
    right: 6px;
    bottom: 6px;
    padding: 2px 6px;
    border-radius: 4px;
    background: rgb(0 0 0 / 0.7);
    color: #fff;
    font-size: 11px;
  }
  li b {
    overflow: hidden;
    font-weight: 600;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .when {
    color: var(--text-muted);
    font-size: 12px;
  }
</style>
