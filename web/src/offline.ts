// The static site, installed (PLAN 2.73): the pages register `sw.js`, the service worker the build
// writes beside them, which keeps every file of the site, so the player and the editor open, play,
// and edit with no network; and they name the site's web app manifest, by which a browser installs
// it. Only the static site's build does (`VITE_BUNDLE`, `just site`): a page `scaena serve` serves,
// or the repository's own, has its network.

/** Register the site's service worker, where this is the site and the page may; `ready` is told
 * once it keeps every file, when the page shows "Works offline" and
 * `document.documentElement.dataset.offline` says `ready`. */
export function offline(ready?: () => void) {
  if (!import.meta.env.VITE_BUNDLE || !("serviceWorker" in navigator) || !isSecureContext) return;
  const link = document.createElement("link");
  link.rel = "manifest";
  link.href = "manifest.webmanifest";
  document.head.append(link);
  navigator.serviceWorker
    .register("./sw.js")
    .then(() => navigator.serviceWorker.ready)
    .then(() => {
      document.documentElement.dataset.offline = "ready";
      // The page says so: "Works offline", beside its status.
      const said = document.getElementById("offline");
      if (said) said.hidden = false;
      ready?.();
    })
    .catch(() => {});
}
