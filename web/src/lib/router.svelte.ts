/** Path routing over the History API: `/lesson/<id>`, `/footage/<session>/<clip>` and so on. */

/** Same-origin paths the server answers itself: static content, and the footage service. */
const SERVED = /^\/(?:api|assets|curriculum|plays)\//;

/** Addresses from before the app used paths: `/#/lesson/x`, and `/footage/?session=<id>`. */
function current(): string {
  const { hash, pathname, search } = location;
  const session = new URLSearchParams(search).get("session");
  const path = hash.startsWith("#/")
    ? hash.slice(1)
    : session && /^\/footage\/?$/.test(pathname)
      ? `/footage/${session}`
      : null;
  if (path) history.replaceState(null, "", path);
  return location.pathname;
}

class Router {
  path = $state(current());

  constructor() {
    addEventListener("popstate", () => (this.path = current()));
    // Plain links stay plain links: a click on one in the app changes the route without a page load.
    document.addEventListener("click", (event) => {
      if (event.defaultPrevented || event.button !== 0 || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
      const link = event.target instanceof Element ? event.target.closest<HTMLAnchorElement>("a[href]") : null;
      if (!link || link.target || link.hasAttribute("download")) return;
      const url = new URL(link.href);
      if (url.origin !== location.origin || SERVED.test(url.pathname)) return;
      event.preventDefault();
      this.go(url.pathname);
    });
  }

  /** `replace` swaps the current history entry, for a redirect the Back button should skip. */
  go(path: string, replace = false) {
    if (path === location.pathname) return;
    if (replace) history.replaceState(null, "", path);
    else history.pushState(null, "", path);
    this.path = path;
    scrollTo(0, 0);
  }
}

export const router = new Router();
