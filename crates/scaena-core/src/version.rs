//! Which formats this build reads (SPEC §3.1). Each deck format since
//! [`crate::OLDEST_FORMAT_VERSION`], and each theme format since
//! [`crate::OLDEST_THEME_FORMAT_VERSION`], has only added to what the file may say, so a deck
//! or a theme saved in one of them is the same document in the current format: it reads as the
//! current one, and is written in it at the next save. One saved in a newer format, or in one
//! older than the oldest, is not read: validation says so (E106), and an editor does not open it.

use serde_json::Value;

/// A kind of file's formats: what it is called, the oldest format this build reads, and the
/// current one, which it writes.
#[derive(Debug, Clone, Copy)]
pub struct Formats {
    pub what: &'static str,
    pub oldest: &'static str,
    pub current: &'static str,
}

/// A deck's formats: its `scaena` key.
pub const DECK: Formats =
    Formats { what: "deck", oldest: crate::OLDEST_FORMAT_VERSION, current: crate::FORMAT_VERSION };

/// A theme's formats: its `scaena-theme` key.
pub const THEME: Formats =
    Formats { what: "theme", oldest: crate::OLDEST_THEME_FORMAT_VERSION, current: crate::THEME_FORMAT_VERSION };

impl Formats {
    /// `version` as this build reads it: the current format where `version` is an older one
    /// this build reads, and `version` as written otherwise.
    pub fn read<'a>(&self, version: &'a str) -> &'a str {
        let older = match (parse(version), parse(self.oldest), parse(self.current)) {
            (Some(saved), Some(oldest), Some(current)) => saved.0 == current.0 && oldest <= saved && saved < current,
            _ => false,
        };
        if older { self.current } else { version }
    }

    /// Why this build does not read a file saved in `version`, where it does not.
    pub fn unread(&self, version: &str) -> Option<String> {
        let (what, oldest, current) = (self.what, self.oldest, self.current);
        let (Some(saved), Some(first), Some(now)) = (parse(version), parse(oldest), parse(current)) else {
            return Some(format!(
                "{what} format `{version}` is not a version this build reads ({oldest} to {current})"
            ));
        };
        if saved > now {
            Some(format!(
                "{what} format {version} is newer than this build reads ({oldest} to {current}): open it in a newer Scaena"
            ))
        } else if saved < first || saved.0 != now.0 {
            Some(format!("{what} format {version} is older than this build reads ({oldest} to {current})"))
        } else {
            None
        }
    }

    /// What this build reads, as validation hints it where a file says another format.
    pub fn hint(&self) -> String {
        format!("This build reads {} formats {} to {}.", self.what, self.oldest, self.current)
    }

    /// `doc`'s `key`, its format version, as this build reads it: what validation checks, so an
    /// older format this build reads is checked as the current one.
    pub(crate) fn as_read(&self, doc: &mut Value, key: &str) {
        if let Some(Value::String(version)) = doc.get_mut(key) {
            let read = self.read(version).to_string();
            *version = read;
        }
    }
}

/// The deck format whose `shadow` first drew (PLAN 2.115). Before it a node's `shadow` was any
/// object and drew nothing, so a deck saved in an older format reads without one, and draws as
/// it did: what a shadow written out may say narrowed in the format that drew it.
const SHADOWS: &str = "0.25";

/// Whether a deck saved in `version` reads as the current format with its `shadow`s taken away:
/// one saved before a shadow drew, in a format this build reads.
pub(crate) fn unshadows(version: &str) -> bool {
    DECK.read(version) != version && parse(version) < parse(SHADOWS)
}

/// The deck format in which a text that shrinks is held under its own `maxSize`. Before it a
/// `maxSize` meant something only to a text that grows, and a generated deck's set of texts side
/// by side could carry one on a text a fix had made shrink: a deck saved in an older format reads
/// without it there, and draws as it did.
const CAPPED: &str = "0.36";

/// Whether a deck saved in `version` reads with the `maxSize` of each text that shrinks taken
/// away: one saved before a shrinking text was held under it, in a format this build reads.
pub(crate) fn uncaps(version: &str) -> bool {
    DECK.read(version) != version && parse(version) < parse(CAPPED)
}

/// The deck format in which code stands on a card unless it says otherwise (PLAN 2.136). Before
/// it, code stood on the slide: a deck saved in an older format reads with each code text's card
/// off, and draws as it did.
const CARDS: &str = "0.39";

/// Whether a deck saved in `version` reads with each code text's `card` off where it says none:
/// one saved before code stood on a card, in a format this build reads.
pub(crate) fn uncards(version: &str) -> bool {
    DECK.read(version) != version && parse(version) < parse(CARDS)
}

