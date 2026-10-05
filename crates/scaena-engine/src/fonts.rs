//! Bundle-only font access (SPEC §3.1, §13.3).
//!
//! The render path sees exactly the fonts the bundle carries, so the same
//! bundle shapes to the same glyphs on every machine. System font discovery is
//! closed twice: `parley` and `fontique` are built without their `system`
//! feature (no platform backend is compiled in), and the collection here is
//! created with `system_fonts: false`, so cargo feature unification elsewhere
//! in a build cannot reopen it.
//!
//! The engine never reads files: callers hand [`BundleFonts`] the bytes of each
//! font in the bundle, keyed by its bundle id (its path inside the bundle).

use crate::EngineError;
use crate::theme::Theme;
use fontique::{Blob, Collection, CollectionOptions, FontStyle, SourceCache};
use parley::{FontContext, FontData};
use scaena_core::displaylist::FontRef;
use skrifa::color::{Brush, ColorPainter, CompositeMode, Transform};
use skrifa::instance::{LocationRef, NormalizedCoord, Size};
use skrifa::outline::{DrawSettings, OutlineGlyphCollection, OutlinePen};
use skrifa::raw::TableProvider;
use skrifa::raw::tables::glyf::Glyph;
use skrifa::raw::types::BoundingBox;
use skrifa::{GlyphId, MetadataProvider};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Arc;

/// A font context that can see only fonts registered from bundle bytes.
///
/// Never use `FontContext::new()` or `FontContext::default()` in the engine:
/// both discover system fonts when the platform backend is present.
pub fn bundle_font_context() -> FontContext {
    FontContext {
        collection: Collection::new(CollectionOptions { shared: false, system_fonts: false }),
        source_cache: SourceCache::default(),
    }
}

/// The bundle's fonts, registered into a bundle-only context, plus the map from
/// what parley hands back (a font blob and face index) to display-list font ids.
pub struct BundleFonts {
    pub(crate) cx: FontContext,
    /// Blob id → bundle font id. Blob ids are process-unique lookup keys and never
    /// reach a display list.
    by_blob: BTreeMap<u64, String>,
    /// Bundle font id → family names that file provides.
    families: BTreeMap<String, Vec<String>>,
    /// Bundle font ids whose faces are italic or oblique: what a family's italic face must be
    /// (PLAN 2.40).
    slanted: BTreeSet<String>,
    /// Bundle font id → what its license lets a document do with it.
    embedding: BTreeMap<String, Embedding>,
    /// The glyphs [`BundleFonts::check_glyphs`] has found the painters can draw, by font
    /// blob, face, and instance.
    drawable: HashMap<(u64, u32, Vec<i16>), HashSet<u32>>,
}

impl Default for BundleFonts {
    fn default() -> Self {
        Self {
            cx: bundle_font_context(),
            by_blob: BTreeMap::new(),
            families: BTreeMap::new(),
            slanted: BTreeSet::new(),
            embedding: BTreeMap::new(),
            drawable: HashMap::new(),
        }
    }
}

/// What a font's license lets a document do with it: its OS/2 embedding bits (`fsType`,
/// in OpenType's OS/2 table; SPEC §7.5 W230, §16 Q3). A font with no OS/2 table says
/// nothing, and reads as installable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Embedding(pub u16);

impl Embedding {
    /// A font file's, from its first face.
    pub fn of(bytes: &[u8]) -> Self {
        use skrifa::raw::TableProvider;
        let os2 = skrifa::FontRef::from_index(bytes, 0).ok().and_then(|f| f.os2().ok());
        Self(os2.map_or(0, |t| t.fs_type()))
    }

    /// Embedded only with its owner's permission: bit 1, with neither less restrictive
    /// usage bit (2 or 3) beside it. Where more than one is set, the least restrictive
    /// holds, as OpenType says.
    pub fn restricted(self) -> bool {
        self.0 & 0x000e == 0x0002
    }

    /// Embedded only in a document opened read-only: bit 2, without bit 3.
    pub fn preview_and_print(self) -> bool {
        self.0 & 0x000c == 0x0004
    }

    /// Not to be subset before it is embedded: bit 8.
    pub fn no_subsetting(self) -> bool {
        self.0 & 0x0100 != 0
    }

    /// Only its bitmaps may be embedded, not its outlines: bit 9.
    pub fn bitmap_only(self) -> bool {
        self.0 & 0x0200 != 0
    }
}

