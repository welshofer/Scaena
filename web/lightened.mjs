// The editor keeps each lint of every state whole for the last few alone (the 20261110 plan's
// PERF-4): those before them are kept without their findings, so an afternoon's editing does not
// keep every list it was told, and how many there have been is still the count the walks read.
// Node reads web/src/lightened.ts as it stands, its types stripped.
//
//   node --experimental-strip-types web/lightened.mjs   (from the repository's root)
//
// Exits 1 on any failure.
import { lightened } from "./src/lightened.ts";

const failures = [];
const check = (ok, what) => (ok ? console.log(`ok   ${what}`) : (failures.push(what), console.log(`FAIL ${what}`)));

const kept = lightened(5, (lint) => ({ ...lint, findings: [] }));
const lint = (n) => ({ n, findings: Array.from({ length: 1000 }, (_, i) => ({ code: "W210", i })) });
for (let n = 0; n < 500; n++) kept.push(lint(n));
check(kept.length === 500, `as many are kept as were pushed, a count: ${kept.length}`);
check(kept.slice(-5).every((l, i) => l.n === 495 + i && l.findings.length === 1000), "the last five are kept whole");
check(kept.slice(0, -5).every((l, i) => l.n === i && l.findings.length === 0), "each before them without its findings");
check(kept.at(-1).n === 499, `the latest is last: ${kept.at(-1).n}`);
const found = kept.reduce((sum, l) => sum + l.findings.length, 0);
check(found === 5000, `what is held does not grow with the edits: ${found} findings for 500 lints`);
// Several at once, as a push of more than one.
const two = lightened(2, (x) => -x);
two.push(1, 2, 3);
two.push(4);
check(JSON.stringify(two) === JSON.stringify([-1, -2, 3, 4]), `several pushed at once are lightened in turn: ${JSON.stringify(two)}`);

console.log(failures.length ? `${failures.length} failure(s)` : "the editor keeps the last lints whole, and the rest light");
process.exit(failures.length ? 1 : 0);
