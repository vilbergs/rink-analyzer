<script lang="ts">
  let { module }: { module: string } = $props();
</script>

<svg width="40" height="34" viewBox="0 0 40 34" fill="none" aria-hidden="true">
  <path d="M3 4h34v18a10 10 0 0 1-10 10H13A10 10 0 0 1 3 22z" stroke="var(--text-muted)" stroke-width="1.5" />
  <path d="M3 9h34" stroke="var(--accent)" stroke-width="2" />
  <path d="M4 26h32" stroke="var(--danger)" stroke-width="1.5" />
  <path d="M17 26a3 3 0 0 0 6 0" stroke="var(--text-muted)" stroke-width="1.5" />
  {#if module === "breakouts" || module === "out-of-our-zone"}
    <path d="M28 21V13m-3.5 3.5L28 13l3.5 3.5" stroke="var(--text)" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round" />
  {/if}
</svg>
