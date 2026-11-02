<script lang="ts" module>
  const paths = {
    chevron: "M9 6l6 6-6 6",
    back: "M15 6l-6 6 6 6",
    check: "M5 12.5l4.5 4.5L19 7.5",
    lock: "M7 11V8a5 5 0 0 1 10 0v3M6 11h12v9H6z",
  };
</script>

<script lang="ts">
  let { name, size = 20 }: { name: keyof typeof paths; size?: number } = $props();
</script>

<svg
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="2"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  <path d={paths[name]} />
</svg>
