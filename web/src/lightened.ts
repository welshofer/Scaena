/** An array that keeps its last `most` items as pushed, and each before them as `light` makes it
 * (lift-up 20261110 PERF-4): as many items as were pushed, so its length still counts them, the
 * old ones holding little. What the editor keeps of each lint of every state, for an afternoon's
 * editing. */
export function lightened<T>(most: number, light: (item: T) => T): T[] {
  const kept: T[] = [];
  kept.push = (...items: T[]) => {
    const n = Array.prototype.push.apply(kept, items);
    for (let i = Math.max(0, n - most - items.length); i < n - most; i++) kept[i] = light(kept[i]);
    return n;
  };
  return kept;
}
