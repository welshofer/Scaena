//! Hyphenation patterns (SPEC §3.5, ADR-0004 finding 11, ADR-0015): TeX's hyph-utf8 patterns
//! for the seventeen languages the engine hyphenates, compiled to tries by `hypher` 0.1.8 and
//! walked here as `hypher` walks them. Its code is MIT/Apache-2.0, and the walker below is cut
//! from it; the tries are its files, byte for byte (`hyphenation/`, whose README says whence).
//!
//! The tries are compiled into the engine with the `hyphenation` feature: in the CLI, the MCP
//! server, and the player's module. The editor's module leaves them out, a ninth of what it may
//! weigh (SPEC §15): its page hands a language's trie over the first time a text hyphenates in
//! it ([`set_loader`]). A trie is held to its SHA-256, so a frame is the same wherever its
//! patterns came from, and a language whose trie cannot be had is an error that says so, never
//! a text set without hyphens.

use crate::EngineError;
use sha2::{Digest, Sha256};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

/// A language the engine hyphenates.
pub struct Language {
    /// Its ISO 639-1 code, which names its trie: `de`, `hyphenation/de.bin`.
    pub code: &'static str,
    /// The fewest characters a hyphen leaves before it, and after it.
    left: usize,
    right: usize,
    /// Its trie's SHA-256, in hex.
    digest: &'static str,
}

const fn lang(code: &'static str, left: usize, right: usize, digest: &'static str) -> Language {
    Language { code, left, right, digest }
}

/// The languages the engine hyphenates, with `hypher`'s bounds for each.
pub const LANGUAGES: [Language; 17] = [
    lang("en", 2, 3, "be2bcb386cb777a191813f68b995d0c22fbf30783ac52a0ad2489c8ebd7fd354"),
    lang("de", 2, 2, "dc057dc24e655a602c6a26ba0bd1532fd143d5d5057c4aec58ce4fe7b30bf96e"),
    lang("fr", 2, 2, "64359f17aeb80371018144eaa1afb638da546dfeb594e17a7e819eb8a39c06c5"),
    lang("es", 2, 2, "fdca79005c290d9de18d753df630e5accd78333a3c73f418b9eff4a78499fbff"),
    lang("it", 2, 2, "a9dc16dbaa585c43b6db52e345c8d49bedf3c71df234a487e5d318adea0c24e7"),
    lang("pt", 2, 3, "4d55b1babb909cd40dd6f359442ddc77350bbca705246b8b020ce81be6e043a3"),
    lang("nl", 2, 2, "1177ef126b31bbb30f080633680fc14cc4cd6f56d1597981c975389b861d978b"),
    lang("sv", 2, 2, "eb9a3be6acbcba9df573a3b1065b0cf8ea2b08da7c2897a965f7280cd1639419"),
    lang("da", 2, 2, "c69e3d5d43abe2697b2220848a95225a14b292cc4611bf86864a031150084f42"),
    lang("fi", 2, 2, "365d87c66398e2cdae80eee5114d9fcbf589131cea1b5701c63acb7306011f14"),
    lang("pl", 2, 2, "cf4e14e2cc9b030b8ef8a1cff037c82bc06ac104d30a3f5f5c58809b3cf30cbd"),
    lang("cs", 2, 2, "1f7cb32555583658b8bc6daa35d213e856f28b69d47f44322eae2e11c2037557"),
    lang("ru", 2, 2, "0961ecad3a031ab9bd3f3d3e88fbda330dcaab7c3d2bbc0213d742c95b201818"),
    lang("uk", 2, 2, "606c09f88c8bb713e18feaa53c231666b20e55c8f2032c780ce282c5d0a8cb6f"),
    lang("tr", 2, 2, "1661448a091060bdee0d923c5489f6b717dc3dd11f00e2eaed364c8e07889c13"),
    lang("el", 1, 1, "a3138a9bc257b374bcd918fb193592af8a02a841025f68c919f0d1138683a48c"),
    lang("ca", 2, 2, "27a70e94fb14d5c2df252a6df211f69e7df9f4d0de9a014e0466a2ca731ddf5a"),
];

/// The language a BCP 47 tag hyphenates in, by its language subtag (`de-CH`: `de`); none
/// where the engine hyphenates in no such language.
pub fn language(tag: &str) -> Option<&'static Language> {
    let code = tag.split(['-', '_']).next()?.to_ascii_lowercase();
    LANGUAGES.iter().find(|l| l.code == code)
}

