//! Text layout (SPEC §3.5): shaping via `harfrust` through `parley`, line breaking
//! (`greedy` / `pretty` / `balance`), cap-height boxes, hanging punctuation,
//! optical margins, split units for animation.
//!
//! Phase 0 tasks 0.4–0.5. Fonts come from the bundle only (SPEC §13.3): the
//! font context is [`crate::fonts::BundleFonts`], never the system.
//!
//! Output is a list of glyph runs with final positions (the display list's
//! `glyphs` op), so painters never shape.
//!
//! Line breaking:
//! - `greedy`: parley's breaker at the box width.
//! - `balance`: the narrowest width at which greedy still yields the same number of
//!   lines (the `text-wrap: balance` method), found by bisection.
//! - `pretty`: minimum raggedness over parley's own break opportunities, with the
//!   last line held to `minLastLineWords`, realized by handing parley's breaker one
//!   width per line and checking it broke where planned.
//!
//! `balance` and `pretty` fall back to greedy for text with hard line breaks, and say
//! so in [`TextLayout::fallback`]. All three hold each paragraph's last line to
//! `minLastLineWords` words: a word is the text between two break opportunities, and a
//! word hyphenation splits is one, so no paragraph ends on a word's tail alone.
//!
//! Hyphenation (`hyphenate`, by `lang`) puts soft hyphens at the hyphenation points of
//! the language's TeX patterns (`hypher`). A line that ends at one draws `-` in its look,
//! and breaking counts it.
//!
//! Lines break at the box width, or at `measure` characters (`ch`, the advance of `0`)
//! if that is narrower, and align across the box (SPEC §3.4): `start`, `center`, or
//! `end`, in the paragraph's direction. The engine places each line itself, from parley's
//! left-aligned lines, so lines broken at widths of their own (`pretty`, hanging quotes)
//! align like any other, in either direction.
//!
//! Hanging quotes: quotation marks hang outside an aligned edge, never a ragged one
//! (SPEC §3.5). In start-aligned text the marks that open a line hang outside its start
//! edge; in end-aligned text the marks that close a line hang outside its end edge;
//! centered text has no aligned edge. All three breakings measure a line without its
//! hung marks, so they take nothing from the measure, and the letter beside them sits on
//! the edge.
//!
//! `case` sets the text in capitals, lower case, or title case before shaping, and
//! small capitals through the font's `smcp` (nothing is synthesized).
//!
//! Each glyph keeps the offset of the cluster it sets, so [`TextLayout::units`] can cut
//! the laid-out text into lines, words, or clusters for choreography without laying it
//! out again.

use crate::EngineError;
use crate::fonts::BundleFonts;
use crate::theme::{Numeric, TextBox, TextRole, Theme, Wrap};
use parley::layout::BreakReason;
use parley::setting::Tag;
use parley::{
    Alignment, AlignmentOptions, Cluster, FontFamily, FontFamilyName, FontFeature, FontFeatures, FontVariation,
    FontVariations, FontWeight, Language, Layout, LayoutContext, LineHeight, OverflowWrap, PositionedLayoutItem,
    StyleProperty, WordBreak,
};
use scaena_core::displaylist::{Color, FontRef, Glyph};
use scaena_core::model::theme::Case;
use scaena_core::model::values::TextSplit;
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::ops::Range;

/// Text color as parley carries it per run: sRGB with straight alpha, like [`Color`].
type Ink = [u8; 4];

/// Narrower than any segment, so probing at this width breaks at every opportunity.
const PROBE_WIDTH: f32 = 1e-3;
/// Added to each planned line width, so a different float summation order inside
/// parley cannot reject a planned fit. Smaller than any segment that could follow.
const FIT_SLACK: f32 = 0.25;
/// Cost of one more line in the `pretty` plan, against a slack cost of
/// `1000 · (slack / width)²`: a line 10% short costs as much as an extra line.
const LINE_PENALTY: f32 = 10.0;
/// Bisection steps for `balance`: the found width is within `width / 2^24` of optimal.
const BALANCE_STEPS: u32 = 24;
/// The soft hyphen hyphenation inserts: a break opportunity that draws a hyphen only when
/// a line ends there.
const SHY: char = '\u{AD}';
/// What ending a line with a hyphen adds to the `pretty` cost: half an extra line, so a
/// hyphen has to buy real evenness.
const HYPHEN_PENALTY: f32 = 5.0;

/// One span of a text node, and the look it is set in (its role after the cascade).
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub style: TextRole,
}

/// A text node after the cascade, ready to lay out.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpec {
    pub spans: Vec<Span>,
    /// The node's look: its role after its style. Paragraph settings (wrap,
    /// `minLastLineWords`) come from it; each span's own look sets its glyphs.
    pub role: TextRole,
    /// Node-level settings layered over every span's role.
    pub features: BTreeMap<String, u16>,
    pub axes: BTreeMap<String, f32>,
    pub numeric: Option<Numeric>,
    pub wrap: Option<Wrap>,
    pub min_last_line_words: Option<u32>,
    pub lang: Option<String>,
    /// How lines sit across the box.
    pub align: TextAlign,
    /// The most characters a line may hold, in `ch` of the node's look; the node's or
    /// its role's `measure`.
    pub measure: Option<f32>,
    /// `hangingPunctuation`: brackets, stops, commas, and hyphens hang at an aligned edge
    /// as quotation marks always do.
    pub hanging_punctuation: bool,
    /// `opticalMargins`: letters and punctuation at an aligned edge move part of their
    /// width past it, so the edge looks straight.
    pub optical_margins: bool,
    /// `hyphenate`: words may break at the hyphenation points of `lang`.
    pub hyphenate: bool,
}

/// How a paragraph's lines sit across its box, in the paragraph's direction (SPEC §3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Start,
    Center,
    End,
}

/// The edge quotation marks hang outside of: the aligned one (SPEC §3.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Edge {
    Start,
    End,
    /// Both edges are ragged.
    Neither,
}

impl From<TextAlign> for Edge {
    fn from(align: TextAlign) -> Edge {
        match align {
            TextAlign::Start => Edge::Start,
            TextAlign::End => Edge::End,
            TextAlign::Center => Edge::Neither,
        }
    }
}

/// The hyphen a line draws where it breaks inside a word of one span: `-` shaped in the
/// span's look, at x = 0 on the baseline, moved to the line's end when drawn.
#[derive(Debug, Clone)]
struct Hyphen {
    /// The span's bytes in the laid-out text.
    range: Range<usize>,
    advance: f32,
    run: GlyphRun,
}

/// The hyphenation patterns for a BCP 47 tag, by its language subtag: the seventeen the
/// engine carries (`hypher`, TeX patterns), or none.
fn hyphenation_lang(tag: &str) -> Option<hypher::Lang> {
    let code = tag.split(['-', '_']).next()?.to_ascii_lowercase();
    let code: [u8; 2] = code.as_bytes().try_into().ok()?;
    hypher::Lang::from_iso(code)
}

