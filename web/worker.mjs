// The worker alone, in headless Chromium, on the torture deck: a request that carries a source
// older than the latest edit's, posted before that edit was answered and taken while its paint
// yields, must not compile its source over the deck the edit made (lift-up: the race #469's canvas
// red found, where two arrow presses in a row and the Theme panel's bundleFiles between them left
// the worker's deck a track behind the source). Nor may it compile over the deck while an edit
// lints it: a step added would be linted on a deck without it and refused (new.mjs's "+ Step" on
// CI's slower runners, its frame's backdrop painted by the CPU painter). The worker is spoken to by
// its own protocol, from the editor's page, which spawned it.
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
      // A patch that adds a step, and in the same task a request with the source from before it:
      // the edit lints the step it made, on the deck it compiled, so the patch is made.
      const { added } = await ask({ type: "addingState", source: after, state: "containers", what: "step" });
      const stepping = ask({ type: "make", source: after, ops: added.patch, index: index + 1 });
      const older = ask({ type: "bundleFiles", source: after });
      const [stepped] = await Promise.all([stepping, older]);
      const { source: stepSource } = await ask({ type: "source" });
      // An undo, by the page's versions: the deck compiled at version 1, a patch made on it, and
      // the patch's source taken as version 2; then ⌘Z puts version 3, the source before the
      // patch again, and a request with it is posted before its edit. It is newer than the deck
      // held, so the deck is compiled from it, not answered as it stood before the undo.
      await ask({ type: "edit", source: stepSource, index, version: 1 });
      const patched = await ask({ type: "make", source: stepSource, ops: [{ op: "place", node: "tally", at: { col: [1, 9], row: 5 } }], index, version: 1 });
      await ask({ type: "edit", source: patched.source, index, version: 2 });
      await ask({ type: "bundleFiles", source: stepSource, version: 3 });
      const { source: undone } = await ask({ type: "source" });
      // And a request from before the patch, its version older than the deck held, is answered
      // from that deck.
      await ask({ type: "edit", source: patched.source, index, version: 4 });
      await ask({ type: "bundleFiles", source: stepSource, version: 3 });
      const { source: kept } = await ask({ type: "source" });
      // The linter's edit of the source the page had while a patch was made on it, posted in the
      // same task as the patch (#597's builds.mjs on CI): the deck stays the patch's, and the step
      // it added reads.
      const { added: more } = await ask({ type: "addingState", source: kept, state: "containers", what: "step", version: 5 });
      const adding = ask({ type: "make", source: kept, ops: more.patch, index: index + 1, version: 5 });
      const linting = ask({ type: "edit", source: kept, index, version: 5 });
      await Promise.all([adding, linting]);
      const readOut = await ask({ type: "read", state: more.id });
      worker.terminate();
      return {
        lintedOver: readOut.type === "error" ? `${more.id}: ${readOut.message}` : "",
        undoneHeld: undone === stepSource,
        staleKept: kept === patched.source,
        step: added.id,
        stepped: stepped.type === "error" ? `refused: ${stepped.message}` : stepSource.includes(`state ${added.id}`) ? "made" : "missing",
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
  check(result.undoneHeld, "a request with an undo's source, newer than the deck, compiles it: the deck is the source before the patch again");
  check(!result.lintedOver, `the linter's edit of the source before a patch leaves the patch's deck, its step read: ${result.lintedOver || "it reads"}`);
  check(result.staleKept, "a request older than the deck held is answered from it, the patch kept");
  check(result.stepped === "made", `a step added, with a request of the source before it taken meanwhile, is made: ${result.step} ${result.stepped}`);
} finally {
  await browser.close();
  server.close();
}

if (failures.length) {
  console.error(`${failures.length} failure(s): ${failures.join("; ")}`);
  process.exit(1);
}
console.log("the worker keeps the latest edit's deck against a request with an older source");
