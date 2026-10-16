//! The font a deck takes for a script its theme's fonts do not draw (PLAN G.22, ADR-0027): the
//! script's Noto Sans, from google/fonts at the commit the shipped fonts come from, named by its
//! path and its SHA-256. The client fetches it, as it fetches a source; this says which a deck
//! needs and where it is, checks what was fetched, and gives the bundle's copy of the theme a
//! family for it, named last in each text family's fallbacks, so the theme's own faces set what
//! they draw and the script's font the rest.

use crate::{Context, OpsError};
use scaena_core::model::theme::Theme;
use serde::Serialize;
use serde_json::{Value, json};

/// The commit of google/fonts every font Scaena names comes from (`scripts/build_torture_fonts.py`).
pub const COMMIT: &str = "9710da1eacb3be272583c3224dcb70f9da6eadbb";

/// A script's font, as a deck written in it takes one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Script {
    /// The language written in it, by Narrato's name.
    pub language: &'static str,
    /// Its key among the theme's families.
    pub key: &'static str,
    /// The family its font provides.
    pub family: &'static str,
    /// Its path in a bundle.
    pub file: &'static str,
    /// Its path in google/fonts.
    pub path: &'static str,
    /// Its SHA-256, in hex.
    pub sha256: &'static str,
    /// How many bytes it is.
    pub bytes: usize,
    /// The least leading its lines take: what its marks above and below need to clear the
    /// lines beside them, where a theme's display roles are set close for Latin.
    pub leading: f64,
    /// Whether a role's tracking may space its letters apart: never where they join (Arabic) or
    /// stack (Devanagari). None is tracked closer than its font sets it.
    pub tracks: bool,
}

/// Each script a language Narrato writes in takes a font for, where a theme that ships draws
/// none of it.
pub const SCRIPTS: [Script; 5] = [
    Script {
        language: "Japanese",
        key: "japanese",
        family: "Noto Sans JP",
        file: "fonts/NotoSansJP-VF.ttf",
        path: "ofl/notosansjp/NotoSansJP[wght].ttf",
        sha256: "c2f3b4d463500a2ddcd3849cded1fceeb9fd6d1c32e6cbecd568453ba50fc68f",
        bytes: 9_589_900,
        leading: 1.3,
        tracks: true,
    },
    Script {
        language: "Chinese (Simplified)",
        key: "chinese",
        family: "Noto Sans SC",
        file: "fonts/NotoSansSC-VF.ttf",
        path: "ofl/notosanssc/NotoSansSC[wght].ttf",
        sha256: "a3041811a78c361b1de50f953c805e0244951c21c5bd412f7232ef0d899af0da",
        bytes: 17_772_300,
        leading: 1.3,
        tracks: true,
    },
    Script {
        language: "Korean",
        key: "korean",
        family: "Noto Sans KR",
        file: "fonts/NotoSansKR-VF.ttf",
        path: "ofl/notosanskr/NotoSansKR[wght].ttf",
        sha256: "194018e6b2b293a7964f037b25c0249ce1418bc9ab3c971060a03aa57861e252",
        bytes: 10_414_588,
        leading: 1.3,
        tracks: true,
    },
    Script {
        language: "Arabic",
        key: "arabic",
        family: "Noto Sans Arabic",
        file: "fonts/NotoSansArabic-VF.ttf",
        path: "ofl/notosansarabic/NotoSansArabic[wdth,wght].ttf",
        sha256: "63111b5b2e074dd48cc67692e0a2726d86ee94c1c37fe8598257b7b4e87e869e",
        bytes: 844_676,
        leading: 1.6,
        tracks: false,
    },
    Script {
        language: "Hindi",
        key: "devanagari",
        family: "Noto Sans Devanagari",
        file: "fonts/NotoSansDevanagari-VF.ttf",
        path: "ofl/notosansdevanagari/NotoSansDevanagari[wdth,wght].ttf",
        sha256: "14ec4af41f27482216d1c2229f417ff9b1425e1babb014e57d1d40d03229853e",
        bytes: 641_944,
        leading: 1.45,
        tracks: false,
    },
];

/// Where google/fonts serves its files at [`COMMIT`], to any origin.
pub const BASE: &str = "https://raw.githubusercontent.com/google/fonts/9710da1eacb3be272583c3224dcb70f9da6eadbb";

