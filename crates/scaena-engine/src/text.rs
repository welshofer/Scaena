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
//! lines at the wrong edge (PLAN 1.8 lifts this). Role `measure`, hyphenation,
//! hanging punctuation, and optical margins are PLAN 1.8 as well.

use crate::EngineError;
use crate::fonts::BundleFonts;
use crate::theme::{Numeric, TextRole, Theme, Wrap};
use parley::setting::Tag;
use parley::{
    Alignment, AlignmentOptions, FontFamily, FontFamilyName, FontFeature, FontFeatures, FontVariation, FontVariations,
    FontWeight, Language, Layout, LayoutContext, LineHeight, OverflowWrap, PositionedLayoutItem, StyleProperty,
    WordBreak,
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

/// One span of a text node and the role that sets it.
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub text: String,
    pub role: String,
}

/// A text node after the cascade, ready to lay out.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct TextSpec {
    pub spans: Vec<Span>,
    /// The node's role. Paragraph settings (wrap, `minLastLineWords`) come from it;
    /// each span's own role sets its glyphs.
    pub role: String,
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
    /// Widest line, trailing whitespace excluded.
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
    pub top: f32,
    pub baseline: f32,
    pub height: f32,
    pub ascent: f32,
    pub descent: f32,
    /// Advance without trailing whitespace.
    pub width: f32,
    /// Byte range in the node's text (spans concatenated).
    pub text: Range<usize>,
    /// From the line's first run (OS/2), when the font provides it.
    pub cap_height: Option<f32>,
    pub x_height: Option<f32>,
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
        let base = theme.text_role(&spec.role)?;
        let text: String = spec.spans.iter().map(|s| s.text.as_str()).collect();
        let mut layout = {
            let mut builder = self.lcx.ranged_builder(&mut fonts.cx, &text, 1.0, false);
            for prop in style_props(theme, &base, spec)? {
                builder.push_default(prop);
            }
            let mut at = 0;
            for span in &spec.spans {
                let range = at..at + span.text.len();
                at = range.end;
                if span.role != spec.role {
                    for prop in style_props(theme, &theme.text_role(&span.role)?, spec)? {
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
                layout.break_all_lines(Some(max_width));
                Wrap::Greedy
            }
            (Wrap::Balance, None) => {
                balance(&mut layout, max_width);
                Wrap::Balance
            }
            (Wrap::Pretty, None) => {
                if pretty(&mut layout, max_width, min_words) {
                    Wrap::Pretty
                } else {
                    fallback = Some("parley did not break where the pretty plan said");
                    layout.break_all_lines(Some(max_width));
                    Wrap::Greedy
                }
            }
        };
        layout.align(Alignment::Start, AlignmentOptions::default());
        read_layout(&layout, fonts, wrap, fallback, rtl)
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
    let families = theme.families()?;
    let mut features: BTreeMap<String, u16> = families
        .get(&role.family)
        .map(|f| f.features.iter().map(|(k, v)| (k.clone(), v.value())).collect())
        .unwrap_or_default();
    features.extend(role.features.iter().map(|(k, v)| (k.clone(), v.value())));
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
    let hex = theme.color(color_name).ok_or_else(|| EngineError::Theme(format!("unknown color `{color_name}`")))?;
    let ink = Color::from_hex(hex).map_err(|e| EngineError::Theme(e.to_string()))?.0;
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

/// Narrowest width at which greedy breaking keeps the line count it has at `max_width`.
fn balance(layout: &mut Layout<Ink>, max_width: f32) {
    layout.break_all_lines(Some(max_width));
    let lines = layout.len();
    if lines < 2 {
        return;
    }
    let (mut lo, mut hi) = (0.0_f32, max_width);
    for _ in 0..BALANCE_STEPS {
        let mid = 0.5 * (lo + hi);
        layout.break_all_lines(Some(mid));
        if layout.len() <= lines {
            hi = mid;
        } else {
            lo = mid;
        }
    }
    layout.break_all_lines(Some(hi));
}

/// An unbreakable stretch of text between two break opportunities.
struct Segment {
    /// Advance including trailing whitespace (what it adds mid-line).
    full: f32,
    /// Advance without trailing whitespace (what it adds at the end of a line).
    bare: f32,
    text: Range<usize>,
}

/// Minimum-raggedness breaking with the last line held to `min_words` segments.
/// Returns false when parley's breaker did not reproduce the plan.
fn pretty(layout: &mut Layout<Ink>, max_width: f32, min_words: usize) -> bool {
    // Probe: at (almost) zero width parley breaks at every UAX #14 opportunity, so each
    // probe line is exactly one segment. Shaping happens before breaking, so segment
    // widths add up to line widths exactly.
    layout.break_all_lines(Some(PROBE_WIDTH));
    let segments: Vec<Segment> = layout
        .lines()
        .map(|line| {
            let m = line.metrics();
            Segment { full: m.advance, bare: m.advance - m.trailing_whitespace, text: line.text_range() }
        })
        .collect();
    let n = segments.len();
    if n < 2 {
        layout.break_all_lines(Some(max_width));
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
            if width > max_width && i < j - 1 {
                break; // more segments only widen the line
            }
            let cost = if j == n {
                if j - i < min_words && n >= min_words {
                    continue; // the widow `pretty` exists to prevent
                }
                LINE_PENALTY
            } else {
                let slack = ((max_width - width) / max_width).max(0.0);
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
    let mut breaker = layout.break_lines();
    breaker.state_mut().set_layout_max_advance(max_width);
    for line in &plan {
        let width: f32 =
            segments[line.start..line.end - 1].iter().map(|s| s.full).sum::<f32>() + segments[line.end - 1].bare;
        breaker.state_mut().set_line_max_advance((width + FIT_SLACK).min(max_width + FIT_SLACK));
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
    fonts: &BundleFonts,
    wrap: Wrap,
    fallback: Option<&'static str>,
    rtl: bool,
) -> Result<TextLayout, EngineError> {
    let mut lines = Vec::new();
    let mut runs = Vec::new();
    let mut synthesized = false;
    for (index, line) in layout.lines().enumerate() {
        let m = line.metrics();
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
                glyphs: glyph_run.positioned_glyphs().map(|g| Glyph { id: g.id, x: g.x, y: g.y }).collect(),
                line: index,
            });
        }
        lines.push(LineBox {
            top: m.block_min_coord,
            baseline: m.baseline,
            height: m.line_height,
            ascent: m.ascent,
            descent: m.descent,
            width: m.advance - m.trailing_whitespace,
            text: line.text_range(),
            cap_height,
            x_height,
        });
    }
    let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
    Ok(TextLayout { lines, runs, width, height: layout.height(), wrap, fallback, rtl, synthesized })
}