/// A trie compiled in.
#[cfg(feature = "hyphenation")]
fn compiled(code: &str) -> Option<&'static [u8]> {
    Some(match code {
        "en" => include_bytes!("../hyphenation/en.bin"),
        "de" => include_bytes!("../hyphenation/de.bin"),
        "fr" => include_bytes!("../hyphenation/fr.bin"),
        "es" => include_bytes!("../hyphenation/es.bin"),
        "it" => include_bytes!("../hyphenation/it.bin"),
        "pt" => include_bytes!("../hyphenation/pt.bin"),
        "nl" => include_bytes!("../hyphenation/nl.bin"),
        "sv" => include_bytes!("../hyphenation/sv.bin"),
        "da" => include_bytes!("../hyphenation/da.bin"),
        "fi" => include_bytes!("../hyphenation/fi.bin"),
        "pl" => include_bytes!("../hyphenation/pl.bin"),
        "cs" => include_bytes!("../hyphenation/cs.bin"),
        "ru" => include_bytes!("../hyphenation/ru.bin"),
        "uk" => include_bytes!("../hyphenation/uk.bin"),
        "tr" => include_bytes!("../hyphenation/tr.bin"),
        "el" => include_bytes!("../hyphenation/el.bin"),
        "ca" => include_bytes!("../hyphenation/ca.bin"),
        _ => return None,
    })
}

#[cfg(not(feature = "hyphenation"))]
fn compiled(_: &str) -> Option<&'static [u8]> {
    None
}

/// What gives a trie that is not compiled in: given a language's code, its file's bytes.
type Loader = Box<dyn Fn(&str) -> Option<Vec<u8>>>;

thread_local! {
    /// The tries handed over, by language.
    static HANDED: RefCell<BTreeMap<&'static str, Rc<[u8]>>> = RefCell::default();
    static LOADER: RefCell<Option<Loader>> = RefCell::default();
}

/// Where this thread's engines get a language's trie that is not compiled in, the first time a
/// text hyphenates in it: `loader`, given the language's code (`de`), gives the bytes of its
/// file (`hyphenation/de.bin`), or none. What it gives is held to the file's SHA-256.
pub fn set_loader(loader: impl Fn(&str) -> Option<Vec<u8>> + 'static) {
    LOADER.with(|l| *l.borrow_mut() = Some(Box::new(loader)));
}

/// `bytes` as language `lang`'s trie: refused unless they are its file, byte for byte.
fn checked(lang: &'static Language, bytes: Vec<u8>) -> Result<Rc<[u8]>, EngineError> {
    let digest = Sha256::digest(&bytes);
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    if hex != lang.digest {
        return Err(EngineError::Hyphenation(format!(
            "`{}`: what was handed over ({} bytes) is not the engine's trie for it",
            lang.code,
            bytes.len()
        )));
    }
    Ok(bytes.into())
}

/// Language `lang`'s trie, as the engine walks it.
pub(crate) enum Trie {
    Compiled(&'static [u8]),
    Handed(Rc<[u8]>),
}

impl Trie {
    fn bytes(&self) -> &[u8] {
        match self {
            Trie::Compiled(bytes) => bytes,
            Trie::Handed(bytes) => bytes,
        }
    }

