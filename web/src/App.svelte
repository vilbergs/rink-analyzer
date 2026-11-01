<script lang="ts">
  import Home from "./lib/home/Home.svelte";
  import Viewer from "./lib/viewer/Viewer.svelte";

  type Route = { name: "home" } | { name: "viewer" };

  function parse(hash: string): Route {
    const [, page] = hash.split("/");
    if (page === "viewer") return { name: "viewer" };
    return { name: "home" };
  }

  let route = $state(parse(location.hash));
</script>

<svelte:window onhashchange={() => (route = parse(location.hash))} />

{#if route.name === "viewer"}
  <Viewer />
{:else}
  <Home />
{/if}
