<script lang="ts">
  import type { View } from "./api";
  import { crop } from "./crops";

  let { session, view, first }: { session: string; view: View | undefined; first?: number } = $props();
</script>

<span class="crop">
  {#if view}
    {#await crop(session, view, first) then src}
      <img {src} alt="" />
    {:catch}
      <!-- The frame is gone, for example after tracking was run again. The name still identifies the player. -->
    {/await}
  {/if}
</span>

<style>
  .crop {
    display: grid;
    place-items: center;
    aspect-ratio: 3 / 4;
    overflow: hidden;
    border-radius: 5px;
    background: var(--app-bg);
  }
  img {
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
</style>
