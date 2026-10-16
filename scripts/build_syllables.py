#!/usr/bin/env python3
"""The table of English words the engine's syllable rules miss (PLAN O.4a, ADR-0030 decision 27).

`scaena_core::verse` counts an English word by rules after words/syllable 5.0.1's (MIT), and first
looks it up in a table: the 2,000 most frequent words whose count the rules miss. Both come from
two sources, read once and kept as far as this script needs them:

- **CMUdict** (BSD-style, Carnegie Mellon University), at a pinned commit and SHA-256: each word's
  syllables are its vowels, the phones that carry a stress. A word said two ways counts either.
- **Google Books Ngram**'s English 1-grams, version 20200217 (CC BY 3.0): each CMUdict word's
  matches in books from 1950 on rank it. The 24 files are 13.4 GB gzipped, so they are streamed,
  each held to its SHA-256 as it passes, and only CMUdict's words' totals are kept.

What is kept is `tests/verse/syllables.tsv`: the 30,000 most frequent words, in that order, each
with how CMUdict says it and what these rules count. The rules here are words/syllable's own
expressions; `crates/scaena-core/tests/verse.rs` holds the engine's, written over bytes with no
expression, to the `rules` column for every word, so the two never drift apart.

From it this script writes the engine's table, `crates/scaena-core/src/verse/words.txt`: one
string of lines sorted by their bytes, each a word and its count (`fire12`: one or two), which the
engine searches by halves. The notices of all three sources stand beside it, in `NOTICE.md`.

    python3 scripts/build_syllables.py            # fetch both sources, write the kept words, then the table
    python3 scripts/build_syllables.py --check    # no network (CI): the table and the rules' column are what the kept words make
"""

import argparse
import hashlib
import re
import sys
import urllib.request
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
KEPT = ROOT / "tests/verse/syllables.tsv"
TABLE = ROOT / "crates/scaena-core/src/verse/words.txt"
VERSE = ROOT / "crates/scaena-core/src/verse.rs"  # names the table's SHA-256, which a handed-over table is held to

CMUDICT_COMMIT = "74790861f652b15e4ac49015a90074ad62a27690"
CMUDICT = f"https://raw.githubusercontent.com/cmusphinx/cmudict/{CMUDICT_COMMIT}/cmudict.dict"
CMUDICT_SHA256 = "81917843c7f44ce2b094ac63873c2c7a4cf802040792c455ba3ca406891c3d22"

NGRAMS = "http://storage.googleapis.com/books/ngrams/books/20200217/eng/1-{:05d}-of-00024.gz"
# Each file's SHA-256, as streamed on 2026-10-15.
NGRAM_SHA256 = [
    "d2568b44db1bf33e02b76373aa2861517332c84137ba3a8cee657441496c4b45",
    "ea4828275463479b5acdb055e5f5bea9f9cd518f9613dc3a2ebd84c5d3061e3c",
    "1ab1dba104d181b0aa883b0d7831cde48060c1e432204be19bb995c8efa34c1b",
    "c4a1d91fb7f29089e247450c768ac7f76b91b5225713ca36ab48fcd1da3e4e06",
    "799f6e9514c988b7655c9601a8f3abb912bf10741d4ab238a198231b664bd154",
    "e59cb5e596ba2a929dbf95fc1a44cb6d8559fc4c857a30e46ed8112ad6205b03",
    "e0d13c8664bf917a4cf40805a5191125f31275dc2a3d7f106dc132493b94753a",
    "1b9371bccc875ca5cb2ef48cd32e9b86bfe5270f4d3f83257a5d0b98a64c3bb8",
    "684d05885d5e00e60bc85cf03fe857fa4938d1f8e967b037677c5f28384214be",
    "b473a861daa57500f40b4b222b9fc58f9d2f1f64cba34525c28266b0265b22b2",
    "1815a46f1fa2d01c6cffc02a4590ab7601171e7762e668af17140df3c16eca58",
    "6438a13d25af6208ad628db5b0fe89a6705fb09960673f90fd8111fd174f1c6d",
    "39c80b9d2e2dc51d8dfc4f7870141d0b3a0082fdead87ae55f495a3bc79f2e1d",
    "7d4b084f854870d3776f5c9ba253c5690b0ad5f74b992a6e323c4676680d7c44",
    "6267febda571239df3aa1ecd20bfa0bb6126e6dbf78cb35e94a55e1a4a21df82",
    "48781c66f2211f3ad36c8785195c4b4c7926a1813c60b93e79807ce79ab0a7a7",
    "6c6af5ae894951e040b9c5f3417fc4765a92d0f28925d635eab25421a45b1410",
    "e3febf59da2f720cea6c633d7bbd5b96120b36ad205b5f58ace487e8920c6875",
    "7f2715ec65466efdba2ce2e399dff741268b58dfec4f1ab4b4820367cac65ae9",
    "98b48b4ca15846a5908d51f199fa6a80d5b937ed3d0edd7b4d09eda8b3bb7cdf",
    "e6ef1b0f26f1519cc3b9cc9d6aa3ea018b82e868785029aa581e3ff32f3a2092",
    "ebe0439514fc965ededccdeaf71c8ce8486855418a3e635c82aa5e31f9998804",
    "db63aa244891dc2fd00cb996341c7b3268a22fe7bc44d22e2e1805c3e162f85b",
    "76fb6a9a9301d8dc3ef4083602e0dea6f5a5f0457100f224fb6c3867ad98865d",
]
SINCE = 1950  # a year's matches count from this year on
KEEP = 30_000  # the words kept, most frequent first
TABLE_WORDS = 2_000  # the words the table holds

