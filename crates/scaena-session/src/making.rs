//! A deck made from a prompt, made a bundle (PLAN G.5, ADR-0026): the second half of
//! `scaena generate`, which the browser's editor module and the Mac's C ABI run. Narrato's calls,
//! the sources, and the deck a narrative builds are the generator's (`scaena-generator` in the
//! browser), so the editor's module carries none of Narrato; this takes the deck it built and
//! makes the bundle as the CLI does: in the theme with its fonts, the characters the fonts lack
//! written plainly, and lint's fixes applied; then the deck seen as it is drawn, each slide's
//! change the model asks kept where lint finds the slide no worse (PLAN G.13); then the texts
//! that still run past their place rewritten by the model, which the client asks, and each sheet
//! given kept as a data source.

use crate::Error;
use scaena_narrato::Picture;
use scaena_narrato::look::Change;
use scaena_ops::OpsError;
use scaena_ops::lint::Write;
use scaena_ops::narrative::{
    Generated, Kept, Made, Shown, adopt, fitted, keeping, kept, looked, making, overflowing, unkept, worse,
};
use scaena_sources::Table;
use std::collections::BTreeMap;
use std::path::Path;

/// A bundle being made from a deck a narrative built: what it came to, its files, the sheets
/// kept, and what it is made in, to make it again with what the model asked (PLAN G.13).
pub struct Making {
    made: Generated,
    write: Option<Write>,
    kept: Vec<Kept>,
    arc: String,
    file: String,
    theme: String,
    fonts: BTreeMap<String, Vec<u8>>,
}

/// What making a deck again with the model's changes came to (PLAN G.13).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Relooked {
    /// The changes were kept.
    Kept,
    /// Lint found these slides, by index, worse for them: nothing was kept.
    Worse(Vec<usize>),
}

fn ops(e: OpsError) -> Error {
    Error::Ops(e.to_string())
}