/// `text` with a soft hyphen at each hyphenation point of its words (runs of letters), or
/// `None` when no word has one. A word with a soft hyphen of its own keeps only those.
fn hyphenate(text: &str, lang: hypher::Lang) -> Option<String> {
    let mut out = String::with_capacity(text.len() + text.len() / 4);
    let mut changed = false;
    let mut rest = text;
    while !rest.is_empty() {
        let start = rest.find(char::is_alphabetic).unwrap_or(rest.len());
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let end = rest.find(|c: char| !c.is_alphabetic()).unwrap_or(rest.len());
        let word = &rest[..end];
        // The author's soft hyphens, on either side, mean the word is hyphenated already.
        if !word.is_empty() && !out.ends_with(SHY) && !rest[end..].starts_with(SHY) {
            let mut syllables = hypher::hyphenate(word, lang);
            if let Some(first) = syllables.next() {
                out.push_str(first);
                for syllable in syllables {
                    out.push(SHY);
                    out.push_str(syllable);
                    changed = true;
                }
            }
        } else {
            out.push_str(word);
        }
        rest = &rest[end..];
    }
    changed.then_some(out)
}

/// What hangs outside the aligned edge (SPEC §3.5): quotation marks always; with
/// `hangingPunctuation`, opening brackets at a start edge, and closing brackets, stops,
/// commas, and hyphens at an end edge.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Hang {
    edge: Edge,
    punctuation: bool,
}

impl Hang {
    fn opens(self, c: char) -> bool {
        is_hanging_quote(c) || (self.punctuation && matches!(c, '(' | '[' | '{'))
    }

    fn closes(self, c: char) -> bool {
        is_hanging_quote(c) || (self.punctuation && is_hanging_stop(c))
    }

    /// Whether a drawn hyphen hangs: hanging punctuation at an aligned end edge.
    fn hangs_hyphens(self) -> bool {
        self.punctuation && self.edge == Edge::End
    }

    /// Whether anything in `text` could hang at all.
    fn possible(self, text: &str) -> bool {
        match self.edge {
            Edge::Start => text.chars().any(|c| self.opens(c)),
            Edge::End => text.chars().any(|c| self.closes(c)),
            Edge::Neither => false,
        }
    }

    /// What a line over `line` hangs outside the aligned edge.
    fn at(self, layout: &Layout<Ink>, text: &str, line: Range<usize>, rtl: bool) -> f32 {
        match self.edge {
            Edge::Start => hang_start(layout, text, line, rtl, |c| self.opens(c)),
            Edge::End => hang_end(layout, text, line, rtl, |c| self.closes(c)),
            Edge::Neither => 0.0,
        }
    }
}

/// A shaped, broken, positioned paragraph. Coordinates are canvas units relative to
/// the text box's top-left corner; y grows down.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    /// The text as laid out: its spans in their `case`, with the soft hyphens hyphenation
    /// inserted. Line ranges index into it.
    pub text: String,
    pub lines: Vec<LineBox>,
    pub runs: Vec<GlyphRun>,
    /// Widest line, trailing whitespace and hung quotes excluded.
    pub width: f32,
    pub height: f32,
    /// The breaking actually used.
    pub wrap: Wrap,
    /// Why the requested `pretty` or `balance` fell back to greedy, if it did.
    pub fallback: Option<&'static str>,
    /// The text's words, as byte ranges of `text`: what lines break between (UAX #14),
    /// so a word keeps the punctuation and space after it, and a hyphenated word is one.
    /// `minLastLineWords` counts them and `split: words` animates them.
    pub words: Vec<Range<usize>>,
    /// A paragraph's last line starts fewer words than `minLastLineWords` asks, although
    /// the paragraph has that many, and breaking could not give it more (lint W200).
    pub widow: bool,
    pub rtl: bool,
    /// Some run asked for faux bold or oblique, which the display list cannot express.
    pub synthesized: bool,
    /// How its lines align across the box.
    pub align: TextAlign,
    /// Where its lines break: the box's width, or its `measure` if that is narrower. A
    /// line wider than this holds a word that cannot break (lint W201).
    pub measure: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LineBox {
    /// Top of the line box (CSS): the sum of the heights of the lines above.
    pub top: f32,
    /// Baseline, from the paragraph top.
    pub baseline: f32,
    /// Line-box height (leading × size of the tallest run).
    pub height: f32,
    pub ascent: f32,
    pub descent: f32,
    /// Advance without trailing whitespace and without hung marks: what sits inside the
    /// measure.
    pub width: f32,
    /// Where that part of the line starts, from the box's left edge.
    pub x: f32,
    /// Advance of the quotation marks hung outside the start edge (start-aligned text):
    /// left of `x` on a left-to-right line, right of `x + width` on a right-to-left one.
    pub hang: f32,
    /// Advance of the quotation marks hung outside the end edge (end-aligned text), and
    /// with `hangingPunctuation`, of the stops, commas, hyphens, and brackets.
    pub hang_end: f32,
    /// How far optical margins moved the line past its aligned edge (`opticalMargins`).
    pub optical: f32,
    /// The line broke at a soft hyphen and ends with a hyphen, counted in `width`.
    pub hyphen: bool,
    /// Byte range in the node's text (spans concatenated).
    pub text: Range<usize>,
    /// From the line's first run (OS/2), when the font provides it.
    pub cap_height: Option<f32>,
    pub x_height: Option<f32>,
}

impl TextLayout {
    /// The vertical extent the text aligns by, from the paragraph top. `line`: the line
    /// boxes. `cap`: from the first line's cap height to the last line's baseline (CSS
    /// `text-box: trim-both cap alphabetic`), so a cap top can sit exactly on a grid
    /// line. A font without OS/2 cap height keeps the line edge on that side.
    pub fn trimmed(&self, trim: TextBox) -> (f32, f32) {
        let (Some(first), Some(last)) = (self.lines.first(), self.lines.last()) else { return (0.0, 0.0) };
        match trim {
            TextBox::Line => (0.0, self.height),
            TextBox::Cap => (first.cap_height.map_or(0.0, |cap| first.baseline - cap), last.baseline),
        }
    }

    /// The units `split` cuts the text into (SPEC §3.5), in reading order: its lines; its
    /// [`words`](TextLayout::words), a hyphenated word one unit with its hyphen and its
    /// glyphs on both lines; or its clusters, a ligature one unit. A unit sets ink:
    /// whitespace and invisible characters are in a line or a word, never a unit of
    /// their own. Frames draw a unit's glyphs where the layout put them; choreography
    /// moves each unit as one (§3.9, PLAN 1.11).
    pub fn units(&self, split: TextSplit) -> Vec<TextUnit> {
        let glyphs = || {
            self.runs
                .iter()
                .enumerate()
                .flat_map(|(r, run)| run.clusters.iter().enumerate().map(move |(g, &c)| (r, g, c)))
        };
        let unit = |text: Range<usize>, inside: &dyn Fn(usize, usize) -> bool| {
            if !self.text[text.clone()].chars().any(sets_ink) {
                return None;
            }
            let glyphs: Vec<(usize, usize)> =
                glyphs().filter(|&(r, _, c)| inside(r, c)).map(|(r, g, _)| (r, g)).collect();
            let line = self.runs[glyphs.first()?.0].line;
            Some(TextUnit { text, line, glyphs })
        };
        match split {
            TextSplit::Lines => (self.lines.iter().enumerate())
                .filter_map(|(k, line)| unit(line.text.clone(), &|r, _| self.runs[r].line == k))
                .collect(),
            TextSplit::Words => self.words.iter().filter_map(|w| unit(w.clone(), &|_, c| w.contains(&c))).collect(),
            TextSplit::Glyphs => {
                let mut starts: Vec<usize> = glyphs().map(|(_, _, c)| c).collect();
                starts.sort_unstable();
                starts.dedup();
                (starts.iter().enumerate())
                    .filter_map(|(i, &start)| {
                        let end = starts.get(i + 1).copied().unwrap_or(self.text.len());
                        unit(start..end, &|_, c| c == start)
                    })
                    .collect()
            }
        }
    }
}