HEADER = "# word\tsaid\trules  (CMUdict at %s, ranked by Google Books Ngram 20200217 since %d)\n" % (
    CMUDICT_COMMIT[:12],
    SINCE,
)

# --- words/syllable 5.0.1's rules (MIT, Titus Wormer), its expressions as it writes them --------

PROBLEMATIC = {
    "abalone": 4, "abare": 3, "abbruzzese": 4, "abed": 2, "aborigine": 5, "abruzzese": 4,
    "acreage": 3, "adame": 3, "adieu": 2, "adobe": 3, "anemone": 4, "anyone": 3, "apache": 3,
    "aphrodite": 4, "apostrophe": 4, "ariadne": 4, "cafe": 2, "calliope": 4, "catastrophe": 4,
    "chile": 2, "chloe": 2, "circe": 2, "coyote": 3, "daphne": 2, "epitome": 4, "eurydice": 4,
    "euterpe": 3, "every": 2, "everywhere": 3, "forever": 3, "gethsemane": 4, "guacamole": 4,
    "hermione": 4, "hyperbole": 4, "jesse": 2, "jukebox": 2, "karate": 3, "machete": 3,
    "maybe": 2, "naive": 2, "newlywed": 3, "penelope": 4, "people": 2, "persephone": 4,
    "phoebe": 2, "pulse": 1, "queue": 1, "recipe": 3, "riverbed": 3, "sesame": 3, "shoreline": 2,
    "simile": 3, "snuffleupagus": 5, "sometimes": 2, "syncope": 3, "tamale": 3, "waterbed": 3,
    "wednesday": 2, "yosemite": 4, "zoe": 2,
}


def _join(parts):
    return "|".join(parts)


MONO_ONE = re.compile(_join([
    "awe($|d|so)", "cia(?:l|$)", "tia", "cius", "cious", "[^aeiou]giu", "[aeiouy][^aeiouy]ion",
    "iou", "sia$", "eous$", "[oa]gue$", ".[^aeiuoycgltdb]{2,}ed$", ".ely$", "^jua", "uai", "eau",
    "^busi$",
    "(?:[aeiouy](?:" + _join(["[bcfgklmnprsvwxyz]", "ch", "dg", "g[hn]", "lch", "l[lv]", "mm", "nch",
                              "n[cgn]", "r[bcnsv]", "squ", "s[chkls]", "th"]) + ")ed$)",
    "(?:[aeiouy](?:" + _join(["[bdfklmnprstvy]", "ch", "g[hn]", "lch", "l[lv]", "mm", "nch", "nn",
                              "r[nsv]", "squ", "s[cklst]", "th"]) + ")es$)",
]))
MONO_TWO = re.compile("[aeiouy](?:" + _join([
    "[bcdfgklmnprstvyz]", "ch", "dg", "g[hn]", "l[lv]", "mm", "n[cgns]", "r[cnsv]", "squ",
    "s[cklst]", "th"]) + ")e$")
DOUBLE_ONE = re.compile("(?:" + _join([
    "([^aeiouy])\\1l", "[^aeiouy]ie(?:r|s?t)", "[aeiouym]bl", "eo", "ism", "asm", "thm", "dnt",
    "snt", "uity", "dea", "gean", "oa", "ua", "react?", "orbed", "shred", "eings?",
    "[aeiouy]sh?e[rs]"]) + ")$")
DOUBLE_TWO = re.compile(_join([
    "creat(?!u)", "[^gq]ua[^auieo]", "[aeiou]{3}", "^(?:ia|mc|coa[dglx].)", "^re(app|es|im|us)",
    "(th|d)eist"]))
