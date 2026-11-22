<script lang="ts">
  import FilmRoom from "./lib/footage/FilmRoom.svelte";
  import Library from "./lib/footage/Library.svelte";
  import LocalNotes from "./lib/footage/local/LocalNotes.svelte";
  import Workspace from "./lib/footage/Workspace.svelte";
  import Home from "./lib/home/Home.svelte";
  import LessonScreen from "./lib/lesson/LessonScreen.svelte";
  import TopicPath from "./lib/home/TopicPath.svelte";
  import { IndexedDbProgressStore } from "./lib/progress/store";
  import { router } from "./lib/router.svelte";
  import Viewer from "./lib/viewer/Viewer.svelte";

  const progress = new IndexedDbProgressStore();

  type Route =
    | { name: "home" }
    | { name: "topic"; id: string }
    | { name: "lesson"; id: string }
    | { name: "footage"; session?: string; clip?: string }
    | { name: "local-notes" }
    | { name: "viewer" };

  function parse(path: string): Route {
    const [page, id, clip] = path.split("/").filter(Boolean);
    if (page === "lesson" && id) return { name: "lesson", id: decodeURIComponent(id) };
    if (page === "topic" && id) return { name: "topic", id: decodeURIComponent(id) };
    if (page === "footage") return { name: "footage", session: id, clip };
    if (page === "local-notes") return { name: "local-notes" };
    if (page === "viewer") return { name: "viewer" };
    return { name: "home" };
  }

  const route = $derived(parse(router.path));
</script>

{#if route.name === "lesson"}
  {#key route.id}
    <LessonScreen id={route.id} {progress} />
  {/key}
{:else if route.name === "topic"}
  <TopicPath id={route.id} {progress} />
{:else if route.name === "footage"}
  {#if route.session && route.clip === "room"}
    <FilmRoom sessionId={route.session} />
  {:else if route.session}
    <Workspace sessionId={route.session} clipId={route.clip} />
  {:else}
    <Library />
  {/if}
{:else if route.name === "local-notes"}
  <LocalNotes />
{:else if route.name === "viewer"}
  <Viewer />
{:else}
  <Home {progress} />
{/if}