/// One unit of a split text: what choreography staggers (SPEC §3.9).
#[derive(Debug, Clone, PartialEq)]
pub struct TextUnit {
    /// Its text, a byte range of [`TextLayout::text`].
    pub text: Range<usize>,
    /// The line its first glyph is on.
    pub line: usize,
    /// Its glyphs, as `(run, glyph)` indices into [`TextLayout::runs`], in paint order.
    pub glyphs: Vec<(usize, usize)>,
}

/// Glyphs that share font, size, instance, and color.
#[derive(Debug, Clone, PartialEq)]
pub struct GlyphRun {
    pub font: FontRef,
    pub size: f32,
    pub coords: Vec<i16>,
    pub color: Color,
    /// Positions relative to the text box's top-left corner; y is the baseline.
    pub glyphs: Vec<Glyph>,
    /// Where the cluster each glyph sets starts in [`TextLayout::text`]: what split units
    /// group glyphs by. A ligature's glyph belongs to its first cluster, and the hyphen
    /// drawn at a break to the cluster before the soft hyphen.
    pub clusters: Vec<usize>,
    /// Each glyph's advance: with its position, the box a split unit turns about.
    pub advances: Vec<f32>,
    pub line: usize,
}

/// Reusable scratch for text layout (parley's layout context). One per thread.
pub struct TextEngine {
    lcx: LayoutContext<Ink>,
}

impl Default for TextEngine {
    fn default() -> Self {
        Self { lcx: LayoutContext::new() }
    }
}

impl TextSpec {
    /// The same text set `scale` times larger (`fit: shrink` and `grow`): every span's
    /// size; leading and tracking follow, as they are relative to it.
    pub fn scaled(&self, scale: f32) -> TextSpec {
        let mut spec = self.clone();
        spec.role.size *= scale;
        for span in &mut spec.spans {
            span.style.size *= scale;
        }
        spec
    }

    /// `text` in one look, with no node-level settings.
    pub fn plain(role: TextRole, text: impl Into<String>) -> TextSpec {
        TextSpec {
            spans: vec![Span { text: text.into(), style: role.clone() }],
            role,
            features: BTreeMap::new(),
            axes: BTreeMap::new(),
            numeric: None,
            wrap: None,
            min_last_line_words: None,
            lang: None,
            align: TextAlign::Start,
            measure: None,
            hanging_punctuation: false,
            optical_margins: false,
            hyphenate: false,
        }
    }
}

impl TextEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Shape `spec` with the bundle's fonts, break it into lines at most `width` wide (or
    /// `measure` characters, if that is narrower), and align the lines across `width`.
    pub fn layout(
        &mut self,
        fonts: &mut BundleFonts,
        theme: &Theme,
        spec: &TextSpec,
        width: f32,
    ) -> Result<TextLayout, EngineError> {
        let base = &spec.role;
        let mut cased: Vec<Cow<str>> = spec.spans.iter().map(|s| set_case(&s.text, s.style.case)).collect();
        if spec.hyphenate
            && let Some(lang) = spec.lang.as_deref().and_then(hyphenation_lang)
        {
            for t in &mut cased {
                if let Some(hyphenated) = hyphenate(t, lang) {
                    *t = Cow::Owned(hyphenated);
                }
            }
        }
        let text: String = cased.iter().map(|t| t.as_ref()).collect();
        let max_width = match spec.measure {
            Some(chars) => width.min(chars * self.ch(fonts, theme, spec)?),
            None => width,
        };
        let mut layout = {
            let mut builder = self.lcx.ranged_builder(&mut fonts.cx, &text, 1.0, false);
            for prop in style_props(theme, base, spec)? {
                builder.push_default(prop);
            }
            let mut at = 0;
            for (span, t) in spec.spans.iter().zip(&cased) {
                let range = at..at + t.len();
                at = range.end;
                if span.style != spec.role {
                    for prop in style_props(theme, &span.style, spec)? {
                        builder.push(prop, range.clone());
                    }
                }
            }
            builder.build(&text)
        };
        // The hyphen each span draws where a line breaks inside one of its words.
        let mut hyphens = Vec::new();
        let mut at = 0;
        for (span, t) in spec.spans.iter().zip(&cased) {
            let range = at..at + t.len();
            at = range.end;
            if t.contains(SHY) {
                hyphens.push(self.hyphen(fonts, theme, spec, &span.style, range)?);
            }
        }

        let rtl = layout.is_rtl();
        let hang = Hang { edge: Edge::from(spec.align), punctuation: spec.hanging_punctuation };
        let room = Room { hang, hyphens: &hyphens };
        let segments = segments(&mut layout, &text, rtl, room);
        let min_words = spec.min_last_line_words.or(base.min_last_line_words).unwrap_or(1) as usize;
        let breaking = Breaking { text: &text, rtl, room, segments: &segments, min_words };
        let requested = spec.wrap.unwrap_or(base.wrap);
        let mut fallback = match requested {
            Wrap::Greedy => None,
            _ if text.contains(['\n', '\r', '\u{2028}', '\u{2029}']) => Some("hard line break"),
            _ => None,
        };
        let wrap = match (requested, fallback) {
            (_, Some(_)) | (Wrap::Greedy, _) => {
                fallback = fallback.or(greedy(&mut layout, breaking, max_width));
                Wrap::Greedy
            }
            (Wrap::Balance, None) => {
                fallback = balance(&mut layout, breaking, max_width);
                Wrap::Balance
            }
            (Wrap::Pretty, None) => {
                if pretty(&mut layout, breaking, max_width) {
                    Wrap::Pretty
                } else {
                    fallback = Some("parley did not break where the pretty plan said");
                    greedy(&mut layout, breaking, max_width);
                    Wrap::Greedy
                }
            }
        };
        // Every line's ink from x = 0, in either direction; `read_layout` places it.
        layout.align(Alignment::Left, AlignmentOptions { align_when_overflowing: true });
        let optical = spec.optical_margins;
        let widow = widowed(&layout, &segments, min_words);
        let words = words(&segments);
        let paragraph = Paragraph {
            wrap,
            fallback,
            rtl,
            align: spec.align,
            hang,
            optical,
            width,
            measure: max_width,
            words,
            widow,
        };
        read_layout(&layout, text, fonts, &hyphens, paragraph)
    }

    /// The hyphen text in `style` draws at a line it breaks inside a word: `-` shaped in
    /// that look, so the breaking reserves exactly the width it draws.
    fn hyphen(
        &mut self,
        fonts: &mut BundleFonts,
        theme: &Theme,
        spec: &TextSpec,
        style: &TextRole,
        range: Range<usize>,
    ) -> Result<Hyphen, EngineError> {
        let mut builder = self.lcx.ranged_builder(&mut fonts.cx, "-", 1.0, false);
        for prop in style_props(theme, style, spec)? {
            builder.push_default(prop);
        }
        let mut layout = builder.build("-");
        layout.break_all_lines(None);
        let read = read_layout(&layout, "-".into(), fonts, &[], Paragraph::plain())?;
        let baseline = read.lines.first().map_or(0.0, |l| l.baseline);
        let mut run = read.runs.into_iter().next().ok_or_else(|| EngineError::Font("no glyph for a hyphen".into()))?;
        // From its own line's baseline: `read_layout` sets it on the line it ends.
        for g in &mut run.glyphs {
            g.y -= baseline;
        }
        Ok(Hyphen { range, advance: read.width, run })
    }

    /// `ch` in the node's look: the advance of `0` (CSS `ch`), which `measure` counts in.
    fn ch(&mut self, fonts: &mut BundleFonts, theme: &Theme, spec: &TextSpec) -> Result<f32, EngineError> {
        let mut builder = self.lcx.ranged_builder(&mut fonts.cx, "0", 1.0, false);
        for prop in style_props(theme, &spec.role, spec)? {
            builder.push_default(prop);
        }
        let mut layout = builder.build("0");
        layout.break_all_lines(None);
        Ok(layout.width())
    }
}