impl Script {
    /// Where it is fetched from: its path under `base`, else under [`BASE`].
    pub fn url(&self, base: Option<&str>) -> String {
        let path: String = (self.path.chars())
            .map(|c| match c {
                '[' => "%5B".to_string(),
                ']' => "%5D".to_string(),
                ',' => "%2C".to_string(),
                c => c.to_string(),
            })
            .collect();
        format!("{}/{path}", base.unwrap_or(BASE).trim_end_matches('/'))
    }

    /// The script whose family is `key` in a theme.
    pub fn keyed(key: &str) -> Option<&'static Script> {
        SCRIPTS.iter().find(|s| s.key == key)
    }

    /// Nothing, if `bytes` are its font; else why not, in a person's words.
    pub fn check(&self, bytes: &[u8]) -> Result<(), OpsError> {
        if scaena_store::sha256(bytes) == self.sha256 {
            return Ok(());
        }
        Err(OpsError::new(format!(
            "what was fetched for {} is not the font it names ({} bytes, where it is {}): fetch it again",
            self.family,
            bytes.len(),
            self.bytes
        )))
    }
}

/// The script `text` is written in, of those [`SCRIPTS`] holds: kana say Japanese, Hangul
/// Korean, and Han without kana Chinese; then Arabic, then Devanagari. None for any other.
pub fn written_in(text: &str) -> Option<&'static Script> {
    let has = |lo: u32, hi: u32| text.chars().any(|c| (lo..=hi).contains(&u32::from(c)));
    let key = if has(0x3040, 0x30FF) {
        "japanese"
    } else if has(0xAC00, 0xD7A3) || has(0x1100, 0x11FF) {
        "korean"
    } else if has(0x4E00, 0x9FFF) || has(0x3400, 0x4DBF) {
        "chinese"
    } else if has(0x600, 0x6FF) {
        "arabic"
    } else if has(0x900, 0x97F) {
        "devanagari"
    } else {
        return None;
    };
    Script::keyed(key)
}

/// `theme`, the JSON of a theme a deck moves to, with the family of each script whose font
/// the theme it had (`was`) took: a deck in Japanese is drawn in its letters in any theme it
/// moves to, as a re-theme keeps it valid (PLAN G.22).
pub fn carried(was: Option<&str>, theme: &str) -> Result<String, OpsError> {
    let Some(was) = was.and_then(|w| serde_json::from_str::<Value>(w).ok()) else { return Ok(theme.to_string()) };
    let mut text = theme.to_string();
    for script in &SCRIPTS {
        let file = was.pointer(&format!("/type/families/{}/file", script.key)).and_then(Value::as_str);
        if file == Some(script.file) {
            text = taken(&text, script)?;
        }
    }
    Ok(text)
}

/// The script a deck takes a font for beside `theme`'s fonts (read by `font`, by the path the
/// theme names each at): that of `language` where the brief names one, else that `text` is
/// written in (slides written elsewhere); none where the theme's fonts draw its letters. A
/// theme whose fonts do not draw a language in no script of [`SCRIPTS`] (a Latin or Cyrillic
/// one) is refused, as [`crate::narrative::writable`] refuses it.
#[cfg(feature = "narrato")]
pub fn needed(
    language: Option<&str>,
    text: &str,
    theme: &str,
    font: &dyn Fn(&str) -> Result<Vec<u8>, OpsError>,
) -> Result<Option<&'static Script>, OpsError> {
    let named = language.map(str::trim).filter(|l| !l.is_empty());
    let script = match named {
        Some(language) => SCRIPTS.iter().find(|s| s.language.eq_ignore_ascii_case(language)),
        None => written_in(text),
    };
    let Some(script) = script else {
        crate::narrative::writable(named, theme, false, font)?;
        return Ok(None);
    };
    Ok(crate::narrative::writable(Some(script.language), theme, false, font).err().map(|_| script))
}

