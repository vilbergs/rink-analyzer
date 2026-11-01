<script lang="ts">
  import type { LessonSession } from "../session.svelte";
  import type { Step } from "../types";

  let { session, step }: { session: LessonSession; step: Extract<Step, { type: "watch" }> } = $props();

  let seen = $state(false);

  $effect(() => {
    if (session.t >= step.to_ms) seen = true;
  });

  const seconds = (ms: number) => (ms / 1000).toFixed(1);
</script>

<div class="step-body">
  <span class="eyebrow">Watch · {seconds(step.from_ms)} to {seconds(step.to_ms)} s</span>
  <p>{session.playing ? "Watch the play. It stops on the beat." : "Read the caption under the rink."}</p>
</div>
<div class="actions">
  <button class="btn" onclick={() => session.enter()}>Replay</button>
  <button class="btn primary" disabled={!seen} onclick={() => session.next()}>Continue</button>
</div>
