//! A deck made from a prompt, made a bundle (PLAN G.5, ADR-0026): the second half of
//! `scaena generate`, which the browser's editor module and the Mac's C ABI run. Narrato's calls,
//! the sources, and the deck a narrative builds are the generator's (`scaena-generator` in the
//! browser), so the editor's module carries none of Narrato; this takes the deck it built and
//! makes the bundle as the CLI does: in the theme with its fonts, the characters the fonts lack
//! written plainly, and lint's fixes applied; then the texts that still run past their place
//! rewritten by the model, which the client asks, and each sheet given kept as a data source.

use crate::Error;
use scaena_ops::OpsError;
use scaena_ops::lint::Write;
use scaena_ops::narrative::{Generated, Kept, Made, fitted, keeping, making, overflowing};
use scaena_sources::Table;
use std::collections::BTreeMap;
use std::path::Path;

/// A bundle being made from a deck a narrative built.
pub struct Making {
    made: Generated,
    write: Option<Write>,
    kept: Vec<Kept>,
}

fn ops(e: OpsError) -> Error {
    Error::Ops(e.to_string())
}

impl Making {
    /// The bundle `built` makes (`scaena_ops::narrative::Made`: the deck a narrative on the arc
    /// `arc` built, and each slide's layout): in the theme file named `file` (`dusk.theme.json`),
    /// whose text is `theme`, each font it names from `fonts` by the path it gives it, as
    /// `scaena generate` makes it. `fallback` says why the heuristic chose the arc, where it did.
    pub fn new(
        built: Made,
        arc: &str,
        fallback: Option<String>,
        file: &str,
        theme: &str,
        fonts: &BTreeMap<String, Vec<u8>>,
    ) -> Result<Making, Error> {
        let font = |path: &str| -> Result<Vec<u8>, OpsError> {
            fonts
                .get(path)
                .cloned()
                .ok_or_else(|| OpsError::new(format!("the theme's font `{path}` was not handed over")))
        };
        let (mut made, write) = making(built, arc, Path::new(file), theme.into(), &font).map_err(ops)?;
        made.fallback = fallback;
        Ok(Making { made, write, kept: Vec::new() })
    }

    /// The texts that still run past their place, which the model is asked to rewrite to fit
    /// (`scaena_narrato::fit`); none where the deck would not validate.
    pub fn longs(&self) -> Result<Vec<scaena_narrato::fit::Long>, Error> {
        let Some(write) = &self.write else { return Ok(Vec::new()) };
        overflowing(&write.deck, &self.made.created.findings).map_err(ops)
    }

    /// The model's rewrites, each a text's id and its words, set in their places, and lint's
    /// fixes applied again.
    pub fn fitted(&mut self, rewrites: Vec<(String, String)>) -> Result<(), Error> {
        let Some(write) = self.write.take() else { return Ok(()) };
        self.write = Some(fitted(&mut self.made, write, rewrites).map_err(ops)?);
        Ok(())
    }

    /// The rewrite failed: why, which the deck made says.
    pub fn unfitted(&mut self, why: String) {
        self.made.unfitted = Some(why);
    }

    /// How many pictures were made for its slides, and why each other asked for was not
    /// (PLAN G.10), which the deck made says.
    pub fn painted(&mut self, pictures: usize, unmade: Vec<String>) {
        self.made.pictures = pictures;
        self.made.unpictured = unmade;
    }

    /// Each sheet given kept as a data source of the deck's, as `data_attach` keeps a file.
    pub fn keep(&mut self, tables: &[Table]) -> Result<(), Error> {
        let Some(write) = self.write.take() else { return Ok(()) };
        let (write, kept) = keeping(write, tables).map_err(ops)?;
        self.write = Some(write);
        self.kept.extend(kept);
        Ok(())
    }

    /// What was made, as `scaena generate --json` says it: the bundle made, its arc, each
    /// slide's layout, what lint fixed and finds, the texts rewritten to fit, and, as `data`,
    /// each data source kept.
    pub fn report(&self) -> serde_json::Value {
        let mut out = serde_json::to_value(&self.made).expect("what was made is JSON");
        out["data"] = serde_json::to_value(&self.kept).expect("what was kept is JSON");
        out
    }