/// `text` in `case`: capitals, lower case, or each word's first letter capitalized.
/// Small capitals are the font's `smcp` (`style_props`), so the letters stay as written.
/// The mappings are Unicode's defaults, the same in every language.
fn set_case(text: &str, case: Option<Case>) -> Cow<'_, str> {
    match case {
        Some(Case::Upper) => Cow::Owned(text.to_uppercase()),
        Some(Case::Lower) => Cow::Owned(text.to_lowercase()),
        Some(Case::Title) => {
            let mut out = String::with_capacity(text.len());
            let mut word_start = true;
            for c in text.chars() {
                if word_start && c.is_alphabetic() {
                    out.extend(c.to_uppercase());
                    word_start = false;
                    continue;
                }
                if c.is_whitespace() {
                    word_start = true;
                } else if c.is_alphanumeric() {
                    word_start = false;
                }
                out.push(c);
            }
            Cow::Owned(out)
        }
        Some(Case::None | Case::Smallcaps) | None => Cow::Borrowed(text),
    }
}

fn tag(name: &str) -> Result<Tag, EngineError> {
    let bytes: [u8; 4] = name
        .as_bytes()
        .try_into()
        .ok()
        .filter(|b: &[u8; 4]| b.is_ascii())
        .ok_or_else(|| EngineError::Theme(format!("`{name}` is not a four-letter OpenType tag")))?;
    Ok(Tag::from_bytes(bytes))
}

/// parley style properties for text set in `role`, with the node's settings on top.
fn style_props(
    theme: &Theme,
    role: &TextRole,
    spec: &TextSpec,
) -> Result<Vec<StyleProperty<'static, Ink>>, EngineError> {
    let stack = theme.family_stack(&role.family)?;
    let family =
        FontFamily::List(Cow::Owned(stack.into_iter().map(|n| FontFamilyName::Named(Cow::Owned(n))).collect()));

    // Features, later wins: family defaults, role, numeral style (node over role), node.
    let mut features: BTreeMap<String, u16> = match theme.families().get(&role.family) {
        Some(f) => crate::theme::features(f.features.as_ref())
            .map_err(|e| EngineError::Theme(format!("family `{}`: {e}", role.family)))?,
        None => BTreeMap::new(),
    };
    features.extend(role.features.iter().map(|(k, v)| (k.clone(), *v)));
    if let Some(numeric) = spec.numeric.or(role.numeric) {
        features.extend(numeric.features().map(|(k, v)| (k.to_string(), v)));
    }
    if role.case == Some(Case::Smallcaps) {
        features.insert("smcp".into(), 1);
    }
    features.extend(spec.features.iter().map(|(k, v)| (k.clone(), *v)));
    let features: Vec<FontFeature> =
        features.iter().map(|(k, v)| Ok(FontFeature::new(tag(k)?, *v))).collect::<Result<_, EngineError>>()?;

    // Variations: `opsz` from the role, else the size (CSS `font-optical-sizing: auto`),
    // then role and node axes. Weight goes through FontWeight so fontique sets `wght`
    // on variable fonts; an explicit `wght` here would fight it.
    let mut axes: BTreeMap<String, f32> = BTreeMap::new();
    axes.insert("opsz".into(), role.opsz.unwrap_or(role.size));
    axes.extend(role.axes.iter().map(|(k, v)| (k.clone(), *v)));
    axes.extend(spec.axes.iter().map(|(k, v)| (k.clone(), *v)));
    let variations: Vec<FontVariation> =
        axes.iter().map(|(k, v)| Ok(FontVariation::new(tag(k)?, *v))).collect::<Result<_, EngineError>>()?;

    let color_name = role.color.as_deref().unwrap_or("onSurface");
    let ink = theme.color(color_name)?.0;
    let locale = match &spec.lang {
        Some(lang) => Some(Language::parse(lang).map_err(|e| EngineError::Theme(format!("lang `{lang}`: {e}")))?),
        None => None,
    };

    Ok(vec![
        StyleProperty::FontFamily(family),
        StyleProperty::FontSize(role.size),
        StyleProperty::FontWeight(FontWeight::new(role.weight)),
        StyleProperty::FontFeatures(FontFeatures::List(Cow::Owned(features))),
        StyleProperty::FontVariations(FontVariations::List(Cow::Owned(variations))),
        StyleProperty::LetterSpacing(role.tracking * role.size),
        StyleProperty::LineHeight(LineHeight::FontSizeRelative(role.leading)),
        StyleProperty::Brush(ink),
        StyleProperty::Locale(locale),
        // Break only at UAX #14 opportunities: no emergency breaks inside words.
        StyleProperty::OverflowWrap(OverflowWrap::Normal),
        StyleProperty::WordBreak(WordBreak::Normal),
    ])
}

/// Quotation marks (Unicode `Quotation_Mark`) that hang when they open a line. The CJK
/// corner brackets and fullwidth forms are left out: JLREQ sets their spacing, not
/// hanging (PLAN 1.8).
fn is_hanging_quote(c: char) -> bool {
    matches!(c, '"' | '\'' | '«' | '»' | '\u{2018}'..='\u{201F}' | '‹' | '›' | '\u{2E42}')
}

/// What `hangingPunctuation` hangs past an aligned end edge besides quotation marks:
/// closing brackets, the stops and commas of CSS `hanging-punctuation: allow-end`, and
/// hyphens.
fn is_hanging_stop(c: char) -> bool {
    matches!(
        c,
        ')' | ']'
            | '}'
            | '.'
            | ','
            | '-'
            | '\u{2010}'
            | '\u{2011}'
            | '\u{060C}'
            | '\u{06D4}'
            | '\u{3001}'
            | '\u{3002}'
            | '\u{FF0C}'
            | '\u{FF0E}'
    )
}

