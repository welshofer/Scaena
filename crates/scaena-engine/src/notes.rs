//! A slide with its presenter's notes, as a page of a PDF (dogfood pass 2; Keynote's "Include
//! presenter notes", PowerPoint's Notes Pages): the slide drawn smaller at the page's head as its
//! own page draws it, and under it the notes a presenter reads for it (SPEC §2.6), set by the
//! engine in the theme's `body` role, black on white, as paper prints them. Notes that run past
//! the page go on to the next, which holds notes alone; a slide with none still has its page.
//!
//! The page is a display list like any frame's, so every painter and export draws it as it
//! draws a slide. Its notes are one layer named [`NOTES`], which a tagged PDF reads as a
//! paragraph after the slide's figure (SPEC §3.12).

use crate::text::TextLayout;
use scaena_core::displaylist::{Blend, Color, DisplayList, FillRule, Op, Paint, Path};

/// The layer that draws a page's notes: what a tagged PDF reads as their paragraph. No deck's
/// node takes it, an id beginning with `_` being none a deck may write.
pub const NOTES: &str = "_notes";

/// How tall a page is against its width: US Letter, upright.
pub const TALL: f32 = 11.0 / 8.5;

/// The page's margin against its width: 120 units on a 1920-wide canvas.
const MARGIN: f32 = 1.0 / 16.0;

/// The room between the slide and its notes, against the margin.
const GAP: f32 = 0.5;

/// The slide's keyline, canvas units, and its gray: what sets a white slide off the page.
const KEYLINE: f32 = 2.0;
const KEYLINE_COLOR: Color = Color([0xBB, 0xBB, 0xBB, 0xFF]);

/// The notes' ink: near black, as paper prints them, whatever the theme's text on its surface.
pub const INK: Color = Color([0x1A, 0x1A, 0x1A, 0xFF]);

/// A notes page's geometry for a slide `viewport` wide and high, canvas units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    /// The page, `[width, height]`.
    pub page: [f32; 2],
    pub margin: f32,
    /// How much smaller the slide is drawn.
    pub scale: f32,
    /// Where the notes start on the first page, and on each page after it.
    pub first: f32,
    pub after: f32,
    /// How wide the notes are set.
    pub width: f32,
}

impl Geometry {
    pub fn of([w, h]: [f32; 2]) -> Geometry {
        let margin = (w * MARGIN).round();
        let width = w - 2.0 * margin;
        let scale = width / w.max(1.0);
        Geometry {
            page: [w, (w * TALL).round()],
            margin,
            scale,
            first: margin + h * scale + GAP * margin,
            after: margin,
            width,
        }
    }

    /// The foot the notes stop above on every page.
    pub fn foot(&self) -> f32 {
        self.page[1] - self.margin
    }
}

/// The pages of `slide` with `notes`, laid out `geometry.width` wide: the first draws the slide
/// and the notes' first lines, each after it the lines that fit on it. `ops` makes a run of the
/// notes' lines into glyph ops on a page's display list.
pub fn pages(
    slide: &DisplayList,
    notes: Option<&TextLayout>,
    geometry: &Geometry,
    mut ops: impl FnMut(&mut DisplayList, &TextLayout) -> Vec<Op>,
) -> Vec<DisplayList> {
    let [w, h] = slide.viewport;
    let page = |fonts: Vec<_>| DisplayList { dl: slide.dl, viewport: geometry.page, fonts, ops: vec![white(geometry)] };
    let mut first = page(slide.fonts.clone());
    // The slide as its own page draws it, held to its box, with a keyline about it.
    let m = geometry.margin;
    let s = geometry.scale;
    first.ops.push(Op::Layer {
        node: None,
        cell: None,
        label: None,
        transform: [s, 0.0, 0.0, s, m, m],
        opacity: 1.0,
        blend: Blend::Normal,
        clip: Some(Path::rect([0.0, 0.0, w, h])),
        ops: slide.ops.clone(),
    });
    first.ops.push(Op::Stroke {
        path: Path::rect([m, m, w * s, h * s]),
        paint: Paint::Solid(KEYLINE_COLOR),
        width: KEYLINE,
        cap: scaena_core::displaylist::Cap::Butt,
        join: scaena_core::displaylist::Join::Miter,
        miter_limit: 4.0,
        dash: Vec::new(),
        dash_offset: 0.0,
    });
    let Some(notes) = notes.filter(|n| !n.runs.is_empty()) else { return vec![first] };
    // The lines each page holds: as many as stand above its foot, and at least one.
    let mut out = Vec::new();
    let mut list = first;
    let mut top = geometry.first;
    let mut line = 0;
    while line < notes.lines.len() {
        let from = notes.lines[line].top;
        let mut to = line + 1;
        while to < notes.lines.len() {
            let l = &notes.lines[to];
            if top + (l.top + l.height - from) > geometry.foot() {
                break;
            }
            to += 1;
        }
        let mut part = notes.clone();
        part.runs.retain(|r| (line..to).contains(&r.line));
        part.underlines.retain(|u| (line..to).contains(&u.line));
        // A run says the text to the next cluster's start: the page's text ends with its last
        // line, so its last run says none of the lines a page after it sets.
        let end = notes.lines[to - 1].text.end.min(part.text.len());
        if part.text.is_char_boundary(end) {
            part.text.truncate(end);
        }
        if !part.runs.is_empty() {
            let inner = ops(&mut list, &part);
            list.ops.push(Op::Layer {
                node: Some(NOTES.to_string()),
                cell: None,
                label: None,
                transform: [1.0, 0.0, 0.0, 1.0, m, top - from],
                opacity: 1.0,
                blend: Blend::Normal,
                clip: None,
                ops: inner,
            });
        }
        out.push(list);
        list = page(Vec::new());
        top = geometry.after;
        line = to;
    }
    out
}

/// The page's paper.
fn white(geometry: &Geometry) -> Op {
    let [w, h] = geometry.page;
    Op::Fill { path: Path::rect([0.0, 0.0, w, h]), rule: FillRule::NonZero, paint: Paint::Solid(Color([0xFF; 4])) }
}
