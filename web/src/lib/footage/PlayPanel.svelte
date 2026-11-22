<script lang="ts">
  import type { Review } from "./review.svelte";
  import { clock } from "./tracks";

  let { review }: { review: Review } = $props();

  const labels = $derived(review.identity.play_labels);
  const name = (id: string) => labels.find((label) => label.id === id)?.name ?? id;
  const description = (id: string) => labels.find((label) => label.id === id)?.description ?? "";
</script>

<section class="panel">
  <h3>{review.draft ? (review.draft.id ? "Edit play" : "New play") : "Plays"}</h3>
  {#if review.draft}
    {@const draft = review.draft}
    <p class="range">
      {clock(draft.start_ms)} – {clock(draft.end_ms)}
      <span>decision at {clock(draft.decision_ms)}</span>
    </p>
    <div class="row">
      <button class="tool quiet" title="I" onclick={() => review.mark("start")}>Start here</button>
      <button class="tool quiet" onclick={() => review.trim("decision", review.frame.source_ms)}>Decision here</button>
      <button class="tool quiet" title="O" onclick={() => review.mark("end")}>End here</button>
    </div>

    <div class="field">
      What is the play about?
      <div class="chips">
        {#each labels as label (label.id)}
          <button
            class="chip"
            aria-pressed={draft.primary === label.id}
            title={label.description}
            onclick={() => {
              draft.primary = label.id;
              if (draft.secondary === label.id) draft.secondary = "";
            }}
          >
            {label.name}
          </button>
        {/each}
      </div>
      {#if draft.primary}<span class="hint">{description(draft.primary)}</span>{/if}
    </div>

    {#if draft.primary}
      <div class="field">
        Also about (optional)
        <div class="chips">
          {#each labels.filter((label) => label.id !== draft.primary) as label (label.id)}
            <button
              class="chip"
              aria-pressed={draft.secondary === label.id}
              title={label.description}
              onclick={() => (draft.secondary = draft.secondary === label.id ? "" : label.id)}
            >
              {label.name}
            </button>
          {/each}
        </div>
      </div>
    {/if}

    <label class="field">
      Coaching cue (optional)
      <textarea maxlength="600" placeholder="What to look for, in a sentence" bind:value={draft.cue}></textarea>
    </label>
    <label class="check">
      <input type="checkbox" bind:checked={draft.attach} disabled={review.selected == null} />
      About {review.selected == null ? "the selected skater" : review.name(review.selected)}
    </label>

    <div class="row">
      <button class="tool primary" disabled={review.saving} onclick={() => review.savePlay()}>Save play</button>
      <button class="tool" onclick={() => review.replay(draft)}>Replay</button>
      <button class="tool quiet" onclick={() => (review.draft = null)}>Cancel</button>
      {#if draft.id}
        <button class="tool quiet danger" disabled={review.saving} onclick={() => review.removePlay(draft.id!)}>Delete</button>
      {/if}
    </div>
  {:else if !review.run}
    <p class="hint">Plays can be labelled once this clip has current tracking.</p>
  {:else if review.plays.length}
    <ul>
      {#each review.plays as play (play.id)}
        <li>
          <button class="open" onclick={() => review.editPlay(play)}>
            <b>{clock(play.decision_ms)}</b>
            <span>{[play.primary, play.secondary].filter((id) => id != null).map(name).join(" · ")}</span>
            {#if play.cue}<small>{play.cue}</small>{/if}
            {#if review.playChanged(play)}<small class="warn">Tracking or the skater's identity changed here.</small>{/if}
          </button>
          <button class="tool quiet" onclick={() => review.replay(play)}>Replay</button>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="hint">Drag across the Plays lane, or press P, to label a stretch of the clip.</p>
  {/if}
</section>

<style>
  .range {
    margin: 0;
    font-variant-numeric: tabular-nums;
    font-weight: 600;
  }
  .range span {
    margin-left: 6px;
    color: var(--text-muted);
    font-weight: 400;
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
    gap: 4px;
  }
  .open {
    display: grid;
    grid-template-columns: auto 1fr;
    flex: 1;
    gap: 2px 8px;
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
  .open small {
    grid-column: 1 / -1;
    color: var(--text-muted);
  }
  .open .warn {
    color: var(--warn);
  }
</style>
