//! PDF export (PLAN 1.20): every page drawn by a PDF rasterizer (`hayro`) against the CPU
//! painter's frame of the same state, by SPEC §13.5's metric.

use scaena_ops::export::{Export, export, export_pdf};
use scaena_ops::render::{Request, render};
use scaena_paint::Raster;
use std::path::Path;
use std::sync::Arc;

const TORTURE: &str = "../../tests/fixtures/torture.scaena";

/// Each page at 2 pixels to the point: the canvas at one pixel to the unit.
fn rasterize(pdf: Vec<u8>) -> Vec<Raster> {
    let pdf = hayro::hayro_syntax::Pdf::new(Arc::new(pdf)).expect("the PDF parses");
    let settings = hayro::hayro_interpret::InterpreterSettings::default();
    hayro::render_pdf(&pdf, 2.0, settings, None)
        .expect("the pages render")
        .into_iter()
        .map(|pixmap| {
            let (width, height) = (u32::from(pixmap.width()), u32::from(pixmap.height()));
            let rgba = pixmap.take_unpremultiplied().into_iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect();
            Raster { width, height, rgba }
        })
        .collect()
}

#[test]
fn every_page_draws_its_slide_as_the_cpu_painter_does() {
    // Shaders at one pixel to the unit, so a page has the same pixels as the CPU
    // painter's frame: grain is per device pixel (SPEC §3.8).
    let bundle = scaena_ops::open(Path::new(TORTURE)).unwrap();
    let Export::Pdf { bytes, pages: states } = export_pdf(&bundle, None, 1.0).unwrap() else { panic!("a pdf") };
    let pages = rasterize(bytes);
    assert_eq!(pages.len(), states.len());
    let mut failed = Vec::new();
    for (state, page) in states.iter().zip(&pages) {
        let cpu = render(Path::new(TORTURE), &Request { state: state.clone(), ..Request::default() }).unwrap();
        let cpu = Raster::from_png(&cpu.png).unwrap();
        let d = scaena_paint::diff::compare(&cpu, page).unwrap();
        if !d.passes() {
            failed.push(format!("{state}: {d:?}"));
        }
    }
    assert!(failed.is_empty(), "pages that draw otherwise than the CPU painter: {failed:#?}");
}

#[test]
fn a_pdf_draws_each_slide_at_its_last_state_and_shaders_at_twice_the_canvas() {
    let bundle = scaena_ops::open(Path::new(TORTURE)).unwrap();
    let Export::Pdf { pages, .. } = export(&bundle, "pdf", None).unwrap() else { panic!("a pdf") };
    let slides: Vec<&str> = pages.iter().map(String::as_str).collect();
    // The chart slide's three states make one page, its last.
    assert!(slides.contains(&"chart-next") && !slides.contains(&"chart-intro") && !slides.contains(&"chart"));
    let mesh = vec!["mesh".to_string()];
    let Export::Pdf { bytes, pages } = export(&bundle, "pdf", Some(&mesh)).unwrap() else { panic!("a pdf") };
    assert_eq!(pages, mesh);
    // The full-canvas mesh embeds as an image of 3840 × 2160 pixels.
    let text = String::from_utf8_lossy(&bytes);
    assert!(text.contains("/Width 3840") && text.contains("/Height 2160"));
    let err = export(&bundle, "pdf", Some(&["nope".to_string()])).unwrap_err();
    assert!(err.to_string().contains("nope"), "{err}");
}

/// The PDF's structure, an element a line, indented by depth: its type, then its alt
/// text in brackets and its language in braces, if it has them.
fn structure(pdf: &[u8]) -> Vec<String> {
    use hayro::hayro_syntax::object::{Dict, Name, Object};
    fn text(s: hayro::hayro_syntax::object::String) -> String {
        match s.as_bytes() {
            [0xFE, 0xFF, rest @ ..] => {
                let units: Vec<u16> = rest.as_chunks::<2>().0.iter().map(|&c| u16::from_be_bytes(c)).collect();
                String::from_utf16_lossy(&units)
            }
            bytes => bytes.iter().map(|&b| char::from(b)).collect(),
        }
    }
    fn walk(k: Object, depth: usize, lines: &mut Vec<String>) {
        match k {
            Object::Array(a) => a.iter::<Object>().for_each(|o| walk(o, depth, lines)),
            Object::Dict(d) => {
                // A marked-content reference has no type: it is content, not an element.
                let Some(kind) = d.get::<Name>(b"S") else { return };
                let mut line = format!("{}{}", "  ".repeat(depth), kind.as_str());
                if let Some(alt) = d.get(b"Alt") {
                    line += &format!(" [{}]", text(alt));
                }
                if let Some(lang) = d.get(b"Lang") {
                    line += &format!(" {{{}}}", text(lang));
                }
                lines.push(line);
                if let Some(k) = d.get::<Object>(b"K") {
                    walk(k, depth + 1, lines);
                }
            }
            _ => {}
        }
    }
    let pdf = hayro::hayro_syntax::Pdf::new(Arc::new(pdf.to_vec())).expect("the PDF parses");
    let xref = pdf.xref();
    let catalog: Dict = xref.get(xref.root_id()).expect("a catalog");
    let root: Dict = catalog.get(b"StructTreeRoot").expect("a structure tree");
    let mut lines = Vec::new();
    walk(root.get(b"K").expect("a root element"), 0, &mut lines);
    lines
}

