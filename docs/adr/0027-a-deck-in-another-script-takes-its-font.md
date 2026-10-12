# ADR-0027: A deck in another script takes the script's font

**Status:** proposed · **Date:** 2026-10-12

## Context

Narrato writes in 15 languages (`scaena_narrato::brief::LANGUAGES`). The themes that ship set their words in Fraunces, Inter, and JetBrains Mono. Since PLAN G.21, their fonts carry Latin, Greek, Cyrillic, and what a talk writes beside its words (U+2000–27BF), and Fraunces falls back on Inter for Greek and Cyrillic.

None of those fonts draws five of the languages: Japanese, Chinese (Simplified), Korean, Arabic, and Hindi.
- Before G.21, a deck in one of them came out blank. The builder writes plainly what its fonts cannot draw, and for a letter that is nothing.
- Since G.21, `scaena_ops::narrative::writable` refuses such a deck before any call is paid for.

The constraints:
- **No system fonts.** A bundle draws only with its own fonts (invariant 1, SPEC §13).
- **No server of Scaena's** (invariant 7).
- **Size.** At the commit the shipped fonts come from, each script's Noto Sans is variable in weight:

  | Font | Size |
  |---|---|
  | Noto Sans JP | 9.6 MB |
  | Noto Sans SC | 17.8 MB |
  | Noto Sans KR | 10.4 MB |
  | Noto Sans Arabic | 0.8 MB |
  | Noto Sans Devanagari | 0.6 MB |

  Carried by the CLI, the pages, and the Mac app, they would add 39 MB to each.
- **A font catalog is Jay's open question** (SPEC §16 Q8): filled on demand, with script fallback chains drawn from it.

## Decision

1. **Where the fonts come from.**
   - They come from google/fonts, at the commit the shipped fonts come from: Noto Sans JP, SC, KR, Arabic, and Devanagari, one for each script.
   - `scaena_ops::scripts` names each by its path there and its SHA-256.
   - Each carries its copyright and the OFL in its name table, as the shipped fonts do, and lets a document embed it (W230 finds nothing).
2. **When they are fetched.**
   - **What decides.** A deck takes a script's font when it is written in a script the theme's fonts do not draw. The brief's language names the script; with no language named, the slides given are read for one (`written_in`).
   - **When.** Before a file is read or any call is paid for.
   - **Who fetches it.** The client fetches it as it fetches a source (G.4):
     - `scaena generate`, with `ureq`;
     - the page's generation worker, with `fetch` (raw.githubusercontent.com lets any page read it);
     - the Mac, with `URLSession`.
   - **Where it is kept.** Each client keeps the font, so the next deck in the script fetches nothing:
     - the CLI, by digest in its cache directory (`$XDG_CACHE_HOME/scaena/fonts`, else `~/.cache/scaena/fonts`);
     - the page, in Cache Storage, by where it comes from;
     - the Mac, by digest in Application Support.
   - **Checked first.** Rust holds the font to its digest before anything reads it, and a kept font that no longer matches is fetched again. A font that does not match, or does not come, stops the deck there, saying why.
   - **Elsewhere.** `$SCAENA_FONTS_BASE` points the CLI at another copy of google/fonts.
3. **How the bundle carries it.**
   - **The family.** The bundle's copy of the theme gains a family keyed by the script (`japanese`, `chinese`, `korean`, `arabic`, `devanagari`). It is named last in the `fallback` of the family of every role but code's, so the theme's own faces set what they draw (digits, Latin, punctuation) and the script's font sets the rest.
   - **The lines.** A theme set for Latin sets its headlines close. Each role but code's opens its leading to what the script's marks need: 1.3 for Han, kana, and Hangul, 1.45 for Devanagari, and 1.6 for Arabic. Where the role snaps to the baseline grid, the leading is rounded up onto it. Each role's box is its whole lines (`box: line`), so a mark above or below stays in its box and lint E101 judges what the reader sees.
   - **Tracking.** None is tracked closer than its font sets it. Arabic, whose letters join, and Devanagari, whose letters stack, are not tracked at all.
   - **The deck.** Its `fonts` lists the file, and its `lang` is the language's.
   - **A re-theme.** It carries the family over to the theme the deck moves to (`scaena_ops::theme::theming`), so the deck still draws in its letters there.
4. **What a save keeps.** A save keeps what the deck draws, and what its script writes most:
   - Arabic and Devanagari, whole: each is small.
   - For Han, kana, or Hangul: the script's punctuation, kana, and full-width forms, whole.
   - The first level of each script's national standard, each character the font has: JIS X 0208's 2,965 kanji, GB 2312's 3,755 hanzi, and KS X 1001's 2,350 Hangul syllables. Python's codecs encode exactly those, and `scripts/build_scripts.py` writes them into `scaena_store::common`.

   A deck saved can then be edited in its language without the font it was cut from. A rarer character typed later has no glyph, which is lint E120.
5. **Right to left.** An Arabic deck's paragraphs run right to left. The engine already aligns a paragraph in its own direction, so text aligned `start` is set flush right in its slot. The slots stay where the layout puts them; mirroring a layout is not part of this.
6. **Where a line breaks.** UAX #14 breaks between any two Han or kana letters and any two Hangul syllables. Balanced alone, a cover's headline cut a word in two: 我 over 们, and 우리 over 는.
   - **Korean** breaks between its words alone, at its spaces, in every wrap, as CSS `word-break: keep-all` sets it.
   - **A balanced headline in Chinese or Japanese** breaks after its punctuation, or where kana gives way to Han or katakana, as a phrase ends at its particle: 気候変動： over 私たちの現在地. It keeps as many lines, the widest as narrow as it can be, as a hyphenated word is kept whole. Where no such break fits, it breaks as before.
   - **Running text** breaks between any two letters, as it is set in print.

## Consequences

- Every language Narrato offers makes a deck that reads, on every client.
- The first deck in a script fetches its font once: 0.6 to 18 MB.
- A saved bundle in Japanese, Chinese, or Korean carries 1.5–3.8 MB of its script's font, its commonest characters' glyphs among them. Saved from five slides, Noto Sans KR kept 1.46 MB of 10.4, JP 3.18 of 9.6, and SC 3.77 of 17.8. Kept to the deck's own characters alone, it would carry 0.6–0.8 MB, and nearly any word typed in it next would have no glyph.
- Offline, a first deck in a script cannot be made, and the refusal says so.
- The render path is unchanged: bundle fonts only.
- Nothing here knows Chinese or Japanese words: ICU4X carries no dictionary for them in this build. A Chinese headline with no punctuation may still break inside a word. A word segmenter for headlines, and mirroring a layout for Arabic, are follow-ups.

## Alternatives

- **Shipping the five fonts.** They would add 39 MB to the CLI, the site, and the app, for decks most users never make.
- **System fonts.** They break determinism (invariant 1).
- **A romanized deck.** It is not in the language asked for.
- **A font of the user's choosing.** That is SPEC §16 Q8, which stays open. This decision is the script fallback that Q8's catalog would hold.