    /// The bundle's files, by their paths in it, `deck.json` among them; none where the deck
    /// would not validate, which [`Making::report`] says.
    pub fn files(&self) -> Result<Option<BTreeMap<String, Vec<u8>>>, Error> {
        let Some(write) = &self.write else { return Ok(None) };
        let mut files = write.files.clone();
        let deck = write.deck.to_json().map_err(|e| Error::Deck(e.to_string()))?;
        files.insert("deck.json".into(), (deck + "\n").into_bytes());
        Ok(Some(files))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn root() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    /// Dusk, and the fonts it names, by the paths it gives them, where the repository keeps them.
    fn dusk() -> (String, BTreeMap<String, Vec<u8>>) {
        let dir = root().join("docs/examples");
        let theme = std::fs::read_to_string(dir.join("themes/dusk.theme.json")).unwrap();
        let named: serde_json::Value = serde_json::from_str(&theme).unwrap();
        let mut fonts = BTreeMap::new();
        for family in named["type"]["families"].as_object().unwrap().values() {
            for file in [&family["file"], &family["italic"]["file"]].into_iter().filter_map(|f| f.as_str()) {
                fonts.insert(file.to_string(), std::fs::read(dir.join(file)).unwrap());
            }
        }
        (theme, fonts)
    }

    /// The deck a narrative of three slides builds, as the generator's module hands it over.
    fn built() -> Made {
        let deck = serde_json::json!({
            "scaena": scaena_core::FORMAT_VERSION,
            "meta": { "title": "Tidewater" },
            "canvas": { "width": 1920, "height": 1080 },
            "nodes": {
                "cover-title": { "type": "text", "role": "display", "text": "Tidewater Freight", "at": { "in": "title" } },
                "close-title": {
                    "type": "text",
                    "role": "display",
                    "text": "Freight emissions are still climbing across every corridor we run, every port, every lane, every quarter, and nothing we have tried so far has bent the curve",
                    "at": { "in": "title" }
                }
            },
            "states": [
                { "id": "cover", "mode": "absolute", "layout": "title", "props": { "cover-title": {} } },
                { "id": "close", "mode": "absolute", "layout": "title", "props": { "close-title": {} } }
            ]
        });
        Made { deck, layouts: vec!["title".into(), "title".into()], assets: Default::default() }
    }

    #[test]
    fn a_built_deck_is_made_a_bundle_as_scaena_generate_makes_it() {
        let (theme, fonts) = dusk();
        let mut m =
            Making::new(built(), "Problem-Solution", Some("no answer".into()), "dusk.theme.json", &theme, &fonts)
                .unwrap();
        let report = m.report();
        assert_eq!(report["arc"], "Problem-Solution");
        assert_eq!(report["fallback"], "no answer");
        assert_eq!(report["created"]["created"], true, "{report:#}");
        let longs = m.longs().unwrap();
        assert!(longs.iter().any(|l| l.id == "close-title"), "{longs:?}");
        m.fitted(vec![("close-title".into(), "Emissions keep climbing".into())]).unwrap();
        assert_eq!(m.report()["fitted"], serde_json::json!(["close-title"]));
        let table =
            Table { named: "q3.csv".into(), hint: "q3".into(), csv: Some(b"region,revenue\nGulf,4.1\n".to_vec()) };
        m.keep(&[table]).unwrap();
        assert_eq!(m.report()["data"][0]["id"], "q3");
        let files = m.files().unwrap().unwrap();
        let deck: serde_json::Value = serde_json::from_slice(&files["deck.json"]).unwrap();
        assert_eq!(deck["nodes"]["close-title"]["text"], "Emissions keep climbing");
        assert_eq!(deck["data"]["q3"]["source"], "data/q3.csv");
        assert!(files.contains_key("themes/dusk.theme.json") && files.contains_key("data/q3.csv"));
        assert!(files.keys().any(|f| f.starts_with("fonts/")), "{:?}", files.keys());
        // The bundle opens as a session.
        crate::Session::open(files).unwrap();
    }

    #[test]
    fn a_font_not_handed_over_says_which() {
        let (theme, _) = dusk();
        let e =
            Making::new(built(), "Problem-Solution", None, "dusk.theme.json", &theme, &BTreeMap::new()).err().unwrap();
        assert!(e.to_string().contains("was not handed over"), "{e}");
    }
}
