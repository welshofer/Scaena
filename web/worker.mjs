// The worker alone, in headless Chromium, on the torture deck: a request that carries a source
// older than the latest edit's, posted before that edit was answered and taken while its paint
// yields, must not compile its source over the deck the edit made (lift-up: the race #469's canvas
// red found, where two arrow presses in a row and the Theme panel's bundleFiles between them left
// the worker's deck a track behind the source). The worker is spoken to by its own protocol, from
// the editor's page, which spawned it.
import { launch, serve } from "./serve.mjs";

const server = await serve();
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const browser = await launch();
try {
  const page = await browser.newPage();
  page.on("pageerror", (e) => failures.push(`pageerror: ${e.message}`));
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/tests/fixtures/torture.scaena/deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });
  // The worker's script, as the page spawned it.
  const script = await page.evaluate(() => performance.getEntriesByType("resource").map((e) => e.name).find((n) => /\/worker-[^/]*\.js$/.test(n)));
  check(Boolean(script), `the page spawned the worker from its script: ${script}`);

  const result = await page.evaluate(
    async ({ script, deck }) => {
      const worker = new Worker(script, { type: "module" });
      let n = 0;
      const waiting = new Map();
      let ready;
      const opened = new Promise((resolve) => (ready = resolve));
      worker.onmessage = ({ data }) => {
        if (data.type === "ready" || data.type === "error") ready(data);
        if (data.id !== undefined) waiting.get(data.id)?.(data);
      };
      const ask = (message) =>
        new Promise((resolve) => {
          const id = ++n;
          waiting.set(id, resolve);
          worker.postMessage({ ...message, id });
        });
      const canvas = document.createElement("canvas").transferControlToOffscreen();
      worker.postMessage({ type: "open", source: { url: deck }, painter: "cpu", canvas }, [canvas]);
      const first = await opened;
      if (first.type !== "ready") return { error: first.message };
      const index = first.states.indexOf("containers");
      const { source: before } = await ask({ type: "source" });
      await ask({ type: "edit", source: before, index });
      const was = (await ask({ type: "inspect", state: "containers" })).inspected.nodes.tally.at;
      // The edit of a patch, and, in the same task, a request with the source the page held
      // before the edit's answer came: the worker takes it while the edit's paint yields.
      const making = ask({ type: "make", source: before, ops: [{ op: "place", node: "tally", at: { col: [1, 10], row: 5 } }], index });
      const listing = ask({ type: "bundleFiles", source: before });
      const [made, files] = await Promise.all([making, listing]);
      const { inspected } = await ask({ type: "inspect", state: "containers" });
      const { boxes } = await ask({ type: "boxes", state: "containers" });
      const { source: after } = await ask({ type: "source" });
      worker.terminate();
      return {
        was,
        now: inspected.nodes.tally.at,
        box: boxes.find((b) => b.node === "tally")?.rect.map(Math.round),
        files: files.files?.length ?? -1,
        sourceHeld: after === made.source,
        line: made.source.split("\n").find((l) => /^\s*tally\b/.test(l))?.trim(),
      };
    },
    { script, deck: `${server.origin}/tests/fixtures/torture.scaena/deck.json` },
  );
  if (result.error) throw new Error(`the worker did not open the deck: ${result.error}`);
  check(JSON.stringify(result.was.col) === "[1,11]", `tally stands on columns 1–11 before the edit: ${JSON.stringify(result.was)}`);
  check(/col\(1-10\)/.test(result.line ?? ""), `the edit's source places it on columns 1–10: ${result.line}`);
  check(result.files > 0, `the request with the older source is answered: ${result.files} files`);
  check(
    result.sourceHeld && JSON.stringify(result.now.col) === "[1,10]",
    `and the deck held stays the edit's, not compiled back from the older source: tally at ${JSON.stringify(result.now)}, box ${result.box}, the source ${result.sourceHeld ? "the edit's" : "the older one"}`,
  );
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`${failures.length} failure(s): ${failures.join("; ")}`);
  process.exit(1);
}
console.log("the worker keeps the latest edit's deck against a request with an older source");
