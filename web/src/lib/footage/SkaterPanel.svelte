<script lang="ts">
  import Crop from "./Crop.svelte";
  import type { Review } from "./review.svelte";
  import { trackLabel } from "./tracks";

  let { review }: { review: Review } = $props();

  let name = $state("");
  let team = $state("");
  let jersey = $state("");
  /** The skater is confirmed, but from this frame on the track is really someone else. */
  let correcting = $state(false);

  const player = $derived(review.targetPlayer);
  const naming = $derived(!player || correcting);
  const teams = $derived([...new Set(Object.values(review.identity.player_details).map((d) => d.team).filter(Boolean))]);

  // Refill the fields whenever the skater under edit changes, not while the user types.
  $effect(() => {
    const details = player && !correcting ? review.identity.player_details[player] : undefined;
    name = (player && !correcting && review.identity.players[player]) || "";
    team = details?.team ?? "";
    jersey = details?.jersey ?? "";
  });
  $effect(() => {
    void review.selected;
    correcting = false;
  });

  /** Confirmed players to link to: the Pi's suggestions first, then everyone else. */
  const matches = $derived.by(() => {
    const offered = review.offers?.candidates.map((candidate) => candidate.player) ?? [];
    const rest = Object.keys(review.identity.players).filter((id) => !offered.includes(id));
    return [...offered, ...rest]
      .filter((id) => id !== player)
      .map((id) => ({
        id,
        name: review.identity.players[id],
        offered: offered.includes(id),
        clear: review.offers?.clear === id,
        view: review.reference(id),
      }));
  });

  // Ask for look-alikes once the playhead rests on an unnamed skater.
  const targetKey = $derived(review.target && !review.playing ? `${review.target.source_ms}|${review.target.track_id}` : "");
  $effect(() => {
    if (!targetKey || !naming || !Object.keys(review.identity.players).length) return;
    const timer = setTimeout(() => review.suggest(), 300);
    return () => clearTimeout(timer);
  });

  const first = (clip: string, run: string) => (clip === review.clip.id && run === review.run ? review.first : undefined);

  async function submit(event: SubmitEvent) {
    event.preventDefault();
    const fields = { name, team, jersey };
    if (await (naming ? review.confirmNew(fields) : review.rename(fields))) correcting = false;
  }
</script>

<section class="panel">
  <h3>Skater</h3>
  {#if review.selected == null}
    <p class="hint">Click a skater in the video or pick one from the list. S steps through the skaters in view.</p>
    {#if review.canFindMe}
      <button class="tool" onclick={() => review.findMe()}>Find me in this clip</button>
    {/if}
  {:else}
    <div class="who">
      <strong>{review.name(review.selected)}</strong>
      <span>
        {#if review.selectedTrack}
          {trackLabel(review.selectedTrack.id)} on this frame
        {:else}
          Not in view on this frame
        {/if}
      </span>
    </div>

    <label class="check">
      <input
        type="checkbox"
        checked={review.isMe}
        disabled={review.saving || review.playing || (!review.selectedTrack && !review.isMe)}
        onchange={(event) => review.setMe(event.currentTarget.checked)}
      />
      This is me{review.anchors.length && !review.isMe ? " from this frame on" : ""}
    </label>

    <div class="row">
      {#if review.partner != null}
        <span class="hint">Comparing with <b>{review.name(review.partner)}</b></span>
        <button class="tool quiet" onclick={() => (review.partner = null)}>Stop</button>
      {:else}
        <span class="hint">Shift-click another skater to compare.</span>
        {#if review.anchors.length && !review.isMe}
          <button class="tool quiet" onclick={() => (review.partner = "you")}>Compare with me</button>
        {/if}
      {/if}
    </div>

    {#if !review.editable}
      <p class="hint">Tracking was run again for this clip. Reload to name skaters on the new run.</p>
    {:else if !review.target || review.playing}
      <p class="hint">Pause on a frame where this skater has a solid box to name them.</p>
    {:else}
      {#if naming && matches.length}
        <div class="matches">
          <span class="hint">{correcting ? "Who is it from this frame on?" : "Same as a player you already named?"}</span>
          <div class="grid">
            {#each matches as match (match.id)}
              <div class="match" class:offered={match.offered}>
                <button disabled={review.saving} onclick={() => review.link(match.id)} title="This skater is {match.name}">
                  <Crop session={review.session.id} view={match.view} first={match.view && first(match.view.clip, match.view.run)} />
                  <span>{match.name}</span>
                  {#if match.offered}<em>{match.clear ? "Likely" : "Possible"}</em>{/if}
                </button>
                {#if match.offered}
                  <button class="no" disabled={review.saving} onclick={() => review.reject(match.id)}>Not them</button>
                {/if}
              </div>
            {/each}
          </div>
        </div>
      {/if}

      <form onsubmit={submit}>
        {#if naming && matches.length}<span class="hint">Or name a new player:</span>{/if}
        <label class="field">
          Name
          <input type="text" maxlength="40" placeholder="Name, or leave blank to use the jersey" bind:value={name} />
        </label>
        <div class="pair">
          <label class="field">
            Team
            <input type="text" maxlength="40" list="footage-teams" bind:value={team} />
          </label>
          <label class="field">
            Jersey
            <input type="text" inputmode="numeric" maxlength="2" bind:value={jersey} />
          </label>
        </div>
        <datalist id="footage-teams">
          {#each teams as option (option)}<option value={option}></option>{/each}
        </datalist>
        <div class="row">
          <button class="tool primary" disabled={review.saving}>{naming ? "Confirm new player" : "Save changes"}</button>
          {#if player}
            <button type="button" class="tool quiet" onclick={() => (correcting = !correcting)}>
              {correcting ? "Cancel" : "Not this player from here on"}
            </button>
          {/if}
        </div>
      </form>
    {/if}
  {/if}
</section>

<style>
  .who {
    display: grid;
    gap: 2px;
  }
  .who strong {
    font-size: 15px;
  }
  .who span {
    color: var(--text-muted);
    font-size: 12px;
  }
  form,
  .matches {
    display: grid;
    gap: 8px;
  }
  .pair {
    display: grid;
    grid-template-columns: 1fr 64px;
    gap: 8px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 6px;
  }
  .match {
    display: grid;
    gap: 2px;
    align-content: start;
  }
  .match button {
    display: grid;
    gap: 3px;
    padding: 3px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--raised);
    color: var(--text);
    font: inherit;
    font-size: 12px;
    text-align: center;
    cursor: pointer;
  }
  .match button:hover:not(:disabled) {
    border-color: var(--accent);
  }
  .match.offered > button:first-child {
    border-color: var(--feedback);
  }
  .match span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .match em {
    color: var(--feedback);
    font-size: 11px;
    font-style: normal;
  }
  .match .no {
    padding: 2px;
    border-color: transparent;
    background: none;
    color: var(--text-muted);
    font-size: 11px;
  }
</style>
