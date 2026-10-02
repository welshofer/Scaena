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
//! so in [`TextLayout::fallback`].
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

use crate::EngineError;
use crate::fonts::BundleFonts;
use crate::theme::{Numeric, TextBox, TextRole, Theme, Wrap};
use parley::setting::Tag;
use parley::{
    Alignment, AlignmentOptions, Cluster, FontFamily, FontFamilyName, FontFeature, FontFeatures, FontVariation,
    FontVariations, FontWeight, Language, Layout, LayoutContext, LineHeight, OverflowWrap, PositionedLayoutItem,
    StyleProperty, WordBreak,
};
use scaena_core::displaylist::{Color, FontRef, Glyph};
use scaena_core::model::theme::Case;
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

/// A shaped, broken, positioned paragraph. Coordinates are canvas units relative to
/// the text box's top-left corner; y grows down.
#[derive(Debug, Clone, PartialEq)]
pub struct TextLayout {
    pub lines: Vec<LineBox>,
    pub runs: Vec<GlyphRun>,
    /// Widest line, trailing whitespace and hung quotes excluded.
    pub width: f32,
    pub height: f32,
    /// The breaking actually used.
    pub wrap: Wrap,
    /// Why the requested `pretty` or `balance` fell back to greedy, if it did.
    pub fallback: Option<&'static str>,
    pub rtl: bool,
    /// Some run asked for faux bold or oblique, which the display list cannot express.
    pub synthesized: bool,
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
    /// Advance of the quotation marks hung outside the end edge (end-aligned text).
    pub hang_end: f32,
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
        let cased: Vec<Cow<str>> = spec.spans.iter().map(|s| set_case(&s.text, s.style.case)).collect();
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

