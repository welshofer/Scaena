// The font subsetter a single-file export's worker is built with (PLAN 2.5): none. Its page
// plays a bundle and never downloads one, which is what the subsetter is for (PLAN 2.4).
const none = (): never => {
  throw new Error("a single-file export does not subset fonts");
};
export default none;
export const subset = none;