    /// The byte offsets in `word` at which a hyphen may break it, in order, as `hypher`'s
    /// `hyphenate` finds them.
    pub(crate) fn breaks(&self, word: &str, lang: &Language) -> Vec<usize> {
        let data = self.bytes();
        let root = State::root(data);
        // The word in lower case, between dots: patterns match at its edges by the dots.
        let dotted = lowercase_and_dot(word);
        let (min, max) = char_to_byte_bounds(word, lang.left, lang.right);
        // The level between each two bytes of the word.
        let mut levels = vec![0u8; word.len().saturating_sub(1)];
        for start in 0..dotted.len() {
            if !is_char_boundary(dotted[start]) {
                continue;
            }
            let mut state = root;
            for &b in &dotted[start..] {
                let Some(next) = state.transition(b) else { break };
                state = next;
                for (offset, level) in state.levels() {
                    let split = start + offset;
                    if split >= min.max(2) && split <= max {
                        let slot = &mut levels[split - 2];
                        *slot = (*slot).max(level);
                    }
                }
            }
        }
        // A break at each odd level.
        (levels.iter().enumerate()).filter(|(_, level)| *level % 2 == 1).map(|(i, _)| i + 1).collect()
    }
}

/// Language `lang`'s trie: compiled in, or handed over. One that cannot be had is an error that
/// says so.
pub(crate) fn trie(lang: &'static Language) -> Result<Trie, EngineError> {
    match compiled(lang.code) {
        Some(bytes) => Ok(Trie::Compiled(bytes)),
        None => handed(lang).map(Trie::Handed),
    }
}

/// Language `lang`'s trie as this thread's loader hands it over: the first time, held to its
/// digest, and kept.
fn handed(lang: &'static Language) -> Result<Rc<[u8]>, EngineError> {
    if let Some(bytes) = HANDED.with(|h| h.borrow().get(lang.code).cloned()) {
        return Ok(bytes);
    }
    let loaded = LOADER.with(|l| l.borrow().as_ref().map(|load| load(lang.code)));
    let bytes = match loaded {
        Some(Some(bytes)) => checked(lang, bytes)?,
        Some(None) => {
            return Err(EngineError::Hyphenation(format!("`{}`: its patterns could not be loaded", lang.code)));
        }
        None => {
            return Err(EngineError::Hyphenation(format!(
                "`{}`: this engine has no patterns compiled in, and none were handed over",
                lang.code
            )));
        }
    };
    HANDED.with(|h| h.borrow_mut().insert(lang.code, Rc::clone(&bytes)));
    Ok(bytes)
}

/// The word in lower case, between dots. A character whose lower case is another length in
/// UTF-8, or more than one character, stays as it is.
fn lowercase_and_dot(word: &str) -> Vec<u8> {
    let mut dotted = Vec::with_capacity(word.len() + 2);
    dotted.push(b'.');
    let mut buf = [0u8; 4];
    for c in word.chars() {
        let mut lower = c.to_lowercase();
        let c = match (lower.next(), lower.next()) {
            (Some(l), None) if l.len_utf8() == c.len_utf8() => l,
            _ => c,
        };
        dotted.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
    }
    dotted.push(b'.');
    dotted
}

/// The first and last byte offsets in the dotted word a break may fall at: `left` characters
/// from the start, and `right` from the end.
fn char_to_byte_bounds(word: &str, left: usize, right: usize) -> (usize, usize) {
    let (left, right) = (left.max(1), right.max(1));
    let min = 1 + word.chars().take(left).map(char::len_utf8).sum::<usize>();
    let max = 1 + word.len() - word.chars().rev().take(right).map(char::len_utf8).sum::<usize>();
    (min, max)
}

fn is_char_boundary(b: u8) -> bool {
    (b as i8) >= -0x40
}

/// A state of a walk through a trie.
#[derive(Copy, Clone)]
struct State<'a> {
    data: &'a [u8],
    addr: usize,
    stride: usize,
    levels: &'a [u8],
    trans: &'a [u8],
    targets: &'a [u8],
}

impl<'a> State<'a> {
    fn root(data: &'a [u8]) -> Self {
        let addr = u32::from_be_bytes([data[0], data[1], data[2], data[3]]) as usize;
        Self::at(data, addr)
    }

    fn at(data: &'a [u8], addr: usize) -> Self {
        let node = &data[addr..];
        let mut pos = 0;
        // Whether the state has levels, how wide its targets are, and how many transitions.
        let has_levels = node[pos] >> 7 != 0;
        let stride = usize::from((node[pos] >> 5) & 3);
        let mut count = usize::from(node[pos] & 31);
        pos += 1;
        if count == 31 {
            count = usize::from(node[pos]);
            pos += 1;
        }
        let mut levels: &[u8] = &[];
        if has_levels {
            let offset = (usize::from(node[pos]) << 4) | (usize::from(node[pos + 1]) >> 4);
            let len = usize::from(node[pos + 1] & 15);
            levels = &data[offset..offset + len];
            pos += 2;
        }
        let trans = &node[pos..pos + count];
        pos += count;
        let targets = &node[pos..pos + stride * count];
        Self { data, addr, stride, levels, trans, targets }
    }

    fn transition(self, b: u8) -> Option<Self> {
        self.trans.iter().position(|&x| x == b).map(|i| {
            let target = &self.targets[self.stride * i..self.stride * (i + 1)];
            let next = (self.addr as isize + delta(target)) as usize;
            Self::at(self.data, next)
        })
    }

    /// Each level the state holds, at its offset.
    fn levels(self) -> impl Iterator<Item = (usize, u8)> + 'a {
        let mut offset = 0;
        self.levels.iter().map(move |&packed| {
            offset += usize::from(packed / 10);
            (offset, packed % 10)
        })
    }
}