/// The direct children of the element on line `at`, by type.
fn children(lines: &[String], at: usize) -> Vec<&str> {
    let depth = |l: &str| l.len() - l.trim_start().len();
    let own = depth(&lines[at]);
    let mut out = Vec::new();
    for line in &lines[at + 1..] {
        match depth(line) {
            d if d <= own => break,
            d if d == own + 2 => out.push(line.trim()),
            _ => {}
        }
    }
    out
}

/// The outline's entries, in order.
fn outline(pdf: &[u8]) -> Vec<String> {
    use hayro::hayro_syntax::object::Dict;
    let pdf = hayro::hayro_syntax::Pdf::new(Arc::new(pdf.to_vec())).expect("the PDF parses");
    let xref = pdf.xref();
    let catalog: Dict = xref.get(xref.root_id()).expect("a catalog");
    let mut entries = Vec::new();
    let mut item = catalog.get::<Dict>(b"Outlines").and_then(|o| o.get::<Dict>(b"First"));
    while let Some(entry) = item {
        let title: hayro::hayro_syntax::object::String = entry.get(b"Title").expect("an entry has a title");
        entries.push(String::from_utf8_lossy(title.as_bytes()).into_owned());
        item = entry.get(b"Next");
    }
    entries
}

#[test]
fn a_pdf_reads_by_the_spine() {
    let bundle = scaena_ops::open(Path::new("../../docs/examples/trails.deck.json")).unwrap();
    let Export::Pdf { bytes, .. } = export(&bundle, "pdf", None).unwrap() else { panic!("a pdf") };
    let lines = structure(&bytes);
    assert_eq!(lines[0], "Document {en-US}");
    // A section per spine section, holding its beats' pages.
    let sections: Vec<usize> = (0..lines.len()).filter(|&i| lines[i] == "  Sect").collect();
    let pages: Vec<usize> = sections.iter().map(|&i| children(&lines, i).len()).collect();
    assert_eq!(children(&lines, 0), ["Sect"; 6]);
    assert_eq!(pages, [2, 2, 6, 2, 2, 1]);
    // The cover reads its kicker, title, and subtitle; its shader is not read.
    assert_eq!(children(&lines, sections[0] + 1), ["P", "H1", "H2"]);
    // A chart is a figure with its alt text.
    let miles = lines.iter().position(|l| l.trim_start().starts_with("Figure [Miles of trail")).unwrap();
    assert_eq!(
        lines[miles].trim(),
        "Figure [Miles of trail rebuilt per month, May to October: 3.1, 8.4, 12.6, 13.9, 9.8, 4.2; the plan was 6.7 a month.]"
    );
    // The budget table, by rows: a header row, then six rows of four cells.
    let table = lines.iter().position(|l| l.trim() == "Table").unwrap();
    let rows: Vec<usize> = (table + 1..lines.len())
        .filter(|&i| lines[i].trim() == "TR")
        .take_while(|&i| lines[i].len() - lines[i].trim_start().len() == 8)
        .collect();
    assert_eq!(rows.len(), 7);
    assert_eq!(children(&lines, rows[0]), ["TH"; 4]);
    assert!(rows[1..].iter().all(|&r| children(&lines, r) == ["TD"; 4]));
    // The spine's sections are the outline.
    assert_eq!(
        outline(&bytes),
        ["Opening", "The season in numbers", "Where the work went", "What it cost", "How we work", "Next season"]
    );
}

#[test]
fn a_table_reads_a_cell_for_every_column_of_every_row() {
    use scaena_ops::create::{Attach, Create, create};
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("pdf-nulls");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // The south's units and the east's share were not counted.
    let csv = dir.join("sales.csv");
    std::fs::write(&csv, "region,units,share\nNorth,120,0.4\nSouth,,0.3\nEast,90,\n").unwrap();
    let deck = serde_json::json!({
        "scaena": scaena_core::FORMAT_VERSION, "canvas": { "width": 1920, "height": 1080 }, "meta": { "lang": "en-US" },
        "nodes": { "sales": { "type": "table", "data": "@sales", "alt": "Units and share by region.",
                              "columns": [{ "field": "region", "title": "Region" }, { "field": "units", "title": "Units" },
                                          { "field": "share", "title": "Share", "format": ".0%" }],
                              "at": { "in": "canvas" } } },
        "states": [{ "id": "a", "props": { "sales": {} } }]
    });
    let theme = Path::new("../../docs/examples/themes/dusk.theme.json").to_path_buf();
    let data = vec![Attach { id: "sales".into(), file: csv, schema: None, parse: None }];
    let made = create(&dir.join("deck"), &Create { theme, deck: Some(deck), data, ..Create::default() }).unwrap();
    assert!(made.created, "{made:#?}");
    let bundle = scaena_ops::open(&dir.join("deck")).unwrap();
    let Export::Pdf { bytes, .. } = export(&bundle, "pdf", None).unwrap() else { panic!("a pdf") };
    let lines = structure(&bytes);
    let table = lines.iter().position(|l| l.trim() == "Table").unwrap();
    let rows = children(&lines, table);
    assert_eq!(rows, ["TR"; 4]);
    let cells: Vec<Vec<&str>> =
        (table + 1..lines.len()).filter(|&i| lines[i].trim() == "TR").map(|i| children(&lines, i)).collect();
    assert_eq!(cells, [vec!["TH"; 3], vec!["TD"; 3], vec!["TD"; 3], vec!["TD"; 3]]);
}