impl BundleFonts {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one font file under its bundle id (for example `fonts/RobotoSerif-VF.ttf`)
    /// and return the family names it provides. Registering the same id twice is an error.
    pub fn register(&mut self, id: &str, bytes: Vec<u8>) -> Result<Vec<String>, EngineError> {
        if self.families.contains_key(id) {
            return Err(EngineError::Font(format!("{id}: registered twice")));
        }
        let embedding = Embedding::of(&bytes);
        let blob = Blob::new(Arc::new(bytes));
        let blob_id = blob.id();
        let mut names: Vec<String> = Vec::new();
        let mut slanted = false;
        for (family, faces) in self.cx.collection.register_fonts(blob, None) {
            let name = self.cx.collection.family_name(family).unwrap_or_default().to_string();
            if !names.contains(&name) {
                names.push(name);
            }
            slanted |= faces.iter().any(|f| f.style() != FontStyle::Normal);
        }
        if names.is_empty() {
            return Err(EngineError::Font(format!("{id}: not a font file")));
        }
        self.by_blob.insert(blob_id, id.to_string());
        self.families.insert(id.to_string(), names.clone());
        if slanted {
            self.slanted.insert(id.to_string());
        }
        self.embedding.insert(id.to_string(), embedding);
        Ok(names)
    }

    /// What the license of the font registered as `id` lets a document do with it.
    pub fn embedding(&self, id: &str) -> Option<Embedding> {
        self.embedding.get(id).copied()
    }

    /// Check that every family in `theme` has its file registered and that the file
    /// provides the family name the theme declares; a mismatch would silently fall
    /// through to the next font in the stack.
    pub fn check_theme(&self, theme: &Theme) -> Result<(), EngineError> {
        for (key, def) in theme.families() {
            // The family's own face, and its italic (PLAN 2.40), which must be one.
            let italic = def.italic.as_ref().map(|face| (face.file.as_str(), "'s italic"));
            for (file, face) in std::iter::once((def.file.as_str(), "")).chain(italic) {
                let provided = self
                    .families
                    .get(file)
                    .ok_or_else(|| EngineError::Font(format!("family `{key}`{face}: {file} is not in the bundle")))?;
                if !provided.contains(&def.family) {
                    return Err(EngineError::Font(format!(
                        "family `{key}`{face}: {file} provides {provided:?}, not `{}`",
                        def.family
                    )));
                }
                if !face.is_empty() && !self.slanted.contains(file) {
                    return Err(EngineError::Font(format!("family `{key}`{face}: {file} is upright, not an italic")));
                }
            }
        }
        Ok(())
    }

    /// Check that each of `glyphs` draws in `font` at `coords` as the painters draw it: a
    /// color glyph's paint graph, each gradient in it with stops and each outline it clips
    /// to, or else the glyph's outline, from a font whose `head` table reads. A font damaged
    /// inside a glyph is an error here that names the font and the glyph. The painters
    /// cannot refuse one: the CPU painter's glyph cache unwraps what skrifa says of it, and
    /// in the browser a panic stops the worker. Each glyph is checked once per instance.
    pub fn check_glyphs(
        &mut self,
        font: &FontData,
        coords: &[i16],
        glyphs: impl IntoIterator<Item = u32>,
    ) -> Result<(), EngineError> {
        let key = (font.data.id(), font.index, coords.to_vec());
        let done = self.drawable.get(&key);
        let todo: Vec<u32> = glyphs.into_iter().filter(|g| done.is_none_or(|d| !d.contains(g))).collect();
        if todo.is_empty() {
            return Ok(());
        }
        let name = self.by_blob.get(&font.data.id()).map_or("a font", String::as_str);
        let damaged = |what: String| EngineError::Font(format!("{name}: {what}: the font file is damaged"));
        let face = skrifa::FontRef::from_index(font.data.data(), font.index).map_err(|e| damaged(e.to_string()))?;
        face.head().map_err(|e| damaged(format!("its `head` table: {e}")))?;
        let normalized: Vec<NormalizedCoord> = coords.iter().map(|&c| NormalizedCoord::from_bits(c)).collect();
        let location = LocationRef::new(&normalized);
        let outlines = face.outline_glyphs();
        let colors = face.color_glyphs();
        for &gid in &todo {
            let id = GlyphId::new(gid);
            let glyph = |what: String| damaged(format!("glyph {gid}: {what}"));
            match colors.get(id) {
                Some(color) => {
                    let mut check = PaintCheck { face: &face, outlines: &outlines, location, error: None };
                    color.paint(location, &mut check).map_err(|e| glyph(e.to_string()))?;
                    check.error.map_or(Ok(()), |e| Err(glyph(e)))?;
                }
                None => draws(&face, &outlines, id, location).map_err(glyph)?,
            }
        }
        self.drawable.entry(key).or_default().extend(todo);
        Ok(())
    }

