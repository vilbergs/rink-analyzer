<script lang="ts">
  import * as api from "./api";
  import type { Session } from "./api";

  let { onimported, onqueued }: { onimported: (session: Session) => void; onqueued: (job: string) => void } = $props();

  const MAX_CLIPS = 32;
  const MAX_BYTES = 1024 ** 3;

  let dialog = $state<HTMLDialogElement>();
  let files = $state.raw<File[]>([]);
  let over = $state(false);
  let busy = $state(false);
  /** Upload progress from 0 to 1, or null while the Pi reads the clips. */
  let progress = $state<number | null>(0);
  let message = $state("");
  let failures = $state.raw<string[]>([]);
  let link = $state("");
  let linkMessage = $state("");

  const invalid = $derived(
    files.length > MAX_CLIPS ||
      files.reduce((sum, file) => sum + file.size, 0) > MAX_BYTES ||
      files.some((file) => !api.uploadName(file)),
  );

  export function open() {
    dialog?.showModal();
  }

  function choose(list: FileList | null | undefined) {
    if (busy || !list) return;
    files = [...list];
    failures = [];
    message = "";
  }

  async function upload(event: SubmitEvent) {
    event.preventDefault();
    if (busy || invalid || !files.length) return;
    busy = true;
    progress = 0;
    failures = [];
    message = "";
    try {
      const result = await api.upload(files, (fraction) => (progress = fraction));
      failures = result.failures ?? [];
      if (result.session) {
        files = [];
        onimported(result.session);
        if (!failures.length) dialog?.close();
        else message = "Session saved, but some clips could not be read.";
      } else {
        message = "No clips were imported.";
      }
    } catch (error) {
      message = (error as Error).message;
    } finally {
      busy = false;
    }
  }

  async function youtube(event: SubmitEvent) {
    event.preventDefault();
    if (busy) return;
    linkMessage = "";
    try {
      const job = await api.importYoutube(link);
      link = "";
      onqueued(job.id);
      dialog?.close();
    } catch (error) {
      linkMessage = (error as Error).message;
    }
  }
</script>

<dialog bind:this={dialog} oncancel={(event) => busy && event.preventDefault()}>
  <header>
    <h2>Import footage</h2>
    <button class="tool quiet" aria-label="Close" disabled={busy} onclick={() => dialog?.close()}>×</button>
  </header>

  <form onsubmit={upload}>
    <label
      class="drop"
      class:over
      ondragover={(event) => {
        event.preventDefault();
        over = !busy;
      }}
      ondragleave={() => (over = false)}
      ondrop={(event) => {
        event.preventDefault();
        over = false;
        choose(event.dataTransfer?.files);
      }}
    >
      <input
        type="file"
        multiple
        accept="video/*,.mov,.mp4"
        disabled={busy}
        onchange={(event) => {
          choose(event.currentTarget.files);
          event.currentTarget.value = "";
        }}
      />
      <b>Drop clips here, or click to choose</b>
      <span>Up to 32 MOV or MP4 clips, 1 GiB together. They become one session.</span>
    </label>
    {#if files.length}
      <ul>
        {#each files as file (file.name + file.size)}<li>{file.name}</li>{/each}
      </ul>
    {/if}
    {#if invalid}<p class="warn">Choose up to 32 MOV or MP4 clips totalling at most 1 GiB.</p>{/if}
    {#if busy}
      <progress value={progress ?? undefined} max="1"></progress>
      <p class="hint">{progress == null ? "Reading clips and making previews…" : "Uploading…"}</p>
    {/if}
    {#if message}<p class="warn" role="alert">{message}</p>{/if}
    {#if failures.length}
      <ul class="warn">
        {#each failures as failure (failure)}<li>{failure}</li>{/each}
      </ul>
    {/if}
    <button class="tool primary" disabled={busy || invalid || !files.length}>Import session</button>
  </form>

  <form class="youtube" onsubmit={youtube}>
    <label class="field">
      Or import one public YouTube video (up to 15 minutes)
      <input type="url" required placeholder="https://www.youtube.com/watch?v=…" bind:value={link} />
    </label>
    <button class="tool" disabled={busy}>Import link</button>
    {#if linkMessage}<p class="warn" role="alert">{linkMessage}</p>{/if}
  </form>
</dialog>

<style>
  dialog {
    width: min(520px, calc(100vw - 32px));
    padding: 18px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--panel);
    color: var(--text);
  }
  dialog::backdrop {
    background: rgb(0 0 0 / 0.55);
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 12px;
  }
  h2 {
    margin: 0;
    font-size: 16px;
  }
  form {
    display: grid;
    gap: 10px;
    justify-items: start;
  }
  .youtube {
    margin-top: 16px;
    padding-top: 16px;
    border-top: 1px solid var(--border);
  }
  .youtube .field {
    width: 100%;
  }
  .drop {
    display: grid;
    gap: 6px;
    width: 100%;
    padding: 28px 16px;
    border: 1px dashed var(--text-dim);
    border-radius: 10px;
    color: var(--text-muted);
    text-align: center;
    cursor: pointer;
  }
  .drop.over {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .drop b {
    color: var(--text);
  }
  .drop input {
    position: absolute;
    width: 1px;
    height: 1px;
    opacity: 0;
  }
  ul {
    max-height: 120px;
    margin: 0;
    padding-left: 18px;
    overflow-y: auto;
    color: var(--text-soft);
  }
  progress {
    width: 100%;
  }
  p {
    margin: 0;
  }
  .warn {
    color: var(--warn);
  }
</style>
