//! A PowerPoint file as grounding (Vzestup's `pptxGrounding`): per slide its title, its body,
//! each chart's cached figures, and its speaker notes, under the deck's title and its sections.
//! Only its words reach the model: no slide, node, or layout of it enters a deck (invariant 8).

use crate::SourceError;
use crate::ooxml::{Package, paragraphs};
use crate::xml::Element;

/// A chart's cached figures: what the slide plots, which a flat transcription never recovers.
#[derive(Debug, Clone, PartialEq)]
pub struct Chart {
    pub title: Option<String>,
    pub kind: String,
    pub categories: Vec<String>,
    pub series: Vec<(Option<String>, Vec<Option<f64>>)>,
    pub format: Option<String>,
}

/// `bytes`, a `.pptx`, as grounding text.
pub fn read(bytes: &[u8]) -> Result<String, SourceError> {
    let mut pkg = Package::open(bytes)?;
    let presentation = pkg.part("ppt/presentation.xml", "it has no ppt/presentation.xml: not a presentation")?;
    let rels = pkg.rels("ppt/presentation.xml");
    // The slides in order, each with the id its sections name it by.
    let mut slides: Vec<(Option<String>, String)> = Vec::new();
    for sld in presentation.child("sldIdLst").into_iter().flat_map(|l| l.all("sldId")) {
        let Some(rel) = sld.rel_id() else { continue };
        let Some(target) = rels.iter().find(|r| r.id == rel).map(|r| r.target.clone()) else { continue };
        let id = sld.attrs.iter().find(|(k, _)| k == "id").map(|(_, v)| v.clone());
        slides.push((id, target));
    }
    if slides.is_empty() {
        return Err(SourceError::Unreadable("it has no slides".into()));
    }
    let sections = sections(&presentation, &slides);
    let mut out = Vec::new();
    if let Some(title) = pkg.title() {
        out.push(format!("# {title}"));
    }
    let mut last_section: Option<&str> = None;
    for (index, (_, path)) in slides.iter().enumerate() {
        if let Some(section) = sections.get(index).and_then(Option::as_deref)
            && last_section != Some(section)
        {
            out.push(format!("# Section: {section}"));
            last_section = Some(section);
        }
        out.push(slide(&mut pkg, index, path));
    }
    Ok(out.join("\n\n").trim().to_string())
}

/// The section each slide is in, by its order, from the presentation's section list.
fn sections(presentation: &Element, slides: &[(Option<String>, String)]) -> Vec<Option<String>> {
    let mut each = vec![None; slides.len()];
    let mut lists = Vec::new();
    presentation.descendants("sectionLst", &mut lists);
    for section in lists.first().into_iter().flat_map(|l| l.all("section")) {
        let name = section.attr("name").unwrap_or_default().to_string();
        for sld in section.child("sldIdLst").into_iter().flat_map(|l| l.all("sldId")) {
            let id = sld.attr("id");
            if let Some(index) = slides.iter().position(|(sid, _)| sid.as_deref() == id) {
                each[index] = Some(name.clone());
            }
        }
    }
    each
}

/// Each text shape (`sp`) and graphic frame of a shape tree, groups opened, in document order.
fn shapes<'a>(tree: &'a Element, sp: &mut Vec<&'a Element>, frames: &mut Vec<&'a Element>) {
    for e in tree.elements() {
        match e.local() {
            "sp" => sp.push(e),
            "graphicFrame" => frames.push(e),
            "grpSp" => shapes(e, sp, frames),
            _ => {}
        }
    }
}

/// A shape's placeholder type, if it is a placeholder.
fn placeholder(sp: &Element) -> Option<&str> {
    sp.at(&["nvSpPr", "nvPr", "ph"]).map(|ph| ph.attr("type").unwrap_or("body"))
}

/// A shape's text: its paragraphs, trimmed, empties dropped.
fn text(sp: &Element) -> String {
    let body = sp.child("txBody");
    let lines = body.map(paragraphs).unwrap_or_default();
    lines.iter().map(|l| l.trim()).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("\n")
}

