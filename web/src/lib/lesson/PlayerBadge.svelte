<script lang="ts">
  import type { LessonSession } from "./session.svelte";

  let { session, id }: { session: LessonSession; id: string } = $props();

  const tag = $derived(session.player(id));
</script>

<span class="badge" class:me={tag.me} style:--team={tag.color} role="img" aria-label={session.name(id)}>{tag.label}</span>

<style>
  .badge {
    display: inline-flex;
    align-items: center;
    height: 16px;
    padding: 0 4px;
    border-radius: 4px;
    background: var(--team);
    color: #fff;
    font-size: 11px;
    font-weight: 700;
    line-height: 1;
    vertical-align: 1px;
  }
  .me {
    box-shadow: 0 0 0 1.5px var(--warn);
  }
</style>
