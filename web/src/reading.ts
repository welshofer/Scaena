// How the state shown reads, for a screen reader (PLAN 2.5, 2.8; SPEC §3.12): the page keeps it
// in a live region, out of sight. Each node that reads is an element of its own (`data-node`),
// and what reads as it did stays put, so a screen reader says what each state changed.

/** Show the reading of each state the deck comes to in `region`, as `html` gives it. The
 * readings come in the order they were asked for, and only the latest is shown. */
export function reader(region: Element, html: (state: string, format?: string) => string | Promise<string>) {
  let shown: string | undefined;
  let asked = 0;
  return async (state: string, format?: string) => {
    const key = `${state}\n${format ?? ""}`;
    if (key === shown) return;
    shown = key;
    const turn = ++asked;
    const parsed = document.createElement("template");
    try {
      parsed.innerHTML = await html(state, format);
    } catch {
      // Read it again the next time the deck comes to it.
      if (turn === asked) shown = undefined;
      return;
    }
    if (turn !== asked) return;
    const kept = new Map([...region.children].map((el) => [el.outerHTML, el]));
    const next = [...parsed.content.children].map((el) => {
      const same = kept.get(el.outerHTML);
      kept.delete(el.outerHTML);
      return same ?? el;
    });
    for (const gone of kept.values()) gone.remove();
    next.forEach((el, i) => {
      if (region.children[i] !== el) region.insertBefore(el, region.children[i] ?? null);
    });
  };
}
