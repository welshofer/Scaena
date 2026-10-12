// SPEC §3.3 check: a picture too large for a slide, dropped on the editor's canvas in headless
// Chromium (serve.mjs), the CPU painting, on the revenue example.
//
//   node web/pictures.mjs     (after `just web`; from the repository's root)
//
// - A picture longer than 8192 pixels on a side is asked about, not refused: the question says how
//   large it is and how large the copy would be.
// - Declined, nothing goes in: the deck is as it was, and the bundle holds no new file.
// - Taken, a smaller copy goes in, named by its SHA-256 as any picture dropped is. A PNG stays a
//   PNG, its transparency kept: saved into the browser's storage and read back, the copy is 8192
//   pixels long, its clear half clear and its red half red. A JPEG stays a JPEG.
// Exits 1 on any failure.
import { launch, serve } from "./serve.mjs";

const server = await serve();
const failures = [];
const check = (ok, what) => {
  console.log(`${ok ? "ok  " : "FAIL"} ${what}`);
  if (!ok) failures.push(what);
};

const browser = await launch();
try {
  const context = await browser.newContext({ viewport: { width: 1920, height: 1200 }, deviceScaleFactor: 1 });
  const page = await context.newPage();
  page.on("pageerror", (e) => failures.push(`page: ${e.message}`));
  page.on("console", (m) => m.type() === "error" && failures.push(`console: ${m.text()}`));
  await page.goto(`${server.origin}/web/dist/editor.html?painter=cpu&bundle=/docs/examples/revenue.deck.json`);
  await page.waitForFunction(() => window.scaena?.last(), null, { timeout: 120000 });

  const source = () => page.evaluate(() => window.scaena.source());
  const status = () => page.evaluate(() => document.querySelector("#status").textContent);
  const trips = () => page.evaluate(() => window.scaena.trips().length);
  const settled = (before) =>
    page.waitForFunction((n) => window.scaena.trips().length > n && !window.scaena.canvas.busy(), before, { timeout: 60000, polling: 50 });
  const says = (what) =>
    page.waitForFunction((w) => document.querySelector("#status").textContent.includes(w), what, { timeout: 60000 }).then(
      () => true,
      () => false,
    );
  const files = async () => {
    await page.evaluate(() => window.scaena.files.refresh());
    return page.evaluate(() => window.scaena.files.listed().map((f) => f.path));
  };
  /** A picture 9000 × 40 made in the page as `type`: its left half clear, its right half red. */
  const made = (type) =>
    page.evaluate(async (t) => {
      const canvas = new OffscreenCanvas(9000, 40);
      const g = canvas.getContext("2d");
      g.fillStyle = "#ff0000";
      g.fillRect(4500, 0, 4500, 40);
      return [...new Uint8Array(await (await canvas.convertToBlob({ type: t })).arrayBuffer())];
    }, type);
  /** A drop of `bytes` as file `name` in the middle of the canvas. */
  const drop = async (name, bytes) => {
    const transfer = await page.evaluateHandle(
      ([b, file]) => {
        const dt = new DataTransfer();
        dt.items.add(new File([new Uint8Array(b)], file, { type: file.endsWith(".png") ? "image/png" : "image/jpeg" }));
        return dt;
      },
      [bytes, name],
    );
    const r = await page.locator("#overlay").boundingBox();
    await page.dispatchEvent("#overlay", "drop", { dataTransfer: transfer, clientX: r.x + r.width / 2, clientY: r.y + r.height / 2 });
  };
  /** What the person is asked when `name` is dropped as `bytes`, answered `yes` or not. */
  const asked = async (name, bytes, yes) => {
    const question = new Promise((resolve) =>
      page.once("dialog", (d) => {
        resolve(d.message());
        void (yes ? d.accept() : d.dismiss());
      }),
    );
    await drop(name, bytes);
    return Promise.race([question, new Promise((resolve) => setTimeout(() => resolve(""), 30000))]);
  };

  const original = await source();
  const before = await files();
  const png = await made("image/png");

  // Declined, nothing goes in.
  const question = await asked("wide.png", png, false);
  check(question.includes("9,000 × 40 pixels") && question.includes("8,192 on a side") && question.includes("8,192 × 36"), `a picture too large is asked about: ${question}`);
  check(await says("wide.png not added"), `declined, nothing is added: ${await status()}`);
  check((await source()) === original, "the deck is as it was");
  check(JSON.stringify(await files()) === JSON.stringify(before), "and the bundle holds no new file");

  // Taken, a smaller copy goes in: a PNG still.
  let n = await trips();
  await asked("wide.png", png, true);
  check(await says("kept as a smaller copy, assets/"), `taken, a smaller copy goes in: ${await status()}`);
  await settled(n);
  const path = (await status()).match(/assets\/[0-9a-f]{64}\.png/)?.[0] ?? "";
  check(path !== "" && (await source()).includes(`"${path}"`), `a PNG still, named by its SHA-256: ${path}`);
  check(await page.evaluate(() => window.scaena.last().valid), "the deck validates with it");

  // Saved and read back: 8192 pixels long, its clear half clear and its red half red.
  const saved = await page.evaluate(() => window.scaena.saveAs({ opfs: "pictures" }));
  check(saved?.where.kind === "opfs", `saved into the browser's storage: ${JSON.stringify(saved?.where)}`);
  const read = await page.evaluate(async (file) => {
    let dir = await navigator.storage.getDirectory();
    for (const part of ["bundles", "pictures", ...file.split("/").slice(0, -1)]) dir = await dir.getDirectoryHandle(part);
    const blob = await (await dir.getFileHandle(file.split("/").pop())).getFile();
    const head = new Uint8Array(await blob.slice(0, 26).arrayBuffer());
    const bitmap = await createImageBitmap(blob);
    const canvas = new OffscreenCanvas(bitmap.width, bitmap.height);
    const g = canvas.getContext("2d");
    g.drawImage(bitmap, 0, 0);
    return {
      signature: [...head.slice(1, 4)].map((b) => String.fromCharCode(b)).join(""),
      // IHDR's color type: 6 is RGBA, 4 gray with alpha.
      colorType: head[25],
      size: [bitmap.width, bitmap.height],
      clear: [...g.getImageData(100, 18, 1, 1).data],
      red: [...g.getImageData(8000, 18, 1, 1).data],
    };
  }, path);
  check(read.signature === "PNG" && (read.colorType === 6 || read.colorType === 4), `kept as a PNG with its transparency: ${read.signature}, color type ${read.colorType}`);
  check(read.size[0] === 8192 && read.size[1] === 36, `8192 pixels long: ${read.size}`);
  check(read.clear[3] === 0, `its clear half clear: ${read.clear}`);
  check(read.red[0] > 250 && read.red[1] < 5 && read.red[2] < 5 && read.red[3] === 255, `its red half red: ${read.red}`);

  // A JPEG stays a JPEG.
  n = await trips();
  await asked("wide.jpg", await made("image/jpeg"), true);
  check(await says("kept as a smaller copy, assets/"), `a JPEG too large goes in as a smaller copy too: ${await status()}`);
  await settled(n);
  const photo = (await status()).match(/assets\/[0-9a-f]{64}\.(png|jpg)/)?.[0] ?? "";
  check(photo.endsWith(".jpg") && (await source()).includes(`"${photo}"`), `a JPEG still: ${photo}`);
} catch (e) {
  failures.push(`error: ${e.message}`);
} finally {
  await browser.close();
  server.close();
}
if (failures.length) {
  console.log(`\n${failures.length} failed:\n${failures.map((f) => `  ${f}`).join("\n")}`);
  process.exit(1);
}
console.log("a picture too large goes in as a smaller copy, a PNG as a PNG with its transparency, if the person wants one");
