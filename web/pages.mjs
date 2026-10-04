// The pages `scaena serve` carries (PLAN 2.11, ADR-0012): this build, copied where the serve
// crate's build looks for it, so a `scaena` built after `just web` serves them.
import { cpSync, rmSync } from "node:fs";

const to = new URL("../crates/scaena-serve/pages/dist/", import.meta.url);
rmSync(to, { recursive: true, force: true });
cpSync(new URL("dist/", import.meta.url), to, { recursive: true });