/// `theme`, the JSON of a theme, with `script`'s family: its font at `script.file`, named last
/// in the fallbacks of the family of every role but the one code is set in (`Theme::code_role`,
/// ADR-0029). Taking it twice changes nothing.
pub fn taken(theme: &str, script: &Script) -> Result<String, OpsError> {
    let model: Theme = serde_json::from_str(theme).context("the theme")?;
    let code = model.code_role();
    let mut keys: Vec<String> = Vec::new();
    for (role, r) in &model.typography.roles {
        if Some(role.as_str()) != code && r.family != script.key && !keys.contains(&r.family) {
            keys.push(r.family.clone());
        }
    }
    let mut theme: Value = serde_json::from_str(theme).context("the theme")?;
    let families = (theme.pointer_mut("/type/families").and_then(Value::as_object_mut))
        .ok_or_else(|| OpsError::new("the theme names no families"))?;
    families.insert(
        script.key.into(),
        json!({ "family": script.family, "file": script.file, "axes": { "wght": [100, 900] } }),
    );
    for key in keys {
        let Some(family) = families.get_mut(&key).and_then(Value::as_object_mut) else { continue };
        let fallback = family.entry("fallback").or_insert_with(|| json!([]));
        if let Some(list) = fallback.as_array_mut()
            && !list.iter().any(|k| k == script.key)
        {
            list.push(json!(script.key));
        }
    }
    // Each text role's lines set far enough apart for the script's marks, a whole number of the
    // baseline grid's lines; tracked no closer than the font sets its letters, nor apart where
    // they join or stack; and fitted by its whole lines, since the script's letters and marks
    // reach past a Latin cap height and baseline.
    let baseline = theme.pointer("/grid/baseline").and_then(Value::as_f64).filter(|b| *b > 0.0);
    let roles = (theme.pointer_mut("/type/roles").and_then(Value::as_object_mut))
        .ok_or_else(|| OpsError::new("the theme names no roles"))?;
    for (_, role) in roles.iter_mut().filter(|(name, _)| Some(name.as_str()) != code) {
        let Some(role) = role.as_object_mut() else { continue };
        let size = role.get("size").and_then(Value::as_f64).unwrap_or(0.0);
        let leading = role.get("leading").and_then(Value::as_f64).unwrap_or(0.0);
        if size > 0.0 && leading < script.leading {
            let wanted = match baseline {
                Some(grid) => (size * script.leading / grid).ceil() * grid / size,
                None => script.leading,
            };
            // Four places, as a theme writes a leading: within the grid's float allowance.
            role.insert("leading".into(), json!((wanted * 1e4).round() / 1e4));
        }
        let tracking = role.get("tracking").and_then(Value::as_f64).unwrap_or(0.0);
        if tracking < 0.0 || (!script.tracks && tracking != 0.0) {
            role.insert("tracking".into(), json!(0));
        }
        role.insert("box".into(), json!("line"));
    }
    Ok(serde_json::to_string_pretty(&theme)?)
}

#[cfg(all(test, feature = "narrato"))]
mod tests {
    use super::*;
    use crate::narrative::LETTERS;
    use std::collections::BTreeMap;
    use std::path::Path;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn font(file: &str) -> Result<Vec<u8>, OpsError> {
        Ok(std::fs::read(root().join("docs/examples").join(file))?)
    }

    #[test]
    fn each_script_is_a_language_narrato_writes_in_that_no_shipped_theme_draws() {
        let dusk = std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap();
        for script in SCRIPTS {
            assert!(LETTERS.iter().any(|(l, _)| *l == script.language), "{}", script.language);
            assert_eq!(needed(Some(script.language), "", &dusk, &font).unwrap(), Some(&script));
            assert!(script.url(None).starts_with(&format!("{BASE}/ofl/")), "{}", script.url(None));
            assert!(BASE.ends_with(COMMIT));
            assert!(!script.url(None).contains('['), "{}", script.url(None));
            assert_eq!(
                script.url(Some("http://127.0.0.1:9/")),
                format!("http://127.0.0.1:9/{}", &script.url(None)[BASE.len() + 1..])
            );
        }
        for language in ["English", "Ukrainian", "Czech", ""] {
            assert_eq!(needed(Some(language), "", &dusk, &font).unwrap(), None, "{language}");
        }
        // Slides written elsewhere say their script where no language is named.
        for (text, key) in [
            ("気候変動の現状", "japanese"),
            ("气候变化", "chinese"),
            ("기후 변화", "korean"),
            ("تغير المناخ", "arabic"),
            ("जलवायु", "devanagari"),
        ] {
            assert_eq!(needed(None, text, &dusk, &font).unwrap().map(|s| s.key), Some(key), "{text}");
        }
        assert_eq!(needed(None, "Climate, Клімат, Κλίμα", &dusk, &font).unwrap(), None);
    }

