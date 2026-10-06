// The engine's worker as the player and the editor start it: a module of its own, which
// loads the engine's module beside it (PLAN 2.1). A single-file export starts its own
// (`standalone.ts`, PLAN 2.5).
import type { Engine } from "./stage";

export const worker: Engine = {
  spawn: () => new Worker(new URL("./worker.ts", import.meta.url), { type: "module" }),
};
