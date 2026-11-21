<script lang="ts">
  import Footage from "./lib/footage/Footage.svelte";
  import Home from "./lib/home/Home.svelte";
  import LessonScreen from "./lib/lesson/LessonScreen.svelte";
  import TopicPath from "./lib/home/TopicPath.svelte";
  import { IndexedDbProgressStore } from "./lib/progress/store";
  import Viewer from "./lib/viewer/Viewer.svelte";

  const progress = new IndexedDbProgressStore();

  type Route =
    | { name: "home" }
    | { name: "topic"; id: string }
    | { name: "lesson"; id: string }
    | { name: "footage" }
    | { name: "local-notes" }
    | { name: "viewer" };

  function parse(hash: string): Route {
    const [, page, id] = hash.split("/");
    if (page === "lesson" && id) return { name: "lesson", id: decodeURIComponent(id) };
    if (page === "topic" && id) return { name: "topic", id: decodeURIComponent(id) };
    if (page === "footage") return { name: "footage" };
    if (page === "local-notes") return { name: "local-notes" };
    if (page === "viewer") return { name: "viewer" };
    return { name: "home" };
  }

  let route = $state(parse(location.hash));
  $effect(() => {
    if (route.name === "footage") location.replace("/footage/");
  });
</script>

<svelte:window onhashchange={() => (route = parse(location.hash))} />

{#if route.name === "lesson"}
  {#key route.id}
    <LessonScreen id={route.id} {progress} />
  {/key}
{:else if route.name === "topic"}
  <TopicPath id={route.id} {progress} />
{:else if route.name === "footage"}
  <a href="/footage/">Open Footage Sessions</a>
{:else if route.name === "local-notes"}
  <Footage />
{:else if route.name === "viewer"}
  <Viewer />
{:else}
  <Home {progress} />
{/if}
