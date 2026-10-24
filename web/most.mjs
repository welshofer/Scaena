// A source's body is read no further than its plan's `most` (the 20261024 plan's SEC-2): an answer
// that streams on and on stops one chunk past it, saying so, rather than filling memory first; one
// within it reads whole. Node reads web/src/most.ts as it stands, its types stripped.
//
//   node --experimental-strip-types web/most.mjs   (from the repository's root)
//
// Exits 1 on any failure.
import { atMost } from "./src/most.ts";

const failures = [];
const check = (ok, what) => (ok ? console.log(`ok   ${what}`) : (failures.push(what), console.log(`FAIL ${what}`)));

/** A response whose body is `chunks` chunks of 64 KB, or never ends, with no length said. */
function streamed(chunks = Infinity) {
  let sent = 0;
  let pulled = false;
  const body = new ReadableStream({
    pull(controller) {
      pulled = true;
      if (sent++ >= chunks) controller.close();
      else controller.enqueue(new Uint8Array(1 << 16).fill(sent));
    },
  });
  return { response: new Response(body), sent: () => sent, pulled: () => pulled };
}

const MB = 1 << 20;
const within = streamed(8);
const whole = await atMost(within.response, MB, "https://example.com/half");
check(whole.length === 8 << 16 && whole[0] === 1 && whole.at(-1) === 8, `a body within most reads whole: ${whole.length} bytes`);

// An answer that never ends: an arrayBuffer() would wait for it forever; a bounded read stops.
const endless = streamed();
const stopped = await Promise.race([
  atMost(endless.response, MB, "https://example.com/endless").then(
    () => "read whole",
    (e) => e.message,
  ),
  new Promise((resolve) => setTimeout(() => resolve("still reading after 5 s"), 5000)),
]);
check(stopped === "https://example.com/endless answered with more than 1 MB", `an endless answer is refused as it passes most: ${stopped}`);
check(endless.sent() <= 18, `and stops reading one chunk past it: ${endless.sent()} chunks of 64 KB`);

// A length said past most is refused before its body is read.
const said = streamed(64);
const headed = new Response(said.response.body, { headers: { "content-length": String(64 << 16) } });
const refused = await atMost(headed, MB, "https://example.com/said").catch((e) => e.message);
check(refused === "https://example.com/said answered with more than 1 MB", `a length said past most is refused: ${refused}`);
check(said.sent() <= 2, `before its body is read: ${said.sent()} chunks pulled`);

if (failures.length) {
  console.log(`\n${failures.length} failed`);
  process.exit(1);
}