/// A signed distance of 1, 2, or 3 bytes, big-endian.
fn delta(buf: &[u8]) -> isize {
    match *buf {
        [a] => a as i8 as isize,
        [a, b] => i16::from_be_bytes([a, b]) as isize,
        [a, b, c] => ((usize::from(a) << 16) | (usize::from(b) << 8) | usize::from(c)) as isize - (1 << 23),
        _ => unreachable!("a trie whose digest checked has strides of 1 to 3"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Each language's file, as a page hands it over.
    fn file(code: &str) -> Option<Vec<u8>> {
        std::fs::read(format!("{}/hyphenation/{code}.bin", env!("CARGO_MANIFEST_DIR"))).ok()
    }

    /// Each word, hyphenated at its breaks with `-`, by the trie compiled in or handed over.
    fn hyphenated(word: &str, trie: &Trie, lang: &Language) -> String {
        let mut out = String::new();
        let mut last = 0;
        for at in trie.breaks(word, lang) {
            out.push_str(&word[last..at]);
            out.push('-');
            last = at;
        }
        out + &word[last..]
    }

    /// The walk is `hypher`'s: every language breaks every word where `hypher` 0.1.8 does,
    /// from the same tries, compiled in or handed over.
    #[test]
    fn every_language_breaks_words_where_hypher_does() {
        let words = [
            "extensive",
            "hyphenation",
            "Silbentrennungsalgorithmus",
            "Donaudampfschifffahrtsgesellschaft",
            "anticonstitutionnellement",
            "electroencefalografista",
            "precipitevolissimevolmente",
            "inconstitucionalissimamente",
            "ongelooflijk",
            "realisationsvinster",
            "uafhængighedserklæringen",
            "epäjärjestelmällistyttämättömyydellänsäkäänköhän",
            "Konstantynopolitańczykowianeczka",
            "nejneobhospodařovávatelnějšími",
            "достопримечательности",
            "чотирнадцятиповерховий",
            "Muvaffakiyetsizleştiricileştiriveremeyebileceklerimizdenmişsinizcesine",
            "διαμερίσματα",
            "anticonstitucionalment",
            "a",
            "",
            "ÉLECTROENCÉPHALOGRAMME",
        ];
        let langs = [
            ("en", hypher::Lang::English),
            ("de", hypher::Lang::German),
            ("fr", hypher::Lang::French),
            ("es", hypher::Lang::Spanish),
            ("it", hypher::Lang::Italian),
            ("pt", hypher::Lang::Portuguese),
            ("nl", hypher::Lang::Dutch),
            ("sv", hypher::Lang::Swedish),
            ("da", hypher::Lang::Danish),
            ("fi", hypher::Lang::Finnish),
            ("pl", hypher::Lang::Polish),
            ("cs", hypher::Lang::Czech),
            ("ru", hypher::Lang::Russian),
            ("uk", hypher::Lang::Ukrainian),
            ("tr", hypher::Lang::Turkish),
            ("el", hypher::Lang::Greek),
            ("ca", hypher::Lang::Catalan),
        ];
        assert_eq!(langs.len(), LANGUAGES.len());
        set_loader(file);
        for (code, theirs) in langs {
            let lang = language(code).unwrap();
            assert_eq!(theirs.bounds(), (lang.left, lang.right), "{code}");
            let tries = [trie(lang).unwrap(), Trie::Handed(handed(lang).unwrap())];
            for word in words {
                let expected = hypher::hyphenate(word, theirs).join("-");
                for trie in &tries {
                    assert_eq!(hyphenated(word, trie, lang), expected, "{code}: {word}");
                }
            }
        }
    }

    #[test]
    fn a_tag_names_its_language_by_its_language_subtag() {
        assert_eq!(language("de-CH").map(|l| l.code), Some("de"));
        assert_eq!(language("EN_us").map(|l| l.code), Some("en"));
        assert!(language("ja").is_none() && language("").is_none() && language("deu").is_none());
    }

    /// A language's trie is loaded once a thread, and kept.
    #[test]
    fn a_trie_handed_over_is_loaded_once() {
        let asked = Rc::new(RefCell::new(Vec::new()));
        let log = Rc::clone(&asked);
        set_loader(move |code| {
            log.borrow_mut().push(code.to_string());
            file(code)
        });
        let sv = language("sv").unwrap();
        let first = handed(sv).unwrap();
        let again = handed(sv).unwrap();
        assert!(Rc::ptr_eq(&first, &again));
        assert_eq!(&first[..], &file("sv").unwrap()[..]);
        assert_eq!(*asked.borrow(), ["sv"]);
    }

    /// A trie that cannot be had is an error that says why: no loader, a loader that has none,
    /// or bytes that are not the language's file.
    #[test]
    fn a_trie_that_cannot_be_had_says_why() {
        let fi = language("fi").unwrap();
        // A thread of its own, which has no loader.
        let none = std::thread::spawn(move || handed(fi).err().unwrap().to_string()).join().unwrap();
        assert!(none.contains("`fi`") && none.contains("none were handed over"), "{none}");
        set_loader(|_| None);
        let lost = handed(fi).err().unwrap().to_string();
        assert!(lost.contains("`fi`: its patterns could not be loaded"), "{lost}");
        set_loader(|_| Some(b"<!doctype html>".to_vec()));
        let wrong = handed(fi).err().unwrap().to_string();
        assert!(wrong.contains("`fi`") && wrong.contains("15 bytes"), "{wrong}");
        // Each language's file is its trie.
        for lang in &LANGUAGES {
            assert!(checked(lang, file(lang.code).unwrap()).is_ok(), "{}", lang.code);
        }
    }
}
