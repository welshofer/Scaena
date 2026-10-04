// The history a single-file export's worker is built with (PLAN 2.5): none. Its page plays a
// bundle and never saves one, which is what the history is recorded by (PLAN 2.9).
const none = (): never => {
  throw new Error("a single-file export keeps no history");
};
export default none;
export const record = none;
export const changes = none;