DOUBLE_THREE = re.compile(_join([
    "[^aeiou]y[ae]", "[^l]lien", "riet", "dien", "iu", "io", "ii", "uen", "[aeilotu]real",
    "real[aeilotu]", "iell", "eo[^aeiou]", "[aeiou]y[aeiou]"]))
DOUBLE_FOUR = re.compile("[^s]ia")
SINGLE = re.compile(_join([
    "^(?:" + _join(["un", "fore", "ware", "none?", "out", "post", "sub", "pre", "pro", "dis", "side",
                   "some"]) + ")",
    "(?:" + _join(["ly", "less", "some", "ful", "ers?", "ness", "cians?", "ments?", "ettes?",
                  "villes?", "ships?", "sides?", "ports?", "shires?", "[gnst]ion(?:ed|s)?"]) + ")$",
]))
DOUBLE = re.compile(_join([
    "^(?:" + _join(["above", "anti", "ante", "counter", "hyper", "afore", "agri", "infra", "intra",
                   "inter", "over", "semi", "ultra", "under", "extra", "dia", "micro", "mega",
                   "kilo", "pico", "nano", "macro", "somer"]) + ")",
    "(?:fully|berry|woman|women|edly|union|((?:[bcdfghjklmnpqrstvwxz])|[aeiou])ye?ing)$",
]))
TRIPLE = re.compile("(creations?|ology|ologist|onomy|onomist)$")


def singular(word: str) -> str | None:
    """The singular a problematic word's plural names: words/syllable asks `pluralize`; every word
    in its list makes its plural with -s or -es."""
    for end in ("es", "s"):
        if word.endswith(end) and word[: -len(end)] in PROBLEMATIC:
            return word[: -len(end)]
    return None


def rules(word: str) -> int:
    """Syllables in one word of letters a–z, by words/syllable's rules."""
    if not word:
        return 0
    if len(word) < 3:
        return 1
    if word in PROBLEMATIC:
        return PROBLEMATIC[word]
    one = singular(word)
    if one is not None:
        return PROBLEMATIC[one]
    count = 0

    def take(n):
        def counter(_):
            nonlocal count
            count += n
            return ""
        return counter

    word = TRIPLE.sub(take(3), word)
    word = DOUBLE.sub(take(2), word)
    word = SINGLE.sub(take(1), word)
    count += sum(1 for part in re.split("[^aeiouy]+", word) if part)
    for expression in (MONO_ONE, MONO_TWO):
        count -= sum(1 for _ in expression.finditer(word))
    for expression in (DOUBLE_ONE, DOUBLE_TWO, DOUBLE_THREE):
        count += sum(1 for _ in expression.finditer(word))
    # Its last expression has no `g`, so it adds one at most.
    count += 1 if DOUBLE_FOUR.search(word) else 0
    return count or 1


# --- the sources --------------------------------------------------------------------------------


def key(word: str) -> str | None:
    """A CMUdict word as the engine looks it up: its letters, apostrophes dropped; none where it
    holds anything but letters and apostrophes."""
    letters = word.replace("'", "")
    return letters if letters and letters.isascii() and letters.isalpha() else None


def cmudict() -> dict[str, set[int]]:
    """Each word's syllable counts, one for each way CMUdict says it."""
    with urllib.request.urlopen(CMUDICT, timeout=300) as r:
        data = r.read()
    digest = hashlib.sha256(data).hexdigest()
    if digest != CMUDICT_SHA256:
        sys.exit(f"cmudict.dict: SHA-256 {digest}, not {CMUDICT_SHA256}")
    said: dict[str, set[int]] = {}
    for line in data.decode("utf-8").splitlines():
        line = line.split("#", 1)[0].strip()
        if not line:
            continue
        word, *phones = line.split()
        word = word.split("(", 1)[0]
        k = key(word.lower())
        if k is None:
            continue
        said.setdefault(k, set()).add(sum(1 for p in phones if p[-1].isdigit()))
    return said


def frequencies(words: set[str]) -> dict[str, int]:
    """Each word's matches since SINCE, from every 1-gram written as it is (no part of speech),
    in any case, its apostrophes dropped; each file streamed and held to its SHA-256."""
    totals: dict[str, int] = {}

    def take(line: bytes):
        tab = line.find(b"\t")
        token = line[:tab]
        if b"_" in token:
            return
        try:
            k = key(token.decode("utf-8").lower())
        except UnicodeDecodeError:
            return
        if k is None or k not in words:
            return
        matches = 0
        for year in line[tab + 1:].split(b"\t"):
            y, m, _ = year.split(b",")
            if int(y) >= SINCE:
                matches += int(m)
        totals[k] = totals.get(k, 0) + matches

    for index, expected in enumerate(NGRAM_SHA256):
        url = NGRAMS.format(index)
        print(f"streaming {url}", file=sys.stderr)
        digest = hashlib.sha256()
        inflate = zlib.decompressobj(16 + zlib.MAX_WBITS)
        rest = b""
        with urllib.request.urlopen(url, timeout=600) as r:
            while chunk := r.read(1 << 20):
                digest.update(chunk)
                while chunk:
                    rest += inflate.decompress(chunk)
                    chunk = b""
                    if inflate.eof:  # the next gzip member
                        chunk = inflate.unused_data
                        inflate = zlib.decompressobj(16 + zlib.MAX_WBITS)
                lines = rest.split(b"\n")
                rest = lines.pop()
                for line in lines:
                    if line:
                        take(line)
        if rest:
            take(rest)
        if digest.hexdigest() != expected:
            sys.exit(f"{url}: SHA-256 {digest.hexdigest()}, not {expected}")
    return totals