/// One slide's grounding block (Vzestup's `slideToText`).
fn slide(pkg: &mut Package, index: usize, path: &str) -> String {
    let mut parts = Vec::new();
    let Some(doc) = pkg.xml(path) else { return format!("## Slide {}", index + 1) };
    let (mut sp, mut frames) = (Vec::new(), Vec::new());
    if let Some(tree) = doc.at(&["cSld", "spTree"]) {
        shapes(tree, &mut sp, &mut frames);
    }
    let is_title = |s: &Element| matches!(placeholder(s), Some("title" | "ctrTitle"));
    let title = sp.iter().find(|s| is_title(s)).map(|s| text(s)).unwrap_or_default();
    parts.push(if title.is_empty() {
        format!("## Slide {}", index + 1)
    } else {
        format!("## Slide {}: {title}", index + 1)
    });
    let body: Vec<String> = sp.iter().filter(|s| !is_title(s)).map(|s| text(s)).filter(|t| !t.is_empty()).collect();
    if !body.is_empty() {
        parts.push(body.join("\n"));
    }
    let rels = pkg.rels(path);
    for frame in frames {
        let Some(chart) = frame.at(&["graphic", "graphicData", "chart"]) else { continue };
        let Some(rel) = chart.rel_id() else { continue };
        let Some(target) = rels.iter().find(|r| r.id == rel).map(|r| r.target.clone()) else { continue };
        if let Some(chart) = pkg.xml(&target).and_then(|x| self::chart(&x)) {
            parts.push(chart_text(&chart));
        }
    }
    if let Some(notes) = rels.iter().find(|r| r.kind == "notesSlide").map(|r| r.target.clone())
        && let Some(notes) = pkg.xml(&notes).map(|n| self::notes(&n))
        && !notes.is_empty()
    {
        parts.push(format!("Speaker notes: {notes}"));
    }
    parts.join("\n")
}

/// The speaker notes of a notes slide: its body's words, the slide's picture, number, date,
/// header, and footer left out.
fn notes(doc: &Element) -> String {
    let (mut sp, mut frames) = (Vec::new(), Vec::new());
    if let Some(tree) = doc.at(&["cSld", "spTree"]) {
        shapes(tree, &mut sp, &mut frames);
    }
    let lines: Vec<String> = sp
        .iter()
        .filter(|s| !matches!(placeholder(s), Some("sldImg" | "sldNum" | "dt" | "ftr" | "hdr")))
        .flat_map(|s| s.child("txBody").map(paragraphs).unwrap_or_default())
        .map(|l| l.trim_end().to_string())
        .collect();
    let mut joined = lines.join("\n");
    while joined.contains("\n\n\n") {
        joined = joined.replace("\n\n\n", "\n\n");
    }
    joined.trim().to_string()
}

/// A plot element's kind, coarsely: `barChart` and `bar3DChart` are `bar`.
fn kind(local: &str) -> String {
    let n = local.trim_end_matches("Chart").trim_end_matches("3D").to_lowercase();
    let k = match n.as_str() {
        n if n.starts_with("bar") => "bar",
        n if n.starts_with("line") => "line",
        n if n == "ofpie" || n.starts_with("pie") => "pie",
        n if n.starts_with("doughnut") => "doughnut",
        n if n.starts_with("area") => "area",
        n if n.starts_with("scatter") || n.starts_with("bubble") => "scatter",
        n if n.starts_with("radar") => "radar",
        "" => "other",
        n => return n.to_string(),
    };
    k.to_string()
}

/// The most points of a series read: a point indexed past it is left out.
const MOST_POINTS: usize = 4_096;