    #[test]
    fn a_theme_takes_a_scripts_family_last_in_its_text_families_fallbacks() {
        let dusk = std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap();
        let japanese = &SCRIPTS[0];
        let taken = taken(&dusk, japanese).unwrap();
        let theme: Value = serde_json::from_str(&taken).unwrap();
        assert_eq!(theme["type"]["families"]["japanese"]["family"], "Noto Sans JP");
        assert_eq!(theme["type"]["families"]["display"]["fallback"], json!(["body", "japanese"]));
        assert_eq!(theme["type"]["families"]["body"]["fallback"], json!(["japanese"]));
        assert_eq!(theme["type"]["families"]["mono"].get("fallback"), None, "code keeps its own");
        assert_eq!(super::taken(&taken, japanese).unwrap(), taken, "taken twice, the same");
        // A theme that keeps another role for code (ADR-0029) keeps that role's family out: a role
        // named `code` that code is not set in is a text role as any other.
        let mut other: Value = serde_json::from_str(&dusk).unwrap();
        other["type"]["families"]["typewriter"] = other["type"]["families"]["mono"].clone();
        other["type"]["roles"]["source"] = other["type"]["roles"]["code"].clone();
        other["type"]["roles"]["source"]["family"] = json!("typewriter");
        other["code"] = json!({ "role": "source" });
        let theme: Value = serde_json::from_str(&super::taken(&other.to_string(), japanese).unwrap()).unwrap();
        assert_eq!(theme["type"]["families"]["typewriter"].get("fallback"), None, "code keeps its own");
        assert_eq!(theme["type"]["families"]["mono"]["fallback"], json!(["japanese"]), "`code` is no code role");
        assert_eq!(theme["type"]["roles"]["source"], other["type"]["roles"]["source"], "nor are its lines opened");
    }

    /// A deck in Japanese or Hindi takes its script's font (PLAN G.22): the theme's own faces set
    /// what they draw and the script's font the rest, every letter drawn and none written
    /// plainly; the deck lists the font and says its language. Noto Sans Devanagari is the very
    /// font a deck in Hindi fetches, its digest the one pinned here; Noto Sans JP is a stand-in
    /// subset to the slides' characters.
    /// The deck the slides of `slides` (in `tests/fixtures/scripts`) make in Dusk, written in
    /// `language`, with `script`'s font, the fixture `file`: the slides, and what was made.
    fn made_in(
        slides: &str,
        language: &str,
        file: &str,
        script: &Script,
    ) -> (scaena_narrato::answer::Slides, crate::narrative::Generated, crate::lint::Write) {
        let fixtures = root().join("tests/fixtures/scripts");
        let dusk = std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap();
        let bytes = std::fs::read(fixtures.join(file)).unwrap();
        let text = std::fs::read_to_string(fixtures.join(slides)).unwrap();
        assert_eq!(needed(Some(language), &text, &dusk, &font).unwrap(), Some(script));
        assert_eq!(needed(None, &text, &dusk, &font).unwrap(), Some(script), "its slides say their script");
        let theme = taken(&dusk, script).unwrap();
        let fonts = |f: &str| if f == script.file { Ok(bytes.clone()) } else { font(f) };
        crate::narrative::writable(Some(language), &theme, true, &fonts).unwrap();
        let written = scaena_narrato::answer::slides(&text).unwrap();
        let narrative = scaena_narrato::Narrative {
            arc: "Problem\u{2013}Solution".into(),
            arc_reasoning: String::new(),
            executive_summary: written.executive_summary.clone(),
            slides: written.slides.clone(),
            truncated: false,
            icons: Default::default(),
            brief: scaena_narrato::Brief {
                topic: written.slides[0].slide_title.clone(),
                language: Some(language.into()),
                ..scaena_narrato::Brief::default()
            },
        };
        let (made, write) =
            crate::narrative::generating(&narrative, Path::new("dusk.theme.json"), theme, &fonts).unwrap();
        let write = write.unwrap_or_else(|| panic!("{language}: {:#?}", made.created.findings));
        (written, made, write)
    }