/// What a line over `line` (a byte range of `text`) hangs: the advance of the quotation
/// marks it opens with, when they are set in the paragraph's direction and so sit on its
/// start edge. A quote that starts a ligature stays inside.
fn hang_start(layout: &Layout<Ink>, text: &str, line: Range<usize>, rtl: bool, hangs: impl Fn(char) -> bool) -> f32 {
    let mut hang = 0.0;
    let mut next = Cluster::from_byte_index(layout, line.start);
    while let Some(cluster) = next {
        let range = cluster.text_range();
        let quote = !range.is_empty() && text[range.clone()].chars().all(&hangs);
        if range.start >= line.end || !quote || cluster.is_rtl() != rtl || cluster.is_ligature_start() {
            break;
        }
        hang += cluster.advance();
        next = cluster.next_logical();
    }
    hang
}

/// What a line over `line` hangs past its end edge: the advance of the quotation marks it
/// closes with, before any trailing whitespace, set in the paragraph's direction. A quote
/// that ends a ligature stays inside.
fn hang_end(layout: &Layout<Ink>, text: &str, line: Range<usize>, rtl: bool, hangs: impl Fn(char) -> bool) -> f32 {
    let mut hang = 0.0;
    let mut next = line.end.checked_sub(1).and_then(|last| Cluster::from_byte_index(layout, last));
    let mut trailing = true;
    while let Some(cluster) = next {
        let range = cluster.text_range();
        if range.end <= line.start || range.is_empty() {
            break;
        }
        let chars = &text[range.clone()];
        next = cluster.previous_logical();
        if trailing && chars.chars().all(char::is_whitespace) {
            continue;
        }
        trailing = false;
        if !chars.chars().all(&hangs) || cluster.is_rtl() != rtl || cluster.is_ligature_continuation() {
            break;
        }
        hang += cluster.advance();
    }
    hang
}

/// Breaks with line `k` at most `max_width + hangs[k]` wide (`max_width` past the end of
/// `hangs`).
fn break_with(layout: &mut Layout<Ink>, max_width: f32, hangs: &[f32]) {
    let mut breaker = layout.break_lines();
    // parley requires every line width to stay within 1 cu of the layout width.
    breaker.state_mut().set_layout_max_advance(max_width + hangs.iter().copied().fold(0.0, f32::max));
    for k in 0.. {
        breaker.state_mut().set_line_max_advance(max_width + hangs.get(k).copied().unwrap_or(0.0));
        if breaker.break_next().is_none() {
            break;
        }
    }
    breaker.finish();
}

/// What breaking a paragraph reads besides its layout and its width.
#[derive(Clone, Copy)]
struct Breaking<'a> {
    text: &'a str,
    rtl: bool,
    room: Room<'a>,
    /// The paragraph's segments, measured once (see [`segments`]).
    segments: &'a [Segment],
    /// `minLastLineWords`.
    min_words: usize,
}

/// Greedy breaking at `max_width`, each line measured without the quotes it hangs at
/// `edge`, and with the hyphen it ends with, each paragraph's last line then held to
/// `min_words` words (see [`hold`]). Returns why it fell back to plain greedy breaking,
/// if it did.
///
/// Without soft hyphens, parley breaks. A line's hang depends on where it starts and
/// ends, which depends on the lines above, so each pass breaks with the hangs the previous
/// pass found. Line `k` breaks right once line `k - 1` has, so the hangs settle within one
/// pass per line; with no quote at an aligned edge, the first pass is plain greedy and the
/// last. With soft hyphens a line's room depends on whether it ends at one, which could
/// make those passes swing, so the lines are planned over measured segments instead, first
/// fit, and parley is handed one width per line.
fn greedy(layout: &mut Layout<Ink>, b: Breaking, max_width: f32) -> Option<&'static str> {
    let Breaking { text, rtl, room, segments, min_words } = b;
    if room.hyphenated(text) {
        let plan = hold(segments, first_fit(segments, max_width), min_words, max_width);
        if realize(layout, segments, &plan, max_width) {
            return None;
        }
        greedy_parley(layout, text, max_width, rtl, room.hang);
        return Some("parley did not break where the hyphenation plan said");
    }
    greedy_parley(layout, text, max_width, rtl, room.hang);
    // Without soft hyphens every segment that sets ink starts a word, so one is held.
    if min_words > 1
        && layout.len() > 1
        && let Some(plan) = plan_of(layout, segments)
    {
        let held = hold(segments, plan.clone(), min_words, max_width);
        if held != plan && !realize(layout, segments, &held, max_width) {
            // parley would not break there: greedy again, and the short last line stays.
            greedy_parley(layout, text, max_width, rtl, room.hang);
        }
    }
    None
}

fn greedy_parley(layout: &mut Layout<Ink>, text: &str, max_width: f32, rtl: bool, hang: Hang) {
    if !hang.possible(text) {
        layout.break_all_lines(Some(max_width));
        return;
    }
    let mut hangs: Vec<f32> = Vec::new();
    // Every line but an empty last one holds a byte, so this bound is never reached; it
    // keeps a change in parley from becoming a hang in the render path.
    for _ in 0..text.len() + 2 {
        break_with(layout, max_width, &hangs);
        let found: Vec<f32> = layout.lines().map(|line| hang.at(layout, text, line.text_range(), rtl)).collect();
        let settled = found.iter().enumerate().all(|(k, &hang)| hang == hangs.get(k).copied().unwrap_or(0.0));
        if settled {
            return;
        }
        hangs = found;
    }
}

/// Narrowest width at which greedy breaking keeps the line count it has at `max_width`,
/// the last line then held to `min_words` words.
fn balance(layout: &mut Layout<Ink>, b: Breaking, max_width: f32) -> Option<&'static str> {
    // Holding the last line never changes the line count, so the search skips it.
    let probe = Breaking { min_words: 0, ..b };
    let fallback = greedy(layout, probe, max_width);
    let lines = layout.len();
    if lines < 2 {
        return fallback;
    }
    let (mut lo, mut hi) = (0.0_f32, max_width);
    for _ in 0..BALANCE_STEPS {
        let mid = 0.5 * (lo + hi);
        greedy(layout, probe, mid);
        if layout.len() <= lines {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    greedy(layout, b, hi)
}

/// What a line has room for besides the measure: the marks it hangs at an aligned edge,
/// and, taking room away, the hyphen it ends with if it breaks at a soft hyphen.
#[derive(Clone, Copy)]
struct Room<'h> {
    hang: Hang,
    hyphens: &'h [Hyphen],
}

impl<'h> Room<'h> {
    /// Whether `text` has soft hyphens a line could end with.
    fn hyphenated(self, text: &str) -> bool {
        !self.hyphens.is_empty() && text.contains(SHY)
    }

    /// The hyphen a line ending at byte `end` draws: it broke at a soft hyphen.
    fn hyphen_at(self, text: &str, end: usize) -> Option<&'h Hyphen> {
        hyphen_at(self.hyphens, text, end)
    }
}

