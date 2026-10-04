// The assistant a single-file export's worker is built with (PLAN 2.5): none. Its page plays a
// bundle and never edits one, which is what the assistant is for (PLAN 2.6).
const none = (): never => {
  throw new Error("a single-file export has no assistant");
};
export const ask = none;
export const forget = none;
export const providers = new Proxy({}, { get: none }) as Record<string, { models: () => Promise<string[]> }>;
