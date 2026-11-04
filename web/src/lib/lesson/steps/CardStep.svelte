<script lang="ts">
  import { settings } from "../../settings.svelte";
  import type { LessonSession } from "../session.svelte";
  import { firstSentence, paragraphs } from "../text";

  let { session, step, intro }: { session: LessonSession; step: { section: string }; intro: string | null } =
    $props();

  const [lead, rest] = $derived(firstSentence(session.meta.sections[step.section] ?? ""));

  /** A Shot section holds a paragraph per hand; show the user's, or both until they set one. */
  const HAND = /^(Right|Left) shot:\s*/;
  const hands = $derived(
    step.section === "Shot"
      ? paragraphs(session.meta.sections.Shot ?? "")
          .map((p) => {
            const text = p.replace(HAND, "");
            return { hand: HAND.exec(p)?.[1].toLowerCase() ?? null, text: text.charAt(0).toUpperCase() + text.slice(1) };
          })
          .filter((p) => settings.shot === null || p.hand === settings.shot)
      : null,
  );
</script>

<div class="step-body">
  {#if hands}
    <span class="eyebrow">{settings.shot ? `${settings.shot} shot` : "Shot"}</span>
    {#each hands as p (p.hand)}
      {#if !settings.shot}<span class="hand">{p.hand} shot</span>{/if}
      <p class="rest">{p.text}</p>
    {/each}
    {#if !settings.shot}
      <p class="intro">Set which way you shoot on the home screen to see only your hand's note.</p>
    {/if}
  {:else}
    <span class="eyebrow">{step.section}</span>
    <p class="lead">{lead}</p>
    {#each paragraphs(rest) as text, i (i)}
      <p class="rest">{text}</p>
    {/each}
    {#if intro}
      <p class="intro">{intro}</p>
    {/if}
  {/if}
</div>
<div class="actions">
  <button class="btn primary" onclick={() => session.next()}>Continue</button>
</div>

<style>
  .lead {
    font-size: 28px;
    font-weight: 700;
    line-height: 1.2;
  }
  .rest {
    color: var(--text-soft);
    font-size: 18px;
    line-height: 1.45;
  }
  .intro {
    color: var(--text-muted);
    font-size: 14px;
  }
  .hand {
    color: var(--accent-soft);
    font-size: 13px;
    font-weight: 600;
    text-transform: capitalize;
  }
</style>