    /// The display-list reference for a font parley selected.
    pub fn font_ref(&self, font: &FontData) -> Result<FontRef, EngineError> {
        let id = self.by_blob.get(&font.data.id()).ok_or_else(|| {
            EngineError::Font("parley selected a font that is not in the bundle (fallback leaked)".into())
        })?;
        Ok(FontRef { id: id.clone(), index: font.index })
    }
}

/// Whether glyph `id`'s outline draws at `location`, unhinted, as the painters draw it. A
/// glyph with no outline draws nothing, which is no error.
fn draws(
    face: &skrifa::FontRef,
    outlines: &OutlineGlyphCollection,
    id: GlyphId,
    location: LocationRef,
) -> Result<(), String> {
    if pointless(face, id) {
        return Err("its outline has no points and data for some".into());
    }
    let Some(outline) = outlines.get(id) else { return Ok(()) };
    let settings = DrawSettings::unhinted(Size::unscaled(), location);
    outline.draw(settings, &mut NoPen).map(|_| ()).map_err(|e| format!("its outline: {e}"))
}

/// Whether glyph `id`, or a glyph it is made of, has no points yet data for some. The
/// read-fonts that skrifa reads outlines with here, as the CPU painter's glyph cache does
/// (0.41), reads such a glyph's flags past the end of its points, a panic that 0.44 fixes;
/// parley pins 0.41 until it moves. The glyph is refused before skrifa reads it.
fn pointless(face: &skrifa::FontRef, id: GlyphId) -> bool {
    let (Ok(loca), Ok(glyf)) = (face.loca(None), face.glyf()) else { return false };
    let (mut todo, mut seen) = (vec![id], HashSet::new());
    // A composite can name itself, or a glyph that names it back: each glyph is read once,
    // and at most 64 of them.
    while let Some(id) = todo.pop() {
        if seen.len() > 64 || !seen.insert(id) {
            continue;
        }
        match loca.get_glyf(id, &glyf) {
            Ok(Some(Glyph::Simple(simple))) if simple.num_points() == 0 && !simple.glyph_data().is_empty() => {
                return true;
            }
            Ok(Some(Glyph::Composite(composite))) => {
                todo.extend(composite.components().map(|c| GlyphId::from(c.glyph)))
            }
            _ => {}
        }
    }
    false
}

/// A pen that keeps nothing: drawing into it only reads the outline.
struct NoPen;

impl OutlinePen for NoPen {
    fn move_to(&mut self, _: f32, _: f32) {}
    fn line_to(&mut self, _: f32, _: f32) {}
    fn quad_to(&mut self, _: f32, _: f32, _: f32, _: f32) {}
    fn curve_to(&mut self, _: f32, _: f32, _: f32, _: f32, _: f32, _: f32) {}
    fn close(&mut self) {}
}

/// Walks a color glyph's paint graph as a painter would, keeping the first thing in it a
/// painter could not draw: an outline it clips to that does not draw, or a gradient with
/// no stops.
struct PaintCheck<'a> {
    face: &'a skrifa::FontRef<'a>,
    outlines: &'a OutlineGlyphCollection<'a>,
    location: LocationRef<'a>,
    error: Option<String>,
}

