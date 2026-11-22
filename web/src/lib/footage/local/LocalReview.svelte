<script lang="ts">
  import { onMount, tick } from "svelte";
  import { exportNotes, fileKey, readMoments, timestamp, type Moment } from "./notes";

  let { file }: { file: File } = $props();
  const prefix = $derived(fileKey(file) + ".moment.");
  let url = $state("");
  let video = $state<HTMLVideoElement>();
  let ready = $state(false);
  let videoError = $state(false);
  let storageError = $state(false);
  let moments = $state<Moment[]>([]);
  let notesList: HTMLOListElement;
  let savedPosition = 0;
  let lastRemoved = $state<Moment | null>(null);
  let playbackMessage = $state("");

  function restorePosition() {
    if (!video) return;
    try {
      const position = Number(localStorage.getItem(fileKey(file) + ".position"));
      if (position > 0 && position < video.duration) {
        savedPosition = position;
        video.currentTime = position;
      }
    } catch {
      storageError = true;
    }
    ready = true;
  }

  function rememberPosition(force = false) {
    if (!ready || !video || video.seeking) return;
    const position = video.ended ? 0 : video.currentTime;
    if (position === savedPosition || (!force && Math.abs(position - savedPosition) < 5)) return;
    try {
      localStorage.setItem(fileKey(file) + ".position", String(position));
      savedPosition = position;
    } catch {
      storageError = true;
    }
  }

  onMount(() => {
    const src = URL.createObjectURL(file);
    url = src;
    try {
      moments = readMoments(prefix);
    } catch {
      storageError = true;
    }
    return () => {
      rememberPosition(true);
      URL.revokeObjectURL(src);
    };
  });

  function save(moment: Moment) {
    try {
      localStorage.setItem(prefix + moment.id, JSON.stringify({ ms: moment.ms, note: moment.note }));
    } catch {
      storageError = true;
    }
  }

  async function mark() {
    if (!ready || !video) return;
    video.pause();
    const id = crypto.getRandomValues(new Uint32Array(4)).join("-");
    const moment = { id, ms: Math.round(video.currentTime * 1000), note: "" };
    moments = [...moments, moment].sort((a, b) => a.ms - b.ms);
    save(moment);
    await tick();
    notesList?.querySelector<HTMLInputElement>(`[data-moment="${id}"]`)?.focus();
  }

  function remove(moment: Moment) {
    try {
      localStorage.removeItem(prefix + moment.id);
    } catch {
      storageError = true;
    }
    moments = moments.filter((m) => m.id !== moment.id);
    lastRemoved = moment;
  }

  function undoRemove() {
    if (!lastRemoved) return;
    moments = [...moments, lastRemoved].sort((a, b) => a.ms - b.ms);
    save(lastRemoved);
    lastRemoved = null;
  }

  function seek(seconds: number) {
    if (!ready || !video) return;
    video.pause();
    video.currentTime = Math.max(0, Math.min(seconds, video.duration));
  }

  async function playLeadIn(moment: Moment) {
    if (!ready || !video) return;
    video.currentTime = Math.max(0, moment.ms / 1000 - 5);
    try {
      await video.play();
    } catch (error) {
      // Changing recordings or pausing can interrupt a pending play request.
      if (!(error instanceof DOMException && error.name === "AbortError")) {
        playbackMessage = "Press Play in the video controls to start the lead-in.";
      }
    }
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.defaultPrevented || e.repeat || e.ctrlKey || e.metaKey || e.altKey || e.isComposing) return;
    if (e.target instanceof HTMLElement && (e.target.isContentEditable || e.target.closest("input, textarea, select"))) return;
    if (e.key.toLowerCase() === "m" && ready) {
      e.preventDefault();
      mark();
    }
  }
</script>

