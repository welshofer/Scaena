//! Scaena draws every glyph unhinted (`hint(false)`), so the vendored skrifa compiles its hinters
//! out (vendor/skrifa/SCAENA.md, ADR-0004 finding 30): a hinting instance is refused, by the
//! interpreter and the autohinter alike, and a glyph still draws unhinted as it did.

use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawError, DrawSettings, Engine, HintingInstance, HintingOptions, OutlinePen};
use skrifa::{FontRef, MetadataProvider};

/// Counts what an outline draws.
#[derive(Default)]
struct Count(usize);

impl OutlinePen for Count {
    fn move_to(&mut self, _: f32, _: f32) {
        self.0 += 1;
    }
    fn line_to(&mut self, _: f32, _: f32) {
        self.0 += 1;
    }
    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {
        self.0 += 1;
    }
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {
        self.0 += 1;
    }
    fn close(&mut self) {}
}

#[test]
fn a_hinting_instance_is_refused_and_a_glyph_draws_unhinted() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let bytes = std::fs::read(root.join("tests/bench/b1.scaena/fonts/Inter-VF.ttf")).unwrap();
    let font = FontRef::new(&bytes).unwrap();
    let outlines = font.outline_glyphs();
    let size = Size::new(16.0);
    for engine in [Engine::Interpreter, Engine::AutoFallback, Engine::Auto(None)] {
        let options = HintingOptions { engine, ..HintingOptions::default() };
        let refused = HintingInstance::new(&outlines, size, LocationRef::default(), options);
        assert!(matches!(refused, Err(DrawError::HintingCompiledOut)), "{:?}", refused.err());
    }
    let glyph = outlines.get(font.charmap().map('a').unwrap()).unwrap();
    let mut pen = Count::default();
    glyph.draw(DrawSettings::unhinted(size, LocationRef::default()), &mut pen).unwrap();
    assert!(pen.0 > 4, "the glyph draws its outline: {}", pen.0);
}