/// The hyphen a line ending at byte `end` of `text` draws, if it broke at a soft hyphen.
fn hyphen_at<'h>(hyphens: &'h [Hyphen], text: &str, end: usize) -> Option<&'h Hyphen> {
    if !text[..end].ends_with(SHY) {
        return None;
    }
    hyphens.iter().find(|h| h.range.contains(&(end - 1)))
}

/// An unbreakable stretch of text between two break opportunities.
struct Segment {
    /// Advance including trailing whitespace (what it adds mid-line).
    full: f32,
    /// Advance without trailing whitespace (what it adds at the end of a line).
    bare: f32,
    /// What a line starting with this segment hangs at its start edge (see [`hang_start`]).
    hang: f32,
    /// What a line ending with this segment hangs at its end edge (see [`hang_end`]).
    hang_end: f32,
    /// The hyphen a line ending with this segment draws: it ends at a soft hyphen.
    hyphen: f32,
    /// It ends at a soft hyphen, so the next segment goes on with its word.
    soft: bool,
    /// It starts a word: it sets ink, and the segment before it did not end at a soft
    /// hyphen (see [`words`]).
    word: bool,
    /// A hard line break ends it, so a line ends with it.
    hard: bool,
    text: Range<usize>,
}

/// Every segment of the paragraph. At (almost) zero width parley breaks at every UAX #14
/// opportunity, so each probe line is exactly one segment. Shaping happens before
/// breaking, so segment widths add up to line widths exactly.
fn segments(layout: &mut Layout<Ink>, text: &str, rtl: bool, room: Room) -> Vec<Segment> {
    layout.break_all_lines(Some(PROBE_WIDTH));
    let hang = room.hang;
    let mut joined = false;
    layout
        .lines()
        .map(|line| {
            let m = line.metrics();
            let range = line.text_range();
            let soft = text[..range.end].ends_with(SHY);
            let word = !joined && text[range.clone()].chars().any(sets_ink);
            joined = soft;
            Segment {
                full: m.advance,
                bare: m.advance - m.trailing_whitespace,
                hang: if hang.edge == Edge::Start { hang.at(layout, text, range.clone(), rtl) } else { 0.0 },
                hang_end: if hang.edge == Edge::End { hang.at(layout, text, range.clone(), rtl) } else { 0.0 },
                // A hyphen that hangs takes no room, and right-to-left lines draw none.
                hyphen: (room.hyphen_at(text, range.end))
                    .filter(|_| !rtl && !hang.hangs_hyphens())
                    .map_or(0.0, |h| h.advance),
                soft,
                word,
                hard: line.break_reason() == BreakReason::Explicit,
                text: range,
            }
        })
        .collect()
}

/// Whether `c` draws something: not whitespace, and not one of the default-ignorable
/// characters a font sets invisibly (soft hyphens, zero-width spaces and joiners, bidi
/// controls, the word joiner, the byte order mark).
fn sets_ink(c: char) -> bool {
    !c.is_whitespace()
        && !matches!(
            c,
            '\u{AD}'
                | '\u{34F}'
                | '\u{61C}'
                | '\u{200B}'..='\u{200F}'
                | '\u{202A}'..='\u{202E}'
                | '\u{2060}'..='\u{206F}'
                | '\u{FEFF}'
        )
}

/// The paragraph's words: the text between its break opportunities (UAX #14), so a
/// word keeps the punctuation and space after it, and a word hyphenation splits counts
/// once. `minLastLineWords` counts them and `split: words` animates them. Text that sets
/// no ink (leading space) is in no word.
fn words(segments: &[Segment]) -> Vec<Range<usize>> {
    let mut words: Vec<Range<usize>> = Vec::new();
    for (k, s) in segments.iter().enumerate() {
        match words.last_mut() {
            _ if s.word => words.push(s.text.clone()),
            Some(word) if k > 0 && segments[k - 1].soft => word.end = s.text.end,
            _ => {}
        }
    }
    words
}

/// How many words start in the line `segments[line]`.
fn word_count(segments: &[Segment], line: &Range<usize>) -> usize {
    segments[line.clone()].iter().filter(|s| s.word).count()
}

/// parley's lines as a plan over `segments`; `None` if a line does not end where a
/// segment does.
fn plan_of(layout: &Layout<Ink>, segments: &[Segment]) -> Option<Vec<Range<usize>>> {
    let mut plan = Vec::new();
    let mut k = 0;
    for line in layout.lines() {
        let (start, end) = (k, line.text_range().end);
        while k < segments.len() && segments[k].text.end <= end {
            k += 1;
        }
        if k == start || segments[k - 1].text.end != end {
            return None;
        }
        plan.push(start..k);
    }
    (k == segments.len()).then_some(plan)
}

/// The paragraphs of `plan`, as ranges of its lines: a hard line break ends one.
fn paragraphs(segments: &[Segment], plan: &[Range<usize>]) -> Vec<Range<usize>> {
    let mut paragraphs = Vec::new();
    let mut start = 0;
    for (k, line) in plan.iter().enumerate() {
        if k + 1 == plan.len() || segments[line.end - 1].hard {
            paragraphs.push(start..k + 1);
            start = k + 1;
        }
    }
    paragraphs
}

/// Each paragraph's last line given as many words as it can take, up to `min_words`, by
/// moving the break above it back as few segments as that takes, while both lines still
/// fit. A word's tail after a hyphen is not a word, so even at `min_words: 1` a last line
/// that is only that tail takes its word's head (TeX's `\finalhyphendemerits`, as a rule).
fn hold(segments: &[Segment], mut plan: Vec<Range<usize>>, min_words: usize, max_width: f32) -> Vec<Range<usize>> {
    let fits = |line: &Range<usize>| {
        let (width, room) = line_fit(segments, line, max_width);
        width <= room
    };
    for lines in paragraphs(segments, &plan) {
        let k = lines.end - 1;
        let mut most = word_count(segments, &plan[k]);
        if lines.len() < 2 || most >= min_words {
            continue;
        }
        let (above, last) = (plan[k - 1].clone(), plan[k].clone());
        let mut best = None;
        // Starting the last line earlier only widens it (see `pretty`).
        for s in (above.start + 1..last.start).rev() {
            let (a, b) = (above.start..s, s..last.end);
            if !fits(&b) {
                break;
            }
            let words = word_count(segments, &b);
            if words > most && fits(&a) {
                (most, best) = (words, Some(s));
                if words >= min_words {
                    break;
                }
            }
        }
        if let Some(s) = best {
            (plan[k - 1], plan[k]) = (above.start..s, s..last.end);
        }
    }
    plan
}

/// Whether a paragraph of two lines or more ends in a line that starts fewer than
/// `min_words` words, although the paragraph has that many: the widow breaking could not
/// hold (lint W200).
fn widowed(layout: &Layout<Ink>, segments: &[Segment], min_words: usize) -> bool {
    let Some(plan) = plan_of(layout, segments) else { return false };
    paragraphs(segments, &plan).into_iter().any(|lines| {
        let last = &plan[lines.end - 1];
        let all = plan[lines.start].start..last.end;
        lines.len() > 1 && word_count(segments, &all) >= min_words && word_count(segments, last) < min_words
    })
}

