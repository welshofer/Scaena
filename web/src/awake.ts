// The screen kept awake while the deck is presented (lift-up REL-1): a talk that stays on one slide
// past the display's sleep timer would dim or blank the projector mid-sentence. The player asks
// for a screen wake lock as it goes fullscreen and lets it go as it leaves, and the presenter view
// holds one for as long as it is open. The browser lets a lock go when the page is hidden, so one
// held is asked for again as the page shows again. A browser without the API, or one that refuses,
// is left as it is: the deck plays all the same.

/** Whether the screen is to be kept awake. */
let wanted = false;
/** The lock held, while one is. */
let lock: WakeLockSentinel | null = null;

/** Ask for the lock, where one is wanted, none is held, and the page shows. */
async function ask(): Promise<void> {
  if (!wanted || lock || document.visibilityState !== "visible" || !navigator.wakeLock) return;
  try {
    const got = await navigator.wakeLock.request("screen");
    // Let go while it was asked for: given back at once.
    if (!wanted || lock) return void got.release().catch(() => {});
    lock = got;
    got.addEventListener("release", () => {
      if (lock === got) lock = null;
    });
  } catch {
    // Refused, as on battery saver: the deck plays with the screen as the system keeps it.
  }
}

document.addEventListener("visibilitychange", () => void ask());

/** Keep the screen awake from now on: while the deck is presented. */
export function stayAwake() {
  wanted = true;
  void ask();
}

/** Let the screen sleep again: the deck no longer presented. */
export function letSleep() {
  wanted = false;
  const held = lock;
  lock = null;
  void held?.release().catch(() => {});
}
