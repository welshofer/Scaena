//! W230: a font whose license, in its OS/2 embedding bits, does not allow what Scaena
//! does with a font (SPEC §7.5, §16 Q3). It warns, and blocks nothing.

use crate::fonts::{BundleFonts, Embedding};
use scaena_core::document::Deck;
use scaena_core::lint::{Finding, Severity};

/// W230: each font the deck lists whose embedding bits (`fsType`) forbid what a bundle
/// does with it. A bundle carries the font, and is edited; saving it, and every export
/// that embeds fonts, subsets it; and every export embeds outlines. One finding per font,
/// naming each thing its license does not allow.
pub struct W230FontLicense;

impl W230FontLicense {
    pub const CODE: &str = "W230";

    pub fn check(deck: &Deck, fonts: &BundleFonts) -> Vec<Finding> {
        let mut out = Vec::new();
        for (i, font) in deck.fonts.iter().enumerate() {
            let Some(e) = fonts.embedding(&font.file) else { continue };
            let says = forbids(e);
            if says.is_empty() {
                continue;
            }
            let message = format!("font `{}` ({}): its license {}", font.file, font.family, says.join("; and "));
            out.push(
                Finding::new(Self::CODE, Severity::Warning, message)
                    .at(format!("/fonts/{i}/file"))
                    .measure(serde_json::json!({ "fsType": format!("0x{:04x}", e.0) }))
                    .hint(
                        "Ask the font's owner whether you may embed it, or choose a font whose license allows embedding, \
                         as the SIL Open Font License does.",
                    ),
            );
        }
        out
    }
}

/// What a font's embedding bits do not allow, each said as what the license allows and
/// what Scaena does, in the order of the bits.
fn forbids(e: Embedding) -> Vec<&'static str> {
    let mut out = Vec::new();
    if e.restricted() {
        out.push(
            "allows embedding only with its owner's permission (restricted), and a bundle carries the font, as a \
             PDF or a single-file export does",
        );
    }
    if e.preview_and_print() {
        out.push(
            "allows embedding only in documents opened read-only (preview and print), and a bundle that carries it \
             is edited",
        );
    }
    if e.no_subsetting() {
        out.push(
            "forbids subsetting it, and Scaena subsets the fonts it saves and exports (`scaena save --keep-fonts` \
             keeps a bundle's whole)",
        );
    }
    if e.bitmap_only() {
        out.push("allows embedding only its bitmaps, and every export embeds outlines");
    }
    out
}
