//! # scaena-subset
//!
//! The font subsetter in the browser (PLAN 2.4): `scaena save`'s ([`scaena_store::subset`]),
//! as a WASM module of its own. A page loads it when it downloads a bundle, subsets each font
//! the engine names to the characters the engine gives, and hands the subsets back
//! (`Player.subsetting`, `Player.addSubset`). The engine's module leaves the subsetter out:
//! it is a quarter of a megabyte gzipped, and only a download runs it (SPEC §15).

use wasm_bindgen::prelude::*;

/// `font` with the glyphs for `chars` and the base ranges every subset keeps, at their old
/// glyph ids: the bytes `scaena save` writes for it.
#[wasm_bindgen]
pub fn subset(font: &[u8], chars: &str) -> Result<Vec<u8>, JsError> {
    scaena_store::subset::subset(font, &chars.chars().collect()).map_err(|e| JsError::new(&e.to_string()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_subsets_as_a_save_does() {
        let font = std::fs::read("../../tests/bench/b1.scaena/fonts/Inter-VF.ttf").unwrap();
        let chars = "Scaena, 2026.";
        let ours = super::subset(&font, chars).map_err(|_| "subset").unwrap();
        assert_eq!(ours, scaena_store::subset::subset(&font, &chars.chars().collect()).unwrap());
    }
}