/// The cached points of a `cat`, `val`, or `tx`, in order by their index, gaps empty.
fn points(container: Option<&Element>) -> Vec<String> {
    let Some(c) = container else { return Vec::new() };
    let scope = c.child("numRef").or_else(|| c.child("strRef")).or_else(|| c.child("multiLvlStrRef")).unwrap_or(c);
    let cache = scope
        .child("numCache")
        .or_else(|| scope.child("strCache"))
        .or_else(|| scope.child("multiLvlStrCache").and_then(|m| m.child("lvl")))
        .unwrap_or(scope);
    let mut by_index: Vec<(usize, String)> = cache
        .all("pt")
        .filter_map(|pt| {
            let idx = pt.attr("idx").map_or(Some(0), |i| i.parse().ok()).filter(|&i: &usize| i < MOST_POINTS)?;
            Some((idx, pt.child("v").map(|v| v.text().trim().to_string()).unwrap_or_default()))
        })
        .collect();
    let Some(most) = by_index.iter().map(|(i, _)| *i).max() else { return Vec::new() };
    let mut out = vec![String::new(); most + 1];
    for (i, v) in by_index.drain(..) {
        out[i] = v;
    }
    out
}

/// A chart part's figures; none where it plots no series.
pub fn chart(doc: &Element) -> Option<Chart> {
    let chart = doc.child("chart")?;
    let plot = chart.child("plotArea")?;
    let mut kind_of = None;
    let mut sers = Vec::new();
    for e in plot.elements().filter(|e| e.local().ends_with("Chart")) {
        kind_of.get_or_insert_with(|| kind(e.local()));
        sers.extend(e.all("ser"));
    }
    if sers.is_empty() {
        return None;
    }
    let mut categories = Vec::new();
    let mut format = None;
    let mut series = Vec::new();
    for ser in sers {
        if categories.is_empty() {
            categories = points(ser.child("cat"));
        }
        let val = ser.child("val");
        if format.is_none() {
            let scope = val.and_then(|v| v.child("numRef")).or(val);
            format = scope
                .and_then(|s| s.child("numCache"))
                .and_then(|c| c.child("formatCode"))
                .map(|f| f.text().trim().to_string())
                .filter(|f| !f.is_empty());
        }
        let values = points(val).iter().map(|v| v.parse::<f64>().ok().filter(|n| n.is_finite())).collect();
        let tx = ser.child("tx");
        let name = tx
            .and_then(|t| t.child("v"))
            .map(|v| v.text().trim().to_string())
            .filter(|n| !n.is_empty())
            .or_else(|| Some(points(tx).into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" ")))
            .filter(|n| !n.is_empty());
        series.push((name, values));
    }
    let title = chart.at(&["title", "tx", "rich"]).map(|rich| {
        let mut ts = Vec::new();
        rich.descendants("t", &mut ts);
        ts.iter().map(|t| t.text()).collect::<String>().trim().to_string()
    });
    Some(Chart {
        title: title.filter(|t| !t.is_empty()),
        kind: kind_of.unwrap_or_else(|| "other".into()),
        categories,
        series,
        format,
    })
}

/// A chart's figures as grounding (Vzestup's `chartToText`):
///
/// ```text
/// Chart "Revenue ($M)" (bar):
///   Revenue [$#,##0]: Q1=12, Q2=18, Q3=24, Q4=31
/// ```
pub fn chart_text(c: &Chart) -> String {
    let head = match &c.title {
        Some(t) => format!("Chart \"{t}\" ({}):", c.kind),
        None => format!("Chart ({}):", c.kind),
    };
    let unit = c.format.as_ref().map(|f| format!(" [{f}]")).unwrap_or_default();
    let mut lines = vec![head];
    for (i, (name, values)) in c.series.iter().enumerate() {
        let label = name.clone().unwrap_or_else(|| format!("Series {}", i + 1));
        let pairs: Vec<String> = values
            .iter()
            .enumerate()
            .map(|(j, v)| {
                let cat =
                    c.categories.get(j).filter(|c| !c.is_empty()).cloned().unwrap_or_else(|| format!("#{}", j + 1));
                let v = v.map_or_else(|| "\u{2014}".to_string(), number);
                format!("{cat}={v}")
            })
            .collect();
        lines.push(format!("  {label}{unit}: {}", pairs.join(", ")));
    }
    lines.join("\n")
}

/// A number as JavaScript writes it: `12`, not `12.0`.
fn number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 { format!("{}", v as i64) } else { format!("{v}") }
}
