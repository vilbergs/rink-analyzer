<script lang="ts">
  import type { PathLesson } from "./path";

  let { lessons }: { lessons: PathLesson[] } = $props();

  const studied = $derived(lessons.filter((l) => l.state === "studied").length);
</script>

<div class="segments" role="img" aria-label="{studied} of {lessons.length} lessons studied">
  {#each lessons as lesson (lesson.id)}
    <span class:studied={lesson.state === "studied"}></span>
  {/each}
</div>

<style>
  .segments {
    display: flex;
    flex: 1;
    gap: 4px;
  }
  span {
    flex: 1;
    height: 4px;
    border-radius: 2px;
    background: var(--raised);
  }
  .studied {
    background: var(--feedback);
  }
</style>
