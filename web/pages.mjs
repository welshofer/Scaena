// The pages `scaena serve` carries (PLAN 2.11, ADR-0012): this build, copied where the serve
// crate's build looks for it, so a `scaena` built after `just web` serves them.
import { cpSync, rmSync } from "node:fs";
import { fileURLToPath } from "node:url";

const to = new URL("../crates/scaena-serve/pages/dist/", import.meta.url);
rmSync(to, { recursive: true, force: true });
// Not the single-file page (PLAN 2.54): `scaena serve` serves the one it carries for
// `export --format html`.
const single = fileURLToPath(new URL("dist/standalone.html", import.meta.url));
cpSync(new URL("dist/", import.meta.url), to, { recursive: true, filter: (from) => from !== single });