impl ColorPainter for PaintCheck<'_> {
    fn push_transform(&mut self, _: Transform) {}
    fn pop_transform(&mut self) {}
    fn push_clip_glyph(&mut self, glyph_id: GlyphId) {
        if self.error.is_none()
            && let Err(e) = draws(self.face, self.outlines, glyph_id, self.location)
        {
            self.error = Some(format!("glyph {} it clips to: {e}", glyph_id.to_u32()));
        }
    }
    fn push_clip_box(&mut self, _: BoundingBox<f32>) {}
    fn pop_clip(&mut self) {}
    fn fill(&mut self, brush: Brush<'_>) {
        let stops = match brush {
            Brush::Solid { .. } => return,
            Brush::LinearGradient { color_stops, .. }
            | Brush::RadialGradient { color_stops, .. }
            | Brush::SweepGradient { color_stops, .. } => color_stops,
        };
        if stops.is_empty() && self.error.is_none() {
            self.error = Some("a gradient with no color stops".into());
        }
    }
    fn push_layer(&mut self, _: CompositeMode) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use fontique::GenericFamily;

    #[test]
    fn bundle_context_sees_no_system_fonts() {
        let mut cx = bundle_font_context();
        let names: Vec<&str> = cx.collection.family_names().collect();
        assert!(names.is_empty(), "system fonts leaked into the render path: {names:?}");
        for generic in [
            GenericFamily::Serif,
            GenericFamily::SansSerif,
            GenericFamily::Monospace,
            GenericFamily::Cursive,
            GenericFamily::Fantasy,
            GenericFamily::SystemUi,
            GenericFamily::UiSerif,
            GenericFamily::UiSansSerif,
            GenericFamily::UiMonospace,
            GenericFamily::UiRounded,
            GenericFamily::Emoji,
            GenericFamily::Math,
            GenericFamily::FangSong,
        ] {
            assert_eq!(cx.collection.generic_families(generic).count(), 0, "{generic:?} resolves to a system font");
        }
    }

    #[test]
    fn rejects_non_fonts_and_double_registration() {
        let mut fonts = BundleFonts::new();
        assert!(fonts.register("fonts/x.ttf", b"not a font".to_vec()).is_err());
        let bytes = std::fs::read("../../tests/fixtures/torture.scaena/fonts/NotoSansHebrew-VF.ttf").unwrap();
        assert_eq!(fonts.register("fonts/he.ttf", bytes.clone()).unwrap(), ["Noto Sans Hebrew"]);
        assert!(fonts.register("fonts/he.ttf", bytes).is_err());
    }

    /// `bytes` with its OS/2 `fsType` set to `bits`: the table's offset from the table
    /// directory, then the field 8 bytes in. Checksums go stale, which nothing here reads.
    fn with_fs_type(mut bytes: Vec<u8>, bits: u16) -> Vec<u8> {
        let tables = u16::from_be_bytes([bytes[4], bytes[5]]) as usize;
        let record = (0..tables).map(|i| 12 + 16 * i).find(|&r| &bytes[r..r + 4] == b"OS/2").unwrap();
        let at = u32::from_be_bytes(bytes[record + 8..record + 12].try_into().unwrap()) as usize + 8;
        bytes[at..at + 2].copy_from_slice(&bits.to_be_bytes());
        bytes
    }

    #[test]
    fn embedding_reads_what_a_fonts_license_allows() {
        let font = std::fs::read("../../tests/fixtures/torture.scaena/fonts/NotoSansHebrew-VF.ttf").unwrap();
        let says = |bits: u16| {
            let e = Embedding::of(&with_fs_type(font.clone(), bits));
            assert_eq!(e, Embedding(bits));
            (e.restricted(), e.preview_and_print(), e.no_subsetting(), e.bitmap_only())
        };
        // Installable, and editable: nothing to say.
        assert_eq!(says(0x0000), (false, false, false, false));
        assert_eq!(says(0x0008), (false, false, false, false));
        assert_eq!(says(0x0002), (true, false, false, false));
        assert_eq!(says(0x0004), (false, true, false, false));
        // More than one usage bit: the least restrictive holds.
        assert_eq!(says(0x0006), (false, true, false, false));
        assert_eq!(says(0x000a), (false, false, false, false));
        assert_eq!(says(0x000e), (false, false, false, false));
        // The two that limit how it is embedded stand on their own.
        assert_eq!(says(0x0100), (false, false, true, false));
        assert_eq!(says(0x0302), (true, false, true, true));
        // A file that is no font says nothing.
        assert_eq!(Embedding::of(b"not a font"), Embedding(0));
        // The registry keeps each font's bits by its bundle id.
        let mut fonts = BundleFonts::new();
        fonts.register("fonts/he.ttf", with_fs_type(font, 0x0102)).unwrap();
        assert_eq!(fonts.embedding("fonts/he.ttf"), Some(Embedding(0x0102)));
        assert_eq!(fonts.embedding("fonts/other.ttf"), None);
    }
}
