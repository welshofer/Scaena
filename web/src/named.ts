// A bundle's name for a deck by its title (W1's dogfood 20): its words, lowercase, joined by `-`,
// as a downloaded `.scaena` and a deck kept in the browser are named. Its own module, so Node
// checks it as it stands (`web/named.mjs`).

/** A bundle's name for a deck titled `title`: lowercase, its words joined by `-`. An accent a
 * Latin letter carries comes off ("Café" → "cafe"); a mark that is part of a letter in its own
 * script stays, ゴ never cut to コ, and a Korean syllable stays whole. */
export function nameFor(title: string): string {
  const words =
    title
      .normalize("NFKD")
      .toLowerCase()
      .replace(/['’]/g, "")
      .match(/[\p{L}\p{M}\p{N}]+/gu)
      ?.map((w) => w.replace(/[̀-ͯ]/g, "").normalize("NFC"))
      .filter((w) => w.length > 0) ?? [];
  return [...words.join("-")].slice(0, 64).join("") || "untitled";
}