/// `doc`'s code texts with their `card` off where they say none: on each node, in each state's
/// deltas, and in the overrides.
fn uncard(doc: &mut Value) {
    let off = |props: Option<&mut Value>| {
        for props in props.and_then(Value::as_object_mut).into_iter().flat_map(|m| m.values_mut()) {
            if let Some(code) = props.get_mut("code").and_then(Value::as_object_mut) {
                code.entry("card").or_insert(Value::Bool(false));
            }
        }
    };
    off(doc.get_mut("nodes"));
    off(doc.get_mut("overrides"));
    for state in doc.get_mut("states").and_then(Value::as_array_mut).into_iter().flatten() {
        off(state.get_mut("props"));
    }
}

/// `doc`, a deck as saved, as this build reads it: its format the current one where it is an
/// older one this build reads; its `shadow`s taken away where it was saved before they drew, on
/// each node, in each state's deltas, and in the overrides; and the `maxSize` of each text whose
/// own `fit` is `shrink` taken away where it was saved before that held it; and each code text's
/// card off where it was saved before code stood on one. What validation checks and the source
/// compiles.
pub(crate) fn deck_as_read(doc: &mut Value) {
    let version = doc.get("scaena").and_then(Value::as_str).unwrap_or_default().to_string();
    let (unshadowed, uncapped, uncarded) = (unshadows(&version), uncaps(&version), uncards(&version));
    DECK.as_read(doc, "scaena");
    if uncapped {
        uncap(doc);
    }
    if uncarded {
        uncard(doc);
    }
    if !unshadowed {
        return;
    }
    let take = |props: Option<&mut Value>| {
        for props in props.and_then(Value::as_object_mut).into_iter().flat_map(|m| m.values_mut()) {
            if let Some(props) = props.as_object_mut() {
                props.remove("shadow");
            }
        }
    };
    take(doc.get_mut("nodes"));
    take(doc.get_mut("overrides"));
    for state in doc.get_mut("states").and_then(Value::as_array_mut).into_iter().flatten() {
        take(state.get_mut("props"));
    }
}

/// `doc` without the `maxSize` of each node whose own `fit`, on the node or in a state's deltas,
/// is `shrink` and never `grow`: on the node, in each state's deltas, and in the overrides.
fn uncap(doc: &mut Value) {
    let fits = |props: Option<&Value>| -> Vec<(String, String)> {
        (props.and_then(Value::as_object).into_iter().flatten())
            .filter_map(|(id, p)| Some((id.clone(), p.get("fit")?.as_str()?.to_string())))
            .collect()
    };
    let mut said = fits(doc.get("nodes"));
    for state in doc.get("states").and_then(Value::as_array).into_iter().flatten() {
        said.extend(fits(state.get("props")));
    }
    let shrinks = |id: &str| {
        let fit: Vec<&str> = said.iter().filter(|(of, _)| of == id).map(|(_, f)| f.as_str()).collect();
        fit.contains(&"shrink") && !fit.contains(&"grow")
    };
    let take = |props: Option<&mut Value>| {
        for (id, props) in props.and_then(Value::as_object_mut).into_iter().flat_map(|m| m.iter_mut()) {
            if shrinks(id)
                && let Some(props) = props.as_object_mut()
            {
                props.remove("maxSize");
            }
        }
    };
    take(doc.get_mut("nodes"));
    take(doc.get_mut("overrides"));
    for state in doc.get_mut("states").and_then(Value::as_array_mut).into_iter().flatten() {
        take(state.get_mut("props"));
    }
}

