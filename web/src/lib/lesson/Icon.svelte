<script lang="ts" module>
  const paths = {
    close: "M6 6l12 12M18 6L6 18",
    expand: "M4 9V4h5M20 9V4h-5M4 15v5h5M20 15v5h-5",
    collapse: "M9 4v5H4M15 4v5h5M9 20v-5H4M15 20v-5h5",
    replay: "M4 12a8 8 0 1 0 2.4-5.7M4 4v4.5h4.5",
    check: "M5 12.5l4.5 4.5L19 7.5",
    play: "M8 5.5v13l10-6.5z",
    pause: "M8 5v14M16 5v14",
  };
  export type IconName = keyof typeof paths;
</script>

<script lang="ts">
  let { name, size = 20 }: { name: IconName; size?: number } = $props();
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
