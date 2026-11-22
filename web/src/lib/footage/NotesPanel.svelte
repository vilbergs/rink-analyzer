<script lang="ts">
  import type { Review } from "./review.svelte";
  import { clock, trackLabel } from "./tracks";

  let { review }: { review: Review } = $props();

  let text = $state("");
  let field = $state<HTMLTextAreaElement>();

  export function focus() {
    review.seek(review.index);
    field?.focus();
  }

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    if (await review.addNote(text)) text = "";
  }

  function who(moment: (typeof review.moments)[number]): string {
    const name = (player: string | null | undefined, track: number) =>
      (player && review.identity.players[player]) || trackLabel(track);
    const other = moment.compare && name(moment.compare.player, moment.compare.view.track_id);
    const clip = review.session.clips.find((clip) => clip.id === moment.view.clip);
    return [
      name(moment.player, moment.view.track_id) + (other ? " + " + other : ""),
      clip && clip.id !== review.clip.id ? clip.name : "",
    ]
      .filter(Boolean)
      .join(" · ");
  }
</script>

<section class="panel">
  <h3>Notes</h3>
  <form onsubmit={submit}>
    <textarea
      bind:this={field}
      bind:value={text}
      maxlength="600"
      disabled={review.selected == null}
      placeholder={review.selected == null ? "Select a skater to note what they do here" : "What does this skater do at this moment?"}
      onkeydown={(event) => {
        if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) event.currentTarget.form?.requestSubmit();
      }}
    ></textarea>
    <button class="tool" disabled={review.saving || !text.trim()}>Save note</button>
  </form>
  {#if review.moments.length || review.localNotes.length}
    <ul>
      {#each review.moments as moment (moment.id)}
        <li>
          <button class="open" onclick={() => review.openMoment(moment)}>
            <b>{clock(moment.view.source_ms)}</b>
            <span>{who(moment)}</span>
            <small>{moment.text}</small>
            {#if review.momentChanged(moment)}<small class="warn">The skater's identity changed since.</small>{/if}
          </button>
          <button class="tool quiet" aria-label="Remove note" disabled={review.saving} onclick={() => review.removeMoment(moment.id)}>×</button>
        </li>
      {/each}
      {#each review.localNotes as note (note.key)}
        <li>
          <button class="open" onclick={() => review.openLocalNote(note)}>
            <b>{clock(review.frames[note.index].source_ms)}</b>
            <span>{trackLabel(note.player)} · this browser only</span>
            <small>{note.text}</small>
          </button>
          <button class="tool quiet" aria-label="Remove note" onclick={() => review.removeLocalNote(note)}>×</button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  form {
    display: grid;
    gap: 6px;
    justify-items: start;
  }
  ul {
    display: grid;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: start;
    gap: 2px;
  }
  .open {
    display: grid;
    grid-template-columns: auto 1fr;
    flex: 1;
    gap: 2px 8px;
    min-width: 0;
    padding: 6px 8px;
    border: 0;
    border-radius: 6px;
    background: var(--raised);
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
  }
  .open b {
    font-variant-numeric: tabular-nums;
  }
  .open span {
    overflow: hidden;
    color: var(--text-muted);
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .open small {
    grid-column: 1 / -1;
    line-height: 1.4;
  }
  .open .warn {
    color: var(--warn);
  }
</style>
