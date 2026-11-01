<script lang="ts">
  import Home from "./lib/home/Home.svelte";
  import LessonScreen from "./lib/lesson/LessonScreen.svelte";
  import { IndexedDbProgressStore } from "./lib/progress/store";
  import Viewer from "./lib/viewer/Viewer.svelte";

  const progress = new IndexedDbProgressStore();

  type Route = { name: "home" } | { name: "lesson"; id: string } | { name: "viewer" };

  function parse(hash: string): Route {
    const [, page, id] = hash.split("/");
    if (page === "lesson" && id) return { name: "lesson", id: decodeURIComponent(id) };
    if (page === "viewer") return { name: "viewer" };
    return { name: "home" };
  }

  let route = $state(parse(location.hash));
</script>

<svelte:window onhashchange={() => (route = parse(location.hash))} />

{#if route.name === "lesson"}
  {#key route.id}
    <LessonScreen id={route.id} {progress} />
  {/key}
{:else if route.name === "viewer"}
  <Viewer />
{:else}
  <Home {progress} />
{/if}