        let rtl = layout.is_rtl();
        let edge = Edge::from(spec.align);
        let requested = spec.wrap.unwrap_or(base.wrap);
        let mut fallback = match requested {
            Wrap::Greedy => None,
            _ if text.contains(['\n', '\r', '\u{2028}', '\u{2029}']) => Some("hard line break"),
            _ => None,
        };
        let min_words = spec.min_last_line_words.or(base.min_last_line_words).unwrap_or(1) as usize;
        let wrap = match (requested, fallback) {
            (_, Some(_)) | (Wrap::Greedy, _) => {
                greedy(&mut layout, &text, max_width, rtl, edge);
                Wrap::Greedy
            }
            (Wrap::Balance, None) => {
                balance(&mut layout, &text, max_width, rtl, edge);
                Wrap::Balance
            }
            (Wrap::Pretty, None) => {
                if pretty(&mut layout, &text, max_width, min_words, rtl, edge) {
                    Wrap::Pretty
                } else {
                    fallback = Some("parley did not break where the pretty plan said");
                    greedy(&mut layout, &text, max_width, rtl, edge);
                    Wrap::Greedy
                }
            }
        };
        // Every line's ink from x = 0, in either direction; `read_layout` places it.
        layout.align(Alignment::Left, AlignmentOptions { align_when_overflowing: true });
        read_layout(&layout, &text, fonts, Paragraph { wrap, fallback, rtl, align: spec.align, width })
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

/// What a line over `line` (a byte range of `text`) hangs: the advance of the quotation
/// marks it opens with, when they are set in the paragraph's direction and so sit on its
/// start edge. A quote that starts a ligature stays inside.
fn hang_at(layout: &Layout<Ink>, text: &str, line: Range<usize>, rtl: bool) -> f32 {
    let mut hang = 0.0;
    let mut next = Cluster::from_byte_index(layout, line.start);
    while let Some(cluster) = next {
        let range = cluster.text_range();
        let quote = !range.is_empty() && text[range.clone()].chars().all(is_hanging_quote);
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
fn hang_end_at(layout: &Layout<Ink>, text: &str, line: Range<usize>, rtl: bool) -> f32 {
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
        if !chars.chars().all(is_hanging_quote) || cluster.is_rtl() != rtl || cluster.is_ligature_continuation() {
            break;
        }
        hang += cluster.advance();
    }
    hang
}

/// What a line over `line` hangs outside the aligned `edge`.
fn hang_of(layout: &Layout<Ink>, text: &str, line: Range<usize>, rtl: bool, edge: Edge) -> f32 {
    match edge {
        Edge::Start => hang_at(layout, text, line, rtl),
        Edge::End => hang_end_at(layout, text, line, rtl),
        Edge::Neither => 0.0,
    }
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

/// Greedy breaking at `max_width`, each line measured without the quotes it hangs at
/// `edge`.
///
/// A line's hang depends on where it starts and ends, which depends on the lines above,
/// so each pass breaks with the hangs the previous pass found. Line `k` breaks right once
/// line `k - 1` has, so the hangs settle within one pass per line; with no quote at an
/// aligned edge, the first pass is plain greedy and the last.
fn greedy(layout: &mut Layout<Ink>, text: &str, max_width: f32, rtl: bool, edge: Edge) {
    if edge == Edge::Neither || !text.contains(is_hanging_quote) {
        layout.break_all_lines(Some(max_width));
        return;
    }
    let mut hangs: Vec<f32> = Vec::new();
    // Every line but an empty last one holds a byte, so this bound is never reached; it
    // keeps a change in parley from becoming a hang in the render path.
    for _ in 0..text.len() + 2 {
        break_with(layout, max_width, &hangs);
        let found: Vec<f32> = layout.lines().map(|line| hang_of(layout, text, line.text_range(), rtl, edge)).collect();
        let settled = found.iter().enumerate().all(|(k, &hang)| hang == hangs.get(k).copied().unwrap_or(0.0));
        if settled {
            return;
        }
        hangs = found;
    }
}

/// Narrowest width at which greedy breaking keeps the line count it has at `max_width`.
fn balance(layout: &mut Layout<Ink>, text: &str, max_width: f32, rtl: bool, edge: Edge) {
    greedy(layout, text, max_width, rtl, edge);
    let lines = layout.len();
    if lines < 2 {
        return;
    }
    let (mut lo, mut hi) = (0.0_f32, max_width);
    for _ in 0..BALANCE_STEPS {
        let mid = 0.5 * (lo + hi);
        greedy(layout, text, mid, rtl, edge);
        if layout.len() <= lines {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    greedy(layout, text, hi, rtl, edge);
}

/// An unbreakable stretch of text between two break opportunities.
struct Segment {
    /// Advance including trailing whitespace (what it adds mid-line).
    full: f32,
    /// Advance without trailing whitespace (what it adds at the end of a line).
    bare: f32,
    /// What a line starting with this segment hangs at its start edge (see [`hang_at`]).
    hang: f32,
    /// What a line ending with this segment hangs at its end edge (see [`hang_end_at`]).
    hang_end: f32,
    text: Range<usize>,
}

/// Minimum-raggedness breaking with the last line held to `min_words` segments, each
/// line measured without the quotes it hangs. Returns false when parley's breaker did
/// not reproduce the plan.
fn pretty(layout: &mut Layout<Ink>, text: &str, max_width: f32, min_words: usize, rtl: bool, edge: Edge) -> bool {
    // Probe: at (almost) zero width parley breaks at every UAX #14 opportunity, so each
    // probe line is exactly one segment. Shaping happens before breaking, so segment
    // widths add up to line widths exactly.
    layout.break_all_lines(Some(PROBE_WIDTH));
    let segments: Vec<Segment> = layout
        .lines()
        .map(|line| {
            let m = line.metrics();
            Segment {
                full: m.advance,
                bare: m.advance - m.trailing_whitespace,
                hang: if edge == Edge::Start { hang_at(layout, text, line.text_range(), rtl) } else { 0.0 },
                hang_end: if edge == Edge::End { hang_end_at(layout, text, line.text_range(), rtl) } else { 0.0 },
                text: line.text_range(),
            }
        })
        .collect();
    let n = segments.len();
    if n < 2 {
        greedy(layout, text, max_width, rtl, edge);
        return true;
    }

    // best[j]: cheapest way to set segments[..j]; start[j]: where its last line starts.
    let mut best = vec![f32::INFINITY; n + 1];
    let mut start = vec![0_usize; n + 1];
    best[0] = 0.0;
    for j in 1..=n {
        let mut leading = 0.0_f32; // full advances of segments[i..j - 1]
        for i in (0..j).rev() {
            if i < j - 1 {
                leading += segments[i].full;
            }
            let width = leading + segments[j - 1].bare;
            // The quotes a line hangs take no room. A segment hangs no more than its own
            // advance, so starting a line earlier still only widens it.
            let room = max_width + segments[i].hang + segments[j - 1].hang_end;
            if width > room && i < j - 1 {
                break; // more segments only widen the line
            }
            let cost = if j == n {
                if j - i < min_words && n >= min_words {
                    continue; // the widow `pretty` exists to prevent
                }
                LINE_PENALTY
            } else {
                let slack = ((room - width) / max_width).max(0.0);
                LINE_PENALTY + 1000.0 * slack * slack
            };
            if best[i] + cost < best[j] {
                best[j] = best[i] + cost;
                start[j] = i;
            }
        }
    }
    if !best[n].is_finite() {
        return false;
    }
    let mut plan: Vec<Range<usize>> = Vec::new();
    let mut j = n;
    while j > 0 {
        plan.push(start[j]..j);
        j = start[j];
    }
    plan.reverse();

    // Realize: one width per planned line. parley requires line widths to stay within
    // 1 cu of the layout width, so a single overflowing segment is capped (it cannot
    // break anyway).
    let hung = |line: &Range<usize>| segments[line.start].hang + segments[line.end - 1].hang_end;
    let most = plan.iter().map(hung).fold(0.0, f32::max);
    let mut breaker = layout.break_lines();
    breaker.state_mut().set_layout_max_advance(max_width + most);
    for line in &plan {
        let width: f32 =
            segments[line.start..line.end - 1].iter().map(|s| s.full).sum::<f32>() + segments[line.end - 1].bare;
        let room = max_width + hung(line);
        breaker.state_mut().set_line_max_advance((width + FIT_SLACK).min(room + FIT_SLACK));
        breaker.break_next();
    }
    breaker.break_remaining(max_width);
    let broke: Vec<Range<usize>> = layout.lines().map(|l| l.text_range()).collect();
    let planned: Vec<Range<usize>> =
        plan.iter().map(|l| segments[l.start].text.start..segments[l.end - 1].text.end).collect();
    broke == planned
}

/// How a paragraph was broken and how its lines sit.
struct Paragraph {
    wrap: Wrap,
    fallback: Option<&'static str>,
    rtl: bool,
    align: TextAlign,
    /// The box the lines align across.
    width: f32,
}

fn read_layout(layout: &Layout<Ink>, text: &str, fonts: &BundleFonts, p: Paragraph) -> Result<TextLayout, EngineError> {
    let edge = Edge::from(p.align);
    let mut lines = Vec::new();
    let mut runs = Vec::new();
    let mut synthesized = false;
    let mut top = 0.0_f32;
    for (index, line) in layout.lines().enumerate() {
        let m = line.metrics();
        let (hang, hang_end) = match edge {
            Edge::Start => (hang_at(layout, text, line.text_range(), p.rtl), 0.0),
            Edge::End => (0.0, hang_end_at(layout, text, line.text_range(), p.rtl)),
            Edge::Neither => (0.0, 0.0),
        };
        // parley set the line's ink from x = 0; trailing whitespace hangs past its end.
        let ink = m.advance - m.trailing_whitespace;
        let width = ink - hang - hang_end;
        // Where the part inside the measure starts. Hung marks sit outside it, at the
        // logical start or end: the left or the right of the line by its direction.
        let x = match (p.align, p.rtl) {
            (TextAlign::Start, false) | (TextAlign::End, true) => 0.0,
            (TextAlign::Start, true) | (TextAlign::End, false) => p.width - width,
            (TextAlign::Center, _) => 0.5 * (p.width - width),
        };
        let left_hang = if p.rtl { hang_end } else { hang };
        let shift = x - left_hang;
        let (mut cap_height, mut x_height) = (None, None);
        for item in line.items() {
            let PositionedLayoutItem::GlyphRun(glyph_run) = item else { continue };
            let run = glyph_run.run();
            let synthesis = run.synthesis();
            synthesized |= synthesis.embolden() || synthesis.skew().is_some();
            if cap_height.is_none() {
                cap_height = run.metrics().cap_height;
                x_height = run.metrics().x_height;
            }
            runs.push(GlyphRun {
                font: fonts.font_ref(run.font())?,
                size: run.font_size(),
                coords: run.normalized_coords().to_vec(),
                color: Color(glyph_run.style().brush),
                glyphs: glyph_run.positioned_glyphs().map(|g| Glyph { id: g.id, x: g.x + shift, y: g.y }).collect(),
                line: index,
            });
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
            text: line.text_range(),
            cap_height,
            x_height,
        });
        top += m.line_height;
    }
    let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    Ok(TextLayout {
        lines,
        runs,
        width,
        height: layout.height(),
        wrap: p.wrap,
        fallback: p.fallback,
        rtl: p.rtl,
        synthesized,
    })
}
