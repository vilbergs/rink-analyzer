<script lang="ts">
  import LocalReview from "./LocalReview.svelte";

  let file = $state.raw<File | undefined>();
  let picker: HTMLInputElement;

  function choose(input: HTMLInputElement) {
    if (input.files?.[0]) file = input.files[0];
    input.value = "";
  }
</script>

<main>
  <nav><a href="/">Lessons</a> / Local video notes</nav>
  <header>
    <h1>Local video notes</h1>
    <input hidden type="file" accept="video/*,.mkv,.mov,.webm" bind:this={picker} onchange={(e) => choose(e.currentTarget)} />
    <button class="btn" class:primary={!file} onclick={() => picker.click()}>{file ? "Change video" : "Open video"}</button>
  </header>
  {#if file}
    {#key file}<LocalReview {file} />{/key}
  {:else}
    <p>Open a game or shift recording to mark moments and take notes. The video stays on your device.</p>
  {/if}
</main>

<style>
  main { max-width: 1180px; margin: 0 auto; padding: 16px clamp(16px, 3vw, 36px) 32px; }
  nav, p { color: var(--text-muted); line-height: 1.5; }
  a { color: var(--accent-soft); }
  header { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin: 18px 0; }
  h1 { margin: 0; font-size: clamp(22px, 4vw, 30px); }
</style>