# --- what is kept, and the table ----------------------------------------------------------------


def said(counts: set[int], word: str) -> tuple[int, int]:
    """The counts a word may take: each way CMUdict says it, and words/syllable's own count where
    its list of hard words names it, as poets say it (`every`, two)."""
    counts = set(counts)
    if word in PROBLEMATIC:
        counts.add(PROBLEMATIC[word])
    return min(counts), max(counts)


def span(low: int, high: int) -> str:
    return str(low) if low == high else f"{low}-{high}"


def write_kept():
    words = cmudict()
    totals = frequencies(set(words))
    ranked = sorted((w for w in totals if totals[w] > 0), key=lambda w: (-totals[w], w))[:KEEP]
    lines = [HEADER]
    for word in ranked:
        low, high = said(words[word], word)
        lines.append(f"{word}\t{span(low, high)}\t{rules(word)}\n")
    KEPT.parent.mkdir(parents=True, exist_ok=True)
    KEPT.write_text("".join(lines), encoding="utf-8")


def read_kept() -> list[tuple[str, tuple[int, int], int]]:
    kept = []
    for line in KEPT.read_text(encoding="utf-8").splitlines():
        if line.startswith("#"):
            continue
        word, saying, counted = line.split("\t")
        low, _, high = saying.partition("-")
        kept.append((word, (int(low), int(high or low)), int(counted)))
    return kept


def table(kept) -> str:
    """The 2,000 most frequent words whose saying the rules miss, each with its count, sorted by
    their bytes: `every23`."""
    missed = [(word, saying) for word, saying, _ in kept if saying != (rules(word), rules(word))]
    entries = []
    for word, (low, high) in missed[:TABLE_WORDS]:
        assert high <= 9, word
        entries.append(f"{word}{low}" + (str(high) if high != low else ""))
    return "\n".join(sorted(entries)) + "\n"


def digest_line(text: str) -> str:
    return 'pub const TABLE_SHA256: &str = "%s";' % hashlib.sha256(text.encode("utf-8")).hexdigest()


def write_digest(text: str):
    source = VERSE.read_text(encoding="utf-8")
    source = re.sub(r'pub const TABLE_SHA256: &str = "[0-9a-f]*";', digest_line(text), source)
    VERSE.write_text(source, encoding="utf-8")


def check() -> list[str]:
    problems = []
    kept = read_kept()
    if len(kept) != KEEP:
        problems.append(f"{KEPT}: {len(kept)} words, not {KEEP}")
    if len({w for w, _, _ in kept}) != len(kept):
        problems.append(f"{KEPT}: a word appears twice")
    for word, _, counted in kept:
        if not (word.isascii() and word.isalpha() and word.islower()):
            problems.append(f"{KEPT}: `{word}` is not a word of letters a–z")
        elif rules(word) != counted:
            problems.append(f"{KEPT}: `{word}` counts {rules(word)} by these rules, not {counted}")
    if not TABLE.exists() or TABLE.read_text(encoding="utf-8") != table(kept):
        problems.append(f"{TABLE} is not what {KEPT} makes; run scripts/build_syllables.py")
    elif digest_line(TABLE.read_text(encoding="utf-8")) not in VERSE.read_text(encoding="utf-8"):
        problems.append(f"{VERSE}: TABLE_SHA256 is not {TABLE}'s; run scripts/build_syllables.py")
    return problems


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--check", action="store_true", help="check, without the network")
    args = parser.parse_args()
    if not args.check:
        write_kept()
        TABLE.write_text(table(read_kept()), encoding="utf-8")
        write_digest(TABLE.read_text(encoding="utf-8"))
    problems = check()
    for problem in problems:
        print(problem, file=sys.stderr)
    if problems:
        sys.exit(1)
    print(f"{len(read_kept())} words kept; the table holds {TABLE_WORDS}")


if __name__ == "__main__":
    main()
