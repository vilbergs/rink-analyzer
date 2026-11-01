<script lang="ts">
  let { text }: { text: string | null } = $props();
</script>

<p class="caption">{text ?? ""}</p>

<style>
  .caption {
    min-height: 2.6em;
    margin: 0;
    padding: 8px 12px;
    background: var(--panel);
    border-top: 1px solid var(--border);
    line-height: 1.4;
  }
</style>