/// The width line `segments[line]` takes, its hyphen included, and the room it has.
fn line_fit(segments: &[Segment], line: &Range<usize>, max_width: f32) -> (f32, f32) {
    let last = &segments[line.end - 1];
    let width = segments[line.start..line.end - 1].iter().map(|s| s.full).sum::<f32>() + last.bare + last.hyphen;
    (width, max_width + segments[line.start].hang + last.hang_end)
}

/// Lines by first fit: each takes every segment that still fits, and at least one.
fn first_fit(segments: &[Segment], max_width: f32) -> Vec<Range<usize>> {
    let mut plan = Vec::new();
    let mut start = 0;
    while start < segments.len() {
        let mut end = start + 1;
        while end < segments.len() && !segments[end - 1].hard {
            let (width, room) = line_fit(segments, &(start..end + 1), max_width);
            if width > room {
                break;
            }
            end += 1;
        }
        plan.push(start..end);
        start = end;
    }
    plan
}

/// Hand parley one width per planned line and check it broke there. parley requires line
/// widths to stay within 1 cu of the layout width, so a single overflowing segment is
/// capped (it cannot break anyway). A hyphen is drawn after the line, so it takes no part
/// of parley's width.
fn realize(layout: &mut Layout<Ink>, segments: &[Segment], plan: &[Range<usize>], max_width: f32) -> bool {
    let hung = |line: &Range<usize>| segments[line.start].hang + segments[line.end - 1].hang_end;
    let most = plan.iter().map(hung).fold(0.0, f32::max);
    let mut breaker = layout.break_lines();
    breaker.state_mut().set_layout_max_advance(max_width + most);
    for line in plan {
        let (width, room) = line_fit(segments, line, max_width);
        let width = width - segments[line.end - 1].hyphen;
        breaker.state_mut().set_line_max_advance((width + FIT_SLACK).min(room + FIT_SLACK));
        breaker.break_next();
    }
    breaker.break_remaining(max_width);
    let broke: Vec<Range<usize>> = layout.lines().map(|l| l.text_range()).collect();
    let planned: Vec<Range<usize>> =
        plan.iter().map(|l| segments[l.start].text.start..segments[l.end - 1].text.end).collect();
    broke == planned
}

/// Minimum-raggedness breaking with the last line held to `min_words` words, each line
/// measured without the quotes it hangs and with the hyphen it ends with; a line ending
/// in a hyphen costs [`HYPHEN_PENALTY`] more. A last line that cannot take `min_words`
/// words takes as many as it can, as [`hold`] does. Returns false when parley's breaker
/// did not reproduce the plan.
fn pretty(layout: &mut Layout<Ink>, b: Breaking, max_width: f32) -> bool {
    let segments = b.segments;
    if segments.len() < 2 {
        greedy(layout, b, max_width);
        return true;
    }
    // At `min_words: 0` nothing holds the last line, so some plan is found.
    let plan = (0..=b.min_words).rev().find_map(|m| plan_pretty(segments, max_width, m));
    plan.is_some_and(|plan| realize(layout, segments, &plan, max_width))
}

/// The `pretty` plan: the cheapest lines over `segments` whose last line starts at least
/// `min_words` words (when the paragraph has that many), or `None` if no last line can.
fn plan_pretty(segments: &[Segment], max_width: f32, min_words: usize) -> Option<Vec<Range<usize>>> {
    let n = segments.len();
    // words[i]: how many words start in segments[i..].
    let mut words = vec![0_usize; n + 1];
    for k in (0..n).rev() {
        words[k] = words[k + 1] + usize::from(segments[k].word);
    }
    // best[j]: cheapest way to set segments[..j]; start[j]: where its last line starts.
    let mut best = vec![f32::INFINITY; n + 1];
    let mut start = vec![0_usize; n + 1];
    best[0] = 0.0;
    for j in 1..=n {
        let last = &segments[j - 1];
        let mut leading = 0.0_f32; // full advances of segments[i..j - 1]
        for i in (0..j).rev() {
            if i < j - 1 {
                leading += segments[i].full;
            }
            let width = leading + last.bare + last.hyphen;
            // The quotes a line hangs take no room. A segment hangs no more than its own
            // advance, so starting a line earlier still only widens it.
            let room = max_width + segments[i].hang + last.hang_end;
            if width > room && i < j - 1 {
                break; // more segments only widen the line
            }
            let cost = if j == n {
                if words[i] < min_words && words[0] >= min_words {
                    continue; // the widow `pretty` exists to prevent
                }
                LINE_PENALTY
            } else {
                let slack = ((room - width) / max_width).max(0.0);
                let hyphen = if last.hyphen > 0.0 { HYPHEN_PENALTY } else { 0.0 };
                LINE_PENALTY + 1000.0 * slack * slack + hyphen
            };
            if best[i] + cost < best[j] {
                best[j] = best[i] + cost;
                start[j] = i;
            }
        }
    }
    if !best[n].is_finite() {
        return None;
    }
    let mut plan: Vec<Range<usize>> = Vec::new();
    let mut j = n;
    while j > 0 {
        plan.push(start[j]..j);
        j = start[j];
    }
    plan.reverse();
    Some(plan)
}

/// How far the cluster on a line's aligned `edge` reaches past it with optical margins:
/// a fraction of its advance by [`protrusion_of`], on the side that faces the edge.
fn protrusion(layout: &Layout<Ink>, text: &str, line: Range<usize>, rtl: bool, edge: Edge) -> f32 {
    let cluster = match edge {
        Edge::Start => Cluster::from_byte_index(layout, line.start),
        _ => {
            // The last cluster before any trailing whitespace.
            let mut next = line.end.checked_sub(1).and_then(|last| Cluster::from_byte_index(layout, last));
            loop {
                match next {
                    Some(c)
                        if c.text_range().start >= line.start
                            && text[c.text_range()].chars().all(char::is_whitespace) =>
                    {
                        next = c.previous_logical();
                    }
                    other => break other,
                }
            }
        }
    };
    let Some(cluster) = cluster.filter(|c| c.text_range().start >= line.start && c.is_rtl() == rtl) else {
        return 0.0;
    };
    let Some(c) = text[cluster.text_range()].chars().next() else { return 0.0 };
    let (left, right) = protrusion_of(c);
    // The side facing the edge: a line's start is its left in left-to-right text.
    let left_side = matches!((edge, rtl), (Edge::Start, false) | (Edge::End, true));
    (if left_side { left } else { right }) * cluster.advance()
}

/// How much of a character's advance protrudes past an aligned edge, `(left, right)`:
/// microtype's defaults for Latin text (pdfTeX's `\rpcode`/`\lpcode` in thousandths).
fn protrusion_of(c: char) -> (f32, f32) {
    match c {
        'A' | 'T' | 'V' | 'W' | 'X' | 'Y' | 'v' | 'w' | 'x' | 'y' => (0.05, 0.05),
        'J' => (0.05, 0.0),
        'F' | 'K' | 'L' | 'k' | 'r' => (0.0, 0.05),
        't' => (0.0, 0.07),
        '.' | '\u{3002}' => (0.0, 0.7),
        ',' | '\u{3001}' | '\u{060C}' => (0.0, 0.5),
        ':' => (0.0, 0.5),
        ';' => (0.0, 0.3),
        '!' | '?' => (0.0, 0.1),
        '-' | '\u{2010}' | '\u{2011}' => (0.0, 0.5),
        '\u{2013}' => (0.2, 0.2),
        '\u{2014}' => (0.15, 0.15),
        _ => (0.0, 0.0),
    }
}

