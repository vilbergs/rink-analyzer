<script lang="ts">
  import * as api from "./api";
  import type { FeedbackRequest, Selections, Session, Tracking } from "./api";

  let {
    session,
    states,
    room = $bindable(),
  }: { session: Session; states: Record<string, Tracking>; room?: FeedbackRequest } = $props();

  let marks = $state.raw<Selections>({});
  let builds = $state.raw<FeedbackRequest[]>([]);
  let note = $state("");
  let sending = $state(false);
  let problem = $state("");

  /** A clip goes into the film once it is tracked and the user has marked themselves in it. */
  function blocker(clip: string): string {
    if (states[clip] !== "ready") return "not tracked yet";
    if (!marks[clip]?.length) return "mark yourself first";
    return "";
  }
  const included = $derived(session.clips.filter((clip) => !blocker(clip.id)));
  const left = $derived(session.clips.filter((clip) => blocker(clip.id)));
  const building = $derived(builds.find((b) => b.state === "queued" || b.state === "processing"));
  const failed = $derived(builds[0]?.state === "failed" ? builds[0] : undefined);
  $effect(() => {
    room = builds.find((b) => b.state === "ready");
  });

  export async function refresh() {
    try {
      [marks, builds] = await Promise.all([api.selections(session.id), api.feedback(session.id)]);
    } catch {
      // Keep what is shown during a connection interruption.
    }
  }

  $effect(() => {
    void session.id;
    refresh();
  });
  $effect(() => {
    if (!building) return;
    const timer = setInterval(refresh, 5000);
    return () => clearInterval(timer);
  });

  async function build(event: SubmitEvent) {
    event.preventDefault();
    sending = true;
    problem = "";
    try {
      await api.requestFeedback(
        session.id,
        included.map((clip) => clip.id),
        note,
      );
      note = "";
      await refresh();
    } catch (error) {
      problem = error instanceof Error ? error.message : "Could not start the film room.";
    } finally {
      sending = false;
    }
  }
</script>

<svelte:window onfocus={refresh} />

<section class="panel">
  <h3>Film room</h3>
  {#if building}
    <p class="hint">
      {building.state === "queued" ? "Waiting for the worker…" : "Picking moments and cutting the film…"} This takes a few minutes.
    </p>
  {:else}
    {#if room}
      <a class="tool primary open" href="/footage/{session.id}/room">Open film room</a>
    {/if}
    <p class="hint">
      {room ? "Rebuild" : "Build"} one feedback film from the {included.length} clip{included.length === 1 ? "" : "s"} you're marked in. Claude
      picks the moments worth learning from and writes what to try.
    </p>
    {#if left.length}
      <ul>
        {#each left as clip (clip.id)}
          <li><span>{clip.name}</span> <small>{blocker(clip.id)}</small></li>
        {/each}
      </ul>
    {/if}
    <form onsubmit={build}>
      <textarea bind:value={note} maxlength="500" rows="2" placeholder="What do you want feedback on? (optional)"></textarea>
      <button class="tool" disabled={sending || !included.length}>{room ? "Rebuild film room" : "Build film room"}</button>
    </form>
    {#if failed && !room}<p class="hint warn">The last build failed: {failed.error}</p>{/if}
    {#if problem}<p class="hint warn">{problem}</p>{/if}
  {/if}
</section>

<style>
  form {
    display: grid;
    gap: 8px;
    justify-items: start;
  }
  .open {
    display: inline-grid;
    place-items: center;
    justify-self: start;
    text-decoration: none;
  }
  ul {
    display: grid;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
    color: var(--text-dim);
    font-size: 12px;
  }
  li {
    display: flex;
    justify-content: space-between;
    gap: 8px;
  }
  li span {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .warn {
    color: var(--danger);
  }
</style>