/// `major.minor[.patch]`'s major and minor.
fn parse(version: &str) -> Option<(u32, u32)> {
    fn number(part: &str) -> Option<u32> {
        if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        part.parse().ok()
    }
    let mut parts = version.split('.');
    let major = number(parts.next()?)?;
    let minor = number(parts.next()?)?;
    if let Some(patch) = parts.next() {
        number(patch)?;
    }
    parts.next().is_none().then_some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_older_format_this_build_reads_reads_as_the_current_one() {
        assert_eq!(DECK.read("0.15"), crate::FORMAT_VERSION);
        assert_eq!(DECK.read(crate::OLDEST_FORMAT_VERSION), crate::FORMAT_VERSION);
        assert_eq!(DECK.read("0.15.2"), crate::FORMAT_VERSION);
        assert_eq!(THEME.read("0.9"), crate::THEME_FORMAT_VERSION);
        assert_eq!(DECK.unread("0.15"), None);
        assert_eq!(THEME.unread("0.5"), None);
    }

    #[test]
    fn the_current_format_reads_as_written() {
        assert_eq!(DECK.read(crate::FORMAT_VERSION), crate::FORMAT_VERSION);
        assert_eq!(DECK.read("0.40.1"), "0.40.1");
        assert_eq!(DECK.unread(crate::FORMAT_VERSION), None);
        assert_eq!(THEME.unread(crate::THEME_FORMAT_VERSION), None);
    }

    #[test]
    fn a_newer_format_an_older_one_or_no_version_is_not_read_and_says_why() {
        let reads = format!("({} to {})", crate::OLDEST_FORMAT_VERSION, crate::FORMAT_VERSION);
        for (version, why) in
            [("0.99", "newer than"), ("1.0", "newer than"), ("0.3", "older than"), ("0.1.4", "older than")]
        {
            assert_eq!(DECK.read(version), version);
            let said = DECK.unread(version).unwrap();
            assert!(said.starts_with(&format!("deck format {version} is {why} this build reads {reads}")), "{said}");
        }
        assert!(DECK.unread("0.99").unwrap().ends_with(": open it in a newer Scaena"));
        assert_eq!(THEME.read("0.4"), "0.4");
        assert!(THEME.unread("0.4").unwrap().starts_with("theme format 0.4 is older than this build reads"));
        for version in ["", "0", "0.", ".15", "0.15.", "0.15.1.2", "0.x", "v0.15", "+0.15", "0.+15"] {
            assert_eq!(DECK.read(version), version);
            let said = DECK.unread(version).unwrap();
            assert_eq!(said, format!("deck format `{version}` is not a version this build reads {reads}"));
        }
    }

    #[test]
    fn validation_checks_an_older_format_as_the_current_one() {
        let mut doc = serde_json::json!({ "scaena": "0.12", "canvas": {} });
        DECK.as_read(&mut doc, "scaena");
        assert_eq!(doc["scaena"], crate::FORMAT_VERSION);
        let mut newer = serde_json::json!({ "scaena": "0.99" });
        DECK.as_read(&mut newer, "scaena");
        assert_eq!(newer["scaena"], "0.99");
        let mut number = serde_json::json!({ "scaena": 15 });
        DECK.as_read(&mut number, "scaena");
        assert_eq!(number["scaena"], 15);
    }

    #[test]
    fn a_deck_saved_before_shadows_drew_reads_without_one() {
        let saved = |version: &str| {
            serde_json::json!({
                "scaena": version,
                "nodes": { "card": { "type": "shape", "shadow": { "y": 8, "spread": 4 } } },
                "states": [{ "id": "one", "props": { "card": { "shadow": { "blur": 20 } } } }],
                "overrides": { "card": { "shadow": { "opacity": 1.5 } } }
            })
        };
        let mut old = saved("0.24");
        deck_as_read(&mut old);
        assert_eq!(old["scaena"], crate::FORMAT_VERSION);
        assert_eq!(old["nodes"]["card"], serde_json::json!({ "type": "shape" }));
        assert_eq!(old["states"][0]["props"]["card"], serde_json::json!({}));
        assert_eq!(old["overrides"]["card"], serde_json::json!({}));
        // From the format that drew them on, a shadow is read as written: validation says what
        // is wrong with one.
        let mut drawn = saved("0.25");
        deck_as_read(&mut drawn);
        assert_eq!(drawn["nodes"]["card"]["shadow"], serde_json::json!({ "y": 8, "spread": 4 }));
        assert!(!unshadows(crate::FORMAT_VERSION) && !unshadows("0.3") && unshadows(crate::OLDEST_FORMAT_VERSION));
    }

    #[test]
    fn a_deck_saved_before_a_shrinking_text_was_capped_reads_without_its_cap() {
        let saved = |version: &str| {
            serde_json::json!({
                "scaena": version,
                "nodes": {
                    "shrinks": { "type": "text", "fit": "shrink", "maxSize": 22 },
                    "grows": { "type": "text", "fit": "grow", "maxSize": 40 },
                    "slot": { "type": "text", "maxSize": 30 },
                    "fixed": { "type": "text", "fit": "grow", "maxSize": 28 }
                },
                "states": [{ "id": "one", "props": {
                    "shrinks": { "maxSize": 20 },
                    "fixed": { "fit": "shrink" }
                } }],
                "overrides": { "shrinks": { "maxSize": 18 } }
            })
        };
        let mut old = saved("0.35");
        deck_as_read(&mut old);
        assert_eq!(old["scaena"], crate::FORMAT_VERSION);
        assert_eq!(old["nodes"]["shrinks"], serde_json::json!({ "type": "text", "fit": "shrink" }));
        assert_eq!(old["states"][0]["props"]["shrinks"], serde_json::json!({}));
        assert_eq!(old["overrides"]["shrinks"], serde_json::json!({}));
        // A text that grows, one whose fit is its slot's, and one that grows somewhere keep theirs.
        assert_eq!(old["nodes"]["grows"]["maxSize"], 40);
        assert_eq!(old["nodes"]["slot"]["maxSize"], 30);
        assert_eq!(old["nodes"]["fixed"]["maxSize"], 28);
        // From the format that holds a shrinking text under it, its maxSize is read as written.
        let mut capped = saved(CAPPED);
        deck_as_read(&mut capped);
        assert_eq!(capped["nodes"]["shrinks"]["maxSize"], 22);
        assert!(!uncaps(crate::FORMAT_VERSION) && uncaps("0.35") && uncaps(crate::OLDEST_FORMAT_VERSION));
    }
}