/// How a paragraph was broken and how its lines sit.
struct Paragraph {
    wrap: Wrap,
    fallback: Option<&'static str>,
    rtl: bool,
    align: TextAlign,
    hang: Hang,
    /// `opticalMargins`.
    optical: bool,
    /// The box the lines align across.
    width: f32,
    /// Where lines break: `width`, or the measure if that is narrower.
    measure: f32,
    /// Its words (see [`words`]).
    words: Vec<Range<usize>>,
    /// A last line shorter than `minLastLineWords` that breaking could not hold.
    widow: bool,
}

impl Paragraph {
    /// One unaligned line: what measuring a single glyph needs.
    fn plain() -> Paragraph {
        let hang = Hang { edge: Edge::Neither, punctuation: false };
        Paragraph {
            wrap: Wrap::Greedy,
            fallback: None,
            rtl: false,
            align: TextAlign::Start,
            hang,
            optical: false,
            width: 0.0,
            measure: 0.0,
            words: Vec::new(),
            widow: false,
        }
    }
}

fn read_layout(
    layout: &Layout<Ink>,
    text: String,
    fonts: &BundleFonts,
    hyphens: &[Hyphen],
    p: Paragraph,
) -> Result<TextLayout, EngineError> {
    let text = text.as_str();
    let mut lines = Vec::new();
    let mut runs = Vec::new();
    let mut synthesized = false;
    let mut top = 0.0_f32;
    for (index, line) in layout.lines().enumerate() {
        let m = line.metrics();
        let hung = p.hang.at(layout, text, line.text_range(), p.rtl);
        let (hang, mut hang_end) = match p.hang.edge {
            Edge::Start => (hung, 0.0),
            Edge::End => (0.0, hung),
            Edge::Neither => (0.0, 0.0),
        };
        // parley set the line's ink from x = 0; trailing whitespace hangs past its end.
        let ink = m.advance - m.trailing_whitespace;
        // A line that broke at a soft hyphen draws a hyphen after its ink (left-to-right
        // lines; no right-to-left script here hyphenates). With hanging punctuation at an
        // aligned end edge it hangs there; otherwise it is part of the line.
        let hyphen = hyphen_at(hyphens, text, line.text_range().end).filter(|_| !p.rtl);
        let hyphen_width = hyphen.map_or(0.0, |h| h.advance);
        if hyphen.is_some() && p.hang.hangs_hyphens() {
            hang_end += hyphen_width;
        }
        let width = ink + hyphen_width - hang - hang_end;
        // Where the part inside the measure starts. Hung marks sit outside it, at the
        // logical start or end: the left or the right of the line by its direction.
        let x = match (p.align, p.rtl) {
            (TextAlign::Start, false) | (TextAlign::End, true) => 0.0,
            (TextAlign::Start, true) | (TextAlign::End, false) => p.width - width,
            (TextAlign::Center, _) => 0.5 * (p.width - width),
        };
        let left_hang = if p.rtl { hang_end } else { hang };
        // Optical margins: what sits on the aligned edge, when nothing hangs there, moves
        // part of its width past it. Outward is left at a left edge, right at a right one.
        let optical = match p.hang.edge {
            Edge::Neither => 0.0,
            _ if !p.optical || hang + hang_end > 0.0 => 0.0,
            // A drawn hyphen is what sits on the end edge.
            Edge::End if hyphen.is_some() => protrusion_of('-').1 * hyphen_width,
            edge => protrusion(layout, text, line.text_range(), p.rtl, edge),
        };
        let left_edge = matches!((p.hang.edge, p.rtl), (Edge::Start, false) | (Edge::End, true));
        let shift = x - left_hang + if left_edge { -optical } else { optical };
        let (mut cap_height, mut x_height) = (None, None);
        // The glyphs of one of the line's runs that earlier glyph runs (style changes)
        // took, as parley counts them: its glyphs in visual cluster order.
        let mut taken: (Range<usize>, usize) = (0..0, 0);
        let first_run = runs.len();
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
            let run = glyph_run.run();
            let synthesis = run.synthesis();
            synthesized |= synthesis.embolden() || synthesis.skew().is_some();
            if cap_height.is_none() {
                cap_height = run.metrics().cap_height;
                x_height = run.metrics().x_height;
            }
            let glyphs: Vec<Glyph> =
                glyph_run.positioned_glyphs().map(|g| Glyph { id: g.id, x: g.x + shift, y: g.y }).collect();
            let advances: Vec<f32> = glyph_run.positioned_glyphs().map(|g| g.advance).collect();
            if taken.0 != run.cluster_range() {
                taken = (run.cluster_range(), 0);
            }
            let clusters: Vec<usize> = run
                .visual_clusters()
                .flat_map(|c| {
                    let start = c.text_range().start;
                    c.glyphs().map(move |_| start)
                })
                .skip(taken.1)
                .take(glyphs.len())
                .collect();
            taken.1 += glyphs.len();
            runs.push(GlyphRun {
                font: fonts.font_ref(run.font())?,
                size: run.font_size(),
                coords: run.normalized_coords().to_vec(),
                color: Color(glyph_run.style().brush),
                glyphs,
                clusters,
                advances,
                line: index,
            });
        }
        if let Some(h) = hyphen {
            let mut run = h.run.clone();
            for g in &mut run.glyphs {
                (g.x, g.y) = (g.x + ink + shift, g.y + m.baseline);
            }
            // It goes with the letter before the soft hyphen.
            let shy = line.text_range().end - SHY.len_utf8();
            let before = runs[first_run..].iter().flat_map(|r| r.clusters.iter().copied()).filter(|&c| c < shy).max();
            run.clusters = vec![before.unwrap_or(shy); run.glyphs.len()];
            run.line = index;
            runs.push(run);
        }
        // parley's `block_min_coord` is the top of the ascent box, which sits above the
        // line box when leading is tighter than the font's ascent + descent; the line
        // box itself starts where the previous one ended.
        lines.push(LineBox {
            top,
            baseline: m.baseline,
            height: m.line_height,
            ascent: m.ascent,
            descent: m.descent,
            width,
            x,
            hang,
            hang_end,
            optical,
            hyphen: hyphen.is_some(),
            text: line.text_range(),
            cap_height,
            x_height,
        });
        top += m.line_height;
    }
    let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    Ok(TextLayout {
        text: text.to_string(),
        lines,
        runs,
        width,
        height: layout.height(),
        wrap: p.wrap,
        fallback: p.fallback,
        words: p.words,
        widow: p.widow,
        rtl: p.rtl,
        synthesized,
        align: p.align,
        measure: p.measure,
    })
}
