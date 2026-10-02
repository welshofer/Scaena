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
//! `balance` and `pretty` fall back to greedy for right-to-left paragraphs and for
//! text with hard line breaks, and say so in [`TextLayout::fallback`]: parley aligns
//! lines broken at per-line widths inside those narrower widths, which puts RTL
//! lines at the wrong edge (PLAN 1.8 lifts this).
//!
//! Hanging quotes: quotation marks that open a line hang outside its start edge, in
//! every role (SPEC §3.5). All three breakings measure a line without them, so they
//! take nothing from the measure, and the letter after them sits on the edge. Role
//! `measure`, hyphenation, the rest of hanging punctuation (`hangingPunctuation`),
//! and optical margins are PLAN 1.8.

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
    /// Advance without trailing whitespace and without `hang`: what sits inside the measure.
    pub width: f32,
    /// Advance of the quotation marks hung outside the start edge: they sit at `-hang..0`
    /// on a left-to-right line, past the right edge of the measure on a right-to-left one.
    pub hang: f32,
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
        }
    }
}

impl TextEngine {
    pub fn new() -> Self {
        Self::default()
    }

    /// Shape `spec` with the bundle's fonts and break it into lines at most `max_width` wide.
    pub fn layout(
        &mut self,
        fonts: &mut BundleFonts,
        theme: &Theme,
        spec: &TextSpec,
        max_width: f32,
    ) -> Result<TextLayout, EngineError> {
        let base = &spec.role;
        let text: String = spec.spans.iter().map(|s| s.text.as_str()).collect();
        let mut layout = {
            let mut builder = self.lcx.ranged_builder(&mut fonts.cx, &text, 1.0, false);
            for prop in style_props(theme, base, spec)? {
                builder.push_default(prop);
            }
            let mut at = 0;
            for span in &spec.spans {
                let range = at..at + span.text.len();
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
        let requested = spec.wrap.unwrap_or(base.wrap);
        let mut fallback = match requested {
            Wrap::Greedy => None,
            _ if rtl => Some("right-to-left paragraph"),
            _ if text.contains(['\n', '\r', '\u{2028}', '\u{2029}']) => Some("hard line break"),
            _ => None,
        };
        let min_words = spec.min_last_line_words.or(base.min_last_line_words).unwrap_or(1) as usize;
        let wrap = match (requested, fallback) {
            (_, Some(_)) | (Wrap::Greedy, _) => {
                greedy(&mut layout, &text, max_width, rtl);
                Wrap::Greedy
            }
            (Wrap::Balance, None) => {
                balance(&mut layout, &text, max_width, rtl);
                Wrap::Balance
            }
            (Wrap::Pretty, None) => {
                if pretty(&mut layout, &text, max_width, min_words, rtl) {
                    Wrap::Pretty
                } else {
                    fallback = Some("parley did not break where the pretty plan said");
                    greedy(&mut layout, &text, max_width, rtl);
                    Wrap::Greedy
                }
            }
        };
        layout.align(Alignment::Start, AlignmentOptions::default());
        read_layout(&layout, &text, fonts, wrap, fallback, rtl)
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

/// Greedy breaking at `max_width`, each line measured without the quotes it hangs.
///
/// A line's hang depends on where it starts, which depends on the lines above, so each
/// pass breaks with the hangs the previous pass found. Line `k` breaks right once line
/// `k - 1` has, so the hangs settle within one pass per line; with no quote opening a
/// line, the first pass is plain greedy and the last.
fn greedy(layout: &mut Layout<Ink>, text: &str, max_width: f32, rtl: bool) {
    if !text.contains(is_hanging_quote) {
        layout.break_all_lines(Some(max_width));
        return;
    }
    let mut hangs: Vec<f32> = Vec::new();
    // Every line but an empty last one holds a byte, so this bound is never reached; it
    // keeps a change in parley from becoming a hang in the render path.
    for _ in 0..text.len() + 2 {
        break_with(layout, max_width, &hangs);
        let found: Vec<f32> = layout.lines().map(|line| hang_at(layout, text, line.text_range(), rtl)).collect();
        let settled = found.iter().enumerate().all(|(k, &hang)| hang == hangs.get(k).copied().unwrap_or(0.0));
        if settled {
            return;
        }
        hangs = found;
    }
}

/// Narrowest width at which greedy breaking keeps the line count it has at `max_width`.
fn balance(layout: &mut Layout<Ink>, text: &str, max_width: f32, rtl: bool) {
    greedy(layout, text, max_width, rtl);
    let lines = layout.len();
    if lines < 2 {
        return;
    }
    let (mut lo, mut hi) = (0.0_f32, max_width);
    for _ in 0..BALANCE_STEPS {
        let mid = 0.5 * (lo + hi);
        greedy(layout, text, mid, rtl);
        if layout.len() <= lines {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    greedy(layout, text, hi, rtl);
}

/// An unbreakable stretch of text between two break opportunities.
struct Segment {
    /// Advance including trailing whitespace (what it adds mid-line).
    full: f32,
    /// Advance without trailing whitespace (what it adds at the end of a line).
    bare: f32,
    /// What a line starting with this segment hangs (see [`hang_at`]).
    hang: f32,
    text: Range<usize>,
}

/// Minimum-raggedness breaking with the last line held to `min_words` segments, each
/// line measured without the quotes it hangs. Returns false when parley's breaker did
/// not reproduce the plan.
fn pretty(layout: &mut Layout<Ink>, text: &str, max_width: f32, min_words: usize, rtl: bool) -> bool {
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
                hang: hang_at(layout, text, line.text_range(), rtl),
                text: line.text_range(),
            }
        })
        .collect();
    let n = segments.len();
    if n < 2 {
        greedy(layout, text, max_width, rtl);
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
            let room = max_width + segments[i].hang;
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
    let most = plan.iter().map(|line| segments[line.start].hang).fold(0.0, f32::max);
    let mut breaker = layout.break_lines();
    breaker.state_mut().set_layout_max_advance(max_width + most);
    for line in &plan {
        let width: f32 =
            segments[line.start..line.end - 1].iter().map(|s| s.full).sum::<f32>() + segments[line.end - 1].bare;
        let room = max_width + segments[line.start].hang;
        breaker.state_mut().set_line_max_advance((width + FIT_SLACK).min(room + FIT_SLACK));
        breaker.break_next();
    }
    breaker.break_remaining(max_width);
    let broke: Vec<Range<usize>> = layout.lines().map(|l| l.text_range()).collect();
    let planned: Vec<Range<usize>> =
        plan.iter().map(|l| segments[l.start].text.start..segments[l.end - 1].text.end).collect();
    broke == planned
}

fn read_layout(
    layout: &Layout<Ink>,
    text: &str,
    fonts: &BundleFonts,
    wrap: Wrap,
    fallback: Option<&'static str>,
    rtl: bool,
) -> Result<TextLayout, EngineError> {
    let mut lines = Vec::new();
    let mut runs = Vec::new();
    let mut synthesized = false;
    let mut top = 0.0_f32;
    for (index, line) in layout.lines().enumerate() {
        let m = line.metrics();
        let hang = hang_at(layout, text, line.text_range(), rtl);
        // A left-to-right line is set from its start edge, so its hung quotes move out
        // past it. A right-to-left line was broken in a box `hang` wider than the measure
        // and start-aligned to that box's right edge, which already put them outside.
        let shift = if rtl { 0.0 } else { hang };
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
                glyphs: glyph_run.positioned_glyphs().map(|g| Glyph { id: g.id, x: g.x - shift, y: g.y }).collect(),
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
            width: m.advance - m.trailing_whitespace - hang,
            hang,
            text: line.text_range(),
            cap_height,
            x_height,
        });
        top += m.line_height;
    }
    let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    Ok(TextLayout { lines, runs, width, height: layout.height(), wrap, fallback, rtl, synthesized })
}
