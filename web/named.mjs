// A bundle's name for a deck by its title (W1's dogfood 20): a Japanese title's voiced kana keep
// their mark, ソラカーゴ never ソラカーコ, a Korean title's syllables stay whole, and a Latin letter's
// accent still comes off. Node reads web/src/named.ts as it stands, its types stripped.
//
//   node --experimental-strip-types web/named.mjs   (from the repository's root)
//
// Exits 1 on any failure.
import { nameFor } from "./src/named.ts";

const failures = [];
const check = (got, want) => {
  const ok = got === want;
  console.log(`${ok ? "ok  " : "FAIL"} ${JSON.stringify(want)}${ok ? "" : `, got ${JSON.stringify(got)}`}`);
  if (!ok) failures.push(want);
};

check(nameFor("ソラカーゴ"), "ソラカーゴ");
check(nameFor("パンとバター"), "パンとバター");
check(nameFor("서울의 봄"), "서울의-봄");
check(nameFor("Café de Flore"), "cafe-de-flore");
check(nameFor("Q3 Review: what's next"), "q3-review-whats-next");
check(nameFor("¡¿!"), "untitled");
check([...nameFor("あ".repeat(100))].length === 64 ? "64" : "not 64", "64");

if (failures.length) {
  console.log(`${failures.length} failed`);
  process.exit(1);
}
