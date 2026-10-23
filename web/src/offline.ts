// The static site, installed (PLAN 2.73): the pages register `sw.js`, the service worker the build
// writes beside them, which keeps every file of the site, so the player and the editor open, play,
// and edit with no network; and they name the site's web app manifest, by which a browser installs
// it. Only the static site's build does (`VITE_BUNDLE`, `just site`): a page `scaena serve` serves,
// or the repository's own, has its network.
//
// A new build (lift-up REL-2): its worker installs beside the one answering the page and waits,
// so the page keeps the build it opened with, whole. The page offers it, "A new version is ready":
// pressed, the waiting worker is told to take over (`skipWaiting`), and each page it answers
// loads again, so no page runs one build's pages with another's modules. The editor asks about
// unsaved changes as it does for any reload.

/** Register the site's service worker, where this is the site and the page may; `ready` is told
 * once it keeps every file, when the page shows "Works offline" and
 * `document.documentElement.dataset.offline` says `ready`. */
export function offline(ready?: () => void) {
  if (!import.meta.env.VITE_BUNDLE || !("serviceWorker" in navigator) || !isSecureContext) return;
  const link = document.createElement("link");
  link.rel = "manifest";
  link.href = "manifest.webmanifest";
  document.head.append(link);
  // Whether a worker answered the page as it opened: one taking over later is a new build's.
  const answered = Boolean(navigator.serviceWorker.controller);
  navigator.serviceWorker.addEventListener("controllerchange", () => {
    if (answered && navigator.serviceWorker.controller) location.reload();
  });
  navigator.serviceWorker
    .register("./sw.js")
    .then((registration) => {
      /** Offer the build waiting, where one is and this page is answered by another. */
      const offer = () => {
        const waiting = registration.waiting;
        const button = document.getElementById("update") as HTMLButtonElement | null;
        if (!waiting || !navigator.serviceWorker.controller || !button) return;
        button.hidden = false;
        button.onclick = () => {
          button.disabled = true;
          waiting.postMessage("skipWaiting");
        };
      };
      offer();
      registration.addEventListener("updatefound", () => {
        const installing = registration.installing;
        installing?.addEventListener("statechange", () => {
          if (installing.state === "installed") offer();
        });
      });
      return navigator.serviceWorker.ready;
    })
    .then(() => {
      document.documentElement.dataset.offline = "ready";
      // The page says so: "Works offline", beside its status.
      const said = document.getElementById("offline");
      if (said) said.hidden = false;
      ready?.();
    })
    .catch(() => {});
}