<svelte:window {onkeydown} onpagehide={() => rememberPosition(true)} />
<div class="review">
  <section aria-label="Video playback">
    <h2>{file.name}</h2>
    {#if url}
      <!-- The recording is supplied locally by the reviewer. -->
      <!-- svelte-ignore a11y_media_has_caption -->
      <video bind:this={video} src={url} controls playsinline preload="metadata"
        onloadedmetadata={restorePosition} onplaying={() => playbackMessage = ""}
        ontimeupdate={() => rememberPosition()} onpause={() => rememberPosition(true)}
        onseeked={() => rememberPosition(true)} onended={() => rememberPosition(true)}
        onerror={() => { ready = false; videoError = true; }}></video>
    {/if}
    {#if videoError}
      <p role="alert">This browser could not play the recording. Choose another video or open it in a browser that supports its format.</p>
    {/if}
    {#if playbackMessage}<p role="status">{playbackMessage}</p>{/if}
    <div class="tools">
      <button class="btn" disabled={!ready} onclick={() => seek((video?.currentTime ?? 0) - 5)}>Back 5s</button>
      <button class="btn" disabled={!ready} onclick={() => seek((video?.currentTime ?? 0) + 5)}>Forward 5s</button>
      <button class="btn primary" disabled={!ready} onclick={mark}>Mark moment <kbd>M</kbd></button>
    </div>
  </section>
  <section aria-label="Marked moments" class="notes">
    <div class="heading">
      <h2>Moments ({moments.length})</h2>
      <button class="btn" disabled={!moments.length} onclick={() => exportNotes(file, moments)}>Export notes</button>
    </div>
    {#if storageError}
      <p role="alert">Browser storage is unavailable. Export your notes before leaving this review.</p>
    {:else}
      <p>Reopen this video in this browser to resume with your notes.</p>
    {/if}
    {#if lastRemoved}
      <div class="row undo">
        <span role="status">Removed {timestamp(lastRemoved.ms)}</span>
        <button class="btn" onclick={undoRemove}>Undo</button>
      </div>
    {/if}
    <ol bind:this={notesList}>
      {#each moments as moment (moment.id)}
        <li>
          <div class="row">
            <button class="time" disabled={!ready} aria-label={'Go to ' + timestamp(moment.ms)}
              onclick={() => seek(moment.ms / 1000)}>{timestamp(moment.ms)}</button>
            <button class="lead-in" disabled={!ready} aria-label={'Play from five seconds before ' + timestamp(moment.ms)}
              onclick={() => playLeadIn(moment)}>Play lead-in</button>
            <button class="remove" aria-label={'Remove moment at ' + timestamp(moment.ms)} onclick={() => remove(moment)}>Remove</button>
          </div>
          <input data-moment={moment.id} aria-label={'Note at ' + timestamp(moment.ms)} placeholder="What happened?"
            value={moment.note} oninput={(e) => { moment.note = e.currentTarget.value; save(moment); }}
            onkeydown={(e) => { if (e.key === "Enter" && !e.isComposing) video?.focus(); }} />
        </li>
      {/each}
    </ol>
  </section>
</div>

<style>
  .review { display: grid; grid-template-columns: minmax(0, 1.8fr) minmax(280px, 1fr); gap: 20px; align-items: start; }
  h2 { margin: 0; font-size: 17px; overflow-wrap: anywhere; }
  video { display: block; width: 100%; max-height: 65vh; margin-top: 12px; background: #000; border-radius: 10px; }
  .tools, .heading, .row { display: flex; flex-wrap: wrap; align-items: center; gap: 8px; }
  .tools { margin-top: 10px; }
  kbd { margin-left: 5px; font: inherit; opacity: .8; }
  .heading, .row { justify-content: space-between; }
  .notes { min-width: 0; background: var(--panel); padding: 16px; border: 1px solid var(--border); border-radius: 12px; }
  p { color: var(--text-muted); line-height: 1.5; }
  [role="alert"] { color: var(--warn); }
  ol { list-style: none; margin: 0; padding: 0; max-height: 55vh; overflow: auto; }
  li { border-top: 1px solid var(--border); padding: 6px 2px 12px; }
  .time, .lead-in, .remove { border: 0; background: none; font: inherit; cursor: pointer; min-height: 40px; }
  .time { font-variant-numeric: tabular-nums; color: var(--accent-soft); text-decoration: underline; }
  .lead-in { color: var(--text-soft); }
  .remove { margin-left: auto; }
  .undo { margin: 12px 0; }
  .remove { color: var(--text-muted); }
  input { width: 100%; padding: 9px; border: 1px solid var(--border); border-radius: 6px; background: var(--raised); color: var(--text); font: inherit; }
  @media (max-width: 760px) { .review { grid-template-columns: minmax(0, 1fr); } }
</style>
