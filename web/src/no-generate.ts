// What a single-file export's worker makes decks from a prompt with (PLAN 2.5): nothing. Its page
// plays a bundle and never makes one, which is what the generator is for (PLAN G.5).
const none = (): never => {
  throw new Error("a single-file export makes no deck");
};
export const choices = none;
export const generate = none;
export const health = none;
export const picture = none;
