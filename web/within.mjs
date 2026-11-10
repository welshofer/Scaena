// The web loader fetches only the bundle's own files (the 20261031 plan's SEC-1): a path a deck
// names that resolves to another origin, or out of the deck's folder, is refused, naming it,
// before anything is fetched, as `scaena_store`'s `inside` refuses it natively. Node reads
// web/src/within.ts as it stands, its types stripped. web/worker.mjs holds the worker to it.
//
//   node --experimental-strip-types web/within.mjs   (from the repository's root)
//
// Exits 1 on any failure.
const { within } = await import("./src/within.ts");

const failures = [];
const check = (ok, what) => (ok ? console.log(`ok   ${what}`) : (failures.push(what), console.log(`FAIL ${what}`)));
const refusal = (path, deck) => {
  try {
    return `fetched ${within(path, deck)}`;
  } catch (e) {
    return e.message;
  }
};

const deck = "http://127.0.0.1:8000/decks/talk.scaena/deck.json";
for (const [path, at] of [
  ["deck.json", "http://127.0.0.1:8000/decks/talk.scaena/deck.json"],
  [deck, deck],
  ["fonts/Inter.ttf", "http://127.0.0.1:8000/decks/talk.scaena/fonts/Inter.ttf"],
  ["assets/a%20b.png", "http://127.0.0.1:8000/decks/talk.scaena/assets/a%20b.png"],
  ["./data/../data/q.csv", "http://127.0.0.1:8000/decks/talk.scaena/data/q.csv"],
])
  check(String(within(path, deck)) === at, `${path} is the bundle's: ${refusal(path, deck)}`);

for (const path of [
  "https://evil.example/x.png",
  "//evil.example/x.png",
  "http://127.0.0.1:8001/decks/talk.scaena/x.png",
  "../other.scaena/deck.json",
  "%2e%2e/other.scaena/deck.json",
  "/scaena/state",
  "data:text/plain,hi",
  "javascript:alert(1)",
]) {
  const said = refusal(path, deck);
  check(said.startsWith(`${path}: `) && !said.startsWith("fetched"), `${path} is refused, naming it: ${said}`);
}

if (failures.length) {
  console.error(`${failures.length} failure(s)`);
  process.exit(1);
}
console.log("the loader fetches the bundle's own files alone");