/// Each font `fonts` holds, by the path the theme gives it.
fn font(fonts: &BTreeMap<String, Vec<u8>>) -> impl Fn(&str) -> Result<Vec<u8>, OpsError> + '_ {
    |path: &str| {
        fonts.get(path).cloned().ok_or_else(|| OpsError::new(format!("the theme's font `{path}` was not handed over")))
    }
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
        let (mut made, write) = making(built, arc, Path::new(file), theme.into(), &font(fonts)).map_err(ops)?;
        made.fallback = fallback;
        Ok(Making {
            made,
            write,
            kept: Vec::new(),
            arc: arc.to_string(),
            file: file.to_string(),
            theme: theme.to_string(),
            fonts: fonts.clone(),
        })
    }

    /// The layout each slide took.
    pub fn layouts(&self) -> &[String] {
        &self.made.layouts
    }

    /// How many slides the deck has; none where it would not validate.
    pub fn count(&self) -> usize {
        self.write.as_ref().map_or(0, |w| w.deck.slides().len())
    }

    /// Each slide drawn `width` pixels wide, at rest, as a JPEG, for the model to see (PLAN
    /// G.13); none where the deck would not validate.
    pub fn drawn(&self, width: u32) -> Result<Vec<Picture>, Error> {
        let Some(write) = &self.write else { return Ok(Vec::new()) };
        scaena_ops::narrative::drawn(write, width).map_err(ops)
    }

    /// The `index`th slide, whole at its last state, drawn `width` pixels wide, at rest, by the
    /// CPU painter: what a page makes a JPEG of for the model to see (PLAN G.13).
    #[cfg(feature = "cpu")]
    pub fn pixels(&self, index: usize, width: u32) -> Result<scaena_paint::Raster, Error> {
        let write = self.write.as_ref().ok_or_else(|| Error::Ops("the deck made would not validate".into()))?;
        let last = (write.deck.slides().get(index).and_then(|states| states.last().copied()))
            .ok_or_else(|| Error::Ops(format!("the deck has no slide {index}")))?;
        scaena_ops::narrative::pixels(write, &write.deck.states[last].id, width).map_err(ops)
    }

    /// The deck made again with the model's `changes`, as `shown` makes it (PLAN G.13): each
    /// slide's change kept where lint finds the slide no worse for it, and what was kept, or why
    /// nothing was, said in the report.
    pub fn look(&mut self, shown: Shown, changes: Vec<Change>) -> Result<(), Error> {
        let Some(write) = self.write.take() else { return Ok(()) };
        let fonts = font(&self.fonts);
        let before = (self.made.clone(), write.clone());
        match looked(shown, changes, before, Path::new(&self.file), &self.theme, &fonts) {
            Ok((made, write)) => {
                self.made = made;
                self.write = Some(write);
                Ok(())
            }
            // The deck the model asked for could not be made again: the one made stands, saying
            // why (narrative STAB-3), never lost to its polish.
            Err(why) => {
                self.write = Some(write);
                self.made.unlooked = Some(format!("the deck the model asked for could not be made: {why}"));
                Ok(())
            }
        }
    }

    /// `built`, the deck the generator built again with the model's `changes` (PLAN G.13),
    /// made a bundle: kept where lint finds no slide worse for them; else the slides it finds
    /// worse, by index, and nothing kept, for the generator to build it again without theirs.
    /// `refused`, the slides a first try let go, is said in the report.
    pub fn relook(&mut self, mut built: Made, changes: Vec<Change>, refused: &[usize]) -> Result<Relooked, Error> {
        let Some(write) = &self.write else { return Ok(Relooked::Kept) };
        // The pictures the deck shows, which the generator builds it again without: the bytes of
        // the deck made.
        for (path, bytes) in write.files.iter().filter(|(path, _)| path.starts_with("assets/")) {
            built.assets.entry(path.clone()).or_insert_with(|| bytes.clone());
        }
        let fonts = font(&self.fonts);
        let after = making(built, &self.arc, Path::new(&self.file), self.theme.clone(), &fonts).map_err(ops)?;
        let (made, Some(after)) = after else {
            self.made.unlooked = Some("the deck the model asked for would not validate".into());
            return Ok(Relooked::Worse(Vec::new()));
        };
        let found = worse((&self.made, write), (&made, &after));
        if !found.is_empty() {
            return Ok(Relooked::Worse(found));
        }
        let before = std::mem::replace(&mut self.made, made.clone());
        let (made, write) = adopt(before, (made, after), kept(changes, refused), refused);
        self.made = made;
        self.write = Some(write);
        Ok(Relooked::Kept)
    }

    /// The look failed, or what it asked was not kept: why, which the report says.
    pub fn unlooked(&mut self, why: String) {
        self.made.unlooked = Some(why);
    }

    /// Why changes on `n` slides were not kept, as the report says it.
    pub fn unkept(n: usize) -> String {
        unkept(n)
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

    /// What still runs past its place once the last rewrite is set, which the deck made says
    /// (STAB-4): nothing where all of it fits.
    pub fn still_long(&mut self) -> Result<(), Error> {
        let longs = self.longs()?;
        if !longs.is_empty() {
            self.made.unfitted = Some(scaena_narrato::fit::still(&longs));
        }
        Ok(())
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
        Made { deck, layouts: vec!["title".into(), "title".into()], assets: Default::default(), relisted: Vec::new() }
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
    fn a_deck_built_again_keeps_its_pictures_and_what_lint_finds_no_worse() {
        let (theme, fonts) = dusk();
        let jpeg = std::fs::read(root().join("tests/fixtures/jpeg/baseline-422.jpg")).unwrap();
        let pictured = |title: &str| {
            let mut made = built();
            made.deck["nodes"]["cover-title"]["text"] = serde_json::json!(title);
            made.deck["nodes"]["cover-art"] = serde_json::json!({
                "type": "image", "src": "assets/picture-01.jpg", "alt": "A ship at dawn", "at": { "in": "art" }
            });
            made.deck["states"][0]["props"]["cover-art"] = serde_json::json!({});
            made
        };
        let mut first = pictured("Tidewater Freight");
        first.assets.insert("assets/picture-01.jpg".into(), jpeg);
        let mut m = Making::new(first, "Problem-Solution", None, "dusk.theme.json", &theme, &fonts).unwrap();
        // Built again by the generator's module, as JSON: its pictures' bytes are not in it.
        let again: Made = serde_json::from_value(serde_json::to_value(pictured("Tidewater")).unwrap()).unwrap();
        assert!(again.assets.is_empty());
        let change = Change { slide: 1, title: Some("Tidewater".into()), why: "Shorter.".into(), ..Change::default() };
        assert_eq!(m.relook(again, vec![change], &[]).unwrap(), Relooked::Kept);
        assert_eq!(m.report()["looked"][0]["slide"], 1);
        let files = m.files().unwrap().unwrap();
        assert!(files.contains_key("assets/picture-01.jpg"), "{:?}", files.keys());
        let deck: serde_json::Value = serde_json::from_slice(&files["deck.json"]).unwrap();
        assert_eq!(deck["nodes"]["cover-title"]["text"], "Tidewater");
        // Each state drawn for the model, as wide as asked.
        assert_eq!(m.count(), 2);
        let drawn = m.pixels(0, 320).unwrap();
        assert_eq!((drawn.width, drawn.height), (320, 180));
    }

    #[test]
    fn a_font_not_handed_over_says_which() {
        let (theme, _) = dusk();
        let e =
            Making::new(built(), "Problem-Solution", None, "dusk.theme.json", &theme, &BTreeMap::new()).err().unwrap();
        assert!(e.to_string().contains("was not handed over"), "{e}");
    }
}
