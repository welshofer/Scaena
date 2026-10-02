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