    #[test]
    fn a_deck_in_another_script_takes_its_scripts_font() {
        let fixtures = root().join("tests/fixtures/scripts");
        for (slides, language, tag, file, script) in [
            ("japanese.json", "Japanese", "ja", "NotoSansJP-subset.ttf", Script::keyed("japanese").unwrap()),
            ("hindi.json", "Hindi", "hi", "NotoSansDevanagari-VF.ttf", Script::keyed("devanagari").unwrap()),
        ] {
            if language == "Hindi" {
                script.check(&std::fs::read(fixtures.join(file)).unwrap()).unwrap();
            }
            let (written, made, write) = made_in(slides, language, file, script);
            let unset: Vec<_> =
                (made.fixed.iter().chain(&made.created.findings)).filter(|f| f.code == "E120").collect();
            assert!(unset.is_empty(), "{language}: {unset:#?}");
            let deck = write.deck.to_value().unwrap();
            assert_eq!(deck["meta"]["lang"], tag);
            assert!(
                deck["fonts"].as_array().unwrap().iter().any(|f| f["file"] == script.file || f == script.file),
                "{}",
                deck["fonts"]
            );
            let said: String =
                (deck["nodes"].as_object().unwrap().values()).filter_map(|n| n["text"].as_str()).collect();
            for slide in &written.slides {
                assert!(said.contains(slide.slide_title.as_str()), "{language}: {:?} in {said}", slide.slide_title);
            }
            assert!(write.files.contains_key(script.file), "the bundle carries the script's font");
        }
    }

    /// A deck in Hindi re-themed keeps its script's font: the theme it moves to takes the
    /// script's family and is written with it, and the deck is drawn in its letters there, with
    /// nothing missing (PLAN G.22).
    #[test]
    fn a_re_theme_keeps_a_scripts_font() {
        let script = Script::keyed("devanagari").unwrap();
        let (_, _, write) = made_in("hindi.json", "Hindi", "NotoSansDevanagari-VF.ttf", script);
        let mut files = write.files.clone();
        files.insert("deck.json".into(), write.deck.to_json().unwrap().into_bytes());
        let b = scaena_store::Bundle::in_memory(files).unwrap();
        let ember = std::fs::read_to_string(root().join("docs/examples/themes/ember.theme.json")).unwrap();
        let (themed, rethemed) =
            crate::theme::theming(&b, "themes/ember.theme.json", &ember, &BTreeMap::new(), true, false, true).unwrap();
        assert!(themed.applied && !themed.refused, "{themed:#?}");
        assert!(themed.added.iter().all(|f| f.code != "E120"), "{:#?}", themed.added);
        let theme: Value = serde_json::from_slice(&rethemed.files["themes/ember.theme.json"]).unwrap();
        assert_eq!(theme["type"]["families"]["devanagari"]["file"], script.file);
        assert_eq!(theme["type"]["families"]["body"]["fallback"], json!(["devanagari"]));
        assert!(rethemed.deck.fonts.iter().any(|f| f.file == script.file), "the deck still lists the font");
    }

    /// A theme taking a script's font sets its roles' lines far enough apart for the script's
    /// marks, on the baseline grid where they keep to it, and tracks no closer than the font
    /// where the script's letters join or stack (PLAN G.22).
    #[test]
    fn a_scripts_lines_clear_its_marks_on_the_grid() {
        let dusk = std::fs::read_to_string(root().join("docs/examples/themes/dusk.theme.json")).unwrap();
        for script in SCRIPTS {
            let theme: Value = serde_json::from_str(&taken(&dusk, &script).unwrap()).unwrap();
            for (name, role) in theme["type"]["roles"].as_object().unwrap() {
                if name == "code" {
                    continue;
                }
                let (size, leading) = (role["size"].as_f64().unwrap(), role["leading"].as_f64().unwrap());
                assert!(leading >= script.leading - 1e-3, "{} {name}: {leading}", script.key);
                if role["snap"] == "baseline" {
                    let lines = size * leading / 8.0;
                    assert!((lines - lines.round()).abs() < 1e-3, "{} {name}: {lines} lines", script.key);
                }
                let tracking = role["tracking"].as_f64().unwrap_or(0.0);
                assert!(tracking >= 0.0 && (script.tracks || tracking == 0.0), "{} {name}: {tracking}", script.key);
                assert_eq!(role["box"], "line", "{} {name}", script.key);
            }
        }
    }

    #[test]
    fn a_font_fetched_is_checked_against_its_digest() {
        let e = SCRIPTS[3].check(b"not the font").unwrap_err();
        assert!(e.message.contains("is not the font it names"), "{e}");
    }
}
