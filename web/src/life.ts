// What the editor has open lives until it opens another (PLAN 2.12): what its parts listen for on
// the page's own elements, which stay from one bundle to the next, and on the document, goes with
// it, so the bundle opened after it hears each key and press once, and no part of the one before
// answers it.

let open = new AbortController();

/** Listen for `type` on `at`, an element the page keeps or the document, with `hear`, until the
 * editor opens another bundle. */
export function listen<K extends keyof HTMLElementEventMap>(
  at: HTMLElement,
  type: K,
  hear: (e: HTMLElementEventMap[K]) => void,
  options?: AddEventListenerOptions,
): void;
export function listen<K extends keyof DocumentEventMap>(at: Document, type: K, hear: (e: DocumentEventMap[K]) => void, options?: AddEventListenerOptions): void;
/** An event the maps do not name, such as `beforepaste`, heard as the event it is. */
export function listen<E extends Event>(at: Document, type: string, hear: (e: E) => void, options?: AddEventListenerOptions): void;
export function listen(at: EventTarget, type: string, hear: (e: never) => void, options: AddEventListenerOptions = {}) {
  at.addEventListener(type, hear as EventListener, { ...options, signal: open.signal });
}

/** `observer`, let go when the editor opens another bundle. */
export function observing<T extends MutationObserver | ResizeObserver>(observer: T): T {
  open.signal.addEventListener("abort", () => observer.disconnect(), { once: true });
  return observer;
}

/** The editor opens another bundle: what was listened for and observed till then is let go. */
export function ended() {
  open.abort();
  open = new AbortController();
}
