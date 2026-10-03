//! PNG and SVG export (PLAN 1.21): an image per state. Every torture state's SVG, drawn by
//! an SVG rasterizer (`resvg`), against the CPU painter's frame of the state, by SPEC
//! §13.5's metric; and the PNGs are the CPU painter's frames.

use scaena_ops::export::{Exported, Request, export};
use scaena_ops::render::{Request as Render, render};
use scaena_paint::Raster;
use std::path::{Path, PathBuf};

const TORTURE: &str = "../../tests/fixtures/torture.scaena";

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn exported(bundle: &str, format: &str, states: Option<&[&str]>, size: Option<&str>, out: &Path) -> Exported {
    let req = Request {
        format: format.into(),
        states: states.map(|s| s.iter().map(|s| s.to_string()).collect()),
        out: Some(out.to_path_buf()),
        size: size.map(Into::into),
        ..Request::default()
    };
    export(&scaena_ops::open(Path::new(bundle)).unwrap(), &req).unwrap()
}

/// An SVG drawn at its own size.
fn rasterize(svg: &[u8]) -> Raster {
    let tree = resvg::usvg::Tree::from_data(svg, &resvg::usvg::Options::default()).expect("the SVG parses");
    let size = tree.size().to_int_size();
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size.width(), size.height()).expect("a drawing");
    resvg::render(&tree, resvg::tiny_skia::Transform::default(), &mut pixmap.as_mut());
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Raster { width: pixmap.width(), height: pixmap.height(), rgba }
}

fn cpu(bundle: &str, state: &str, size: Option<&str>) -> Raster {
    let req = Render { state: state.into(), size: size.map(Into::into), ..Render::default() };
    Raster::from_png(&render(Path::new(bundle), &req).unwrap().png).unwrap()
}

/// Each state's SVG against the CPU painter's frame; what fails is kept beside the SVGs.
fn svgs_draw_as_the_cpu_painter_does(bundle: &str, name: &str, size: Option<&str>) {
    let dir = scratch(name);
    let svgs = exported(bundle, "svg", None, size, &dir);
    let mut failed = Vec::new();
    for (state, file) in svgs.pages.unwrap().iter().zip(svgs.files.unwrap()) {
        let drawn = rasterize(&std::fs::read(&file).unwrap());
        let painted = cpu(bundle, state, size);
        let d = scaena_paint::diff::compare(&painted, &drawn).unwrap();
        if !d.passes() {
            std::fs::write(dir.join(format!("{state}.resvg.png")), drawn.to_png().unwrap()).unwrap();
            std::fs::write(dir.join(format!("{state}.cpu.png")), painted.to_png().unwrap()).unwrap();
            let diff = scaena_paint::diff::image(&painted, &drawn).unwrap();
            std::fs::write(dir.join(format!("{state}.diff.png")), diff.to_png().unwrap()).unwrap();
            failed.push(format!("{state}: {d:?}"));
        }
    }
    assert!(failed.is_empty(), "SVGs that draw otherwise than the CPU painter (in {}): {failed:#?}", dir.display());
}

#[test]
fn every_torture_state_draws_in_svg_as_the_cpu_painter_draws_it() {
    svgs_draw_as_the_cpu_painter_does(TORTURE, "svg-torture", None);
}

#[test]
fn the_trails_example_draws_in_svg_at_half_size_as_the_cpu_painter_does() {
    svgs_draw_as_the_cpu_painter_does("../../docs/examples/trails.deck.json", "svg-trails", Some("960x540"));
}

#[test]
fn an_svg_outlines_its_glyphs_and_keeps_their_text_for_a_reader() {
    let dir = scratch("svg-text");
    let svgs = exported(TORTURE, "svg", Some(&["liga", "emoji", "mesh"]), None, &dir);
    assert_eq!(svgs.pages.as_deref(), Some(&["liga".to_string(), "emoji".to_string(), "mesh".to_string()][..]));
    assert_eq!(svgs.size, Some([1920, 1080]));
    let read = |state: &str| std::fs::read_to_string(dir.join(format!("{state}.svg"))).unwrap();
    let liga = read("liga");
    // Glyphs are paths: no font is named, so nothing lays the text out again.
    assert!(!liga.contains("font-family"), "an SVG names no font");
    // Each run's text lies over it, transparent, as wide as the run.
    let texts: Vec<&str> =
        liga.match_indices("<text ").map(|(i, _)| &liga[i..i + liga[i..].find("</text>").unwrap()]).collect();
    assert!(!texts.is_empty());
    assert!(texts.iter().all(|t| t.contains("fill-opacity=\"0\"") && t.contains("textLength=")), "{texts:#?}");
    // Emoji are color glyphs, drawn as images, and still say what they are.
    let emoji = read("emoji");
    assert!(emoji.contains("<image ") && emoji.contains("<text "));
    // A shader is its CPU reference's pixels, clipped to its rect.
    let mesh = read("mesh");
    assert!(mesh.contains("image-rendering=\"optimizeSpeed\" xlink:href=\"data:image/png;base64,"));
    // The same frame writes the same bytes.
    let again = scratch("svg-text-again");
    exported(TORTURE, "svg", Some(&["liga"]), None, &again);
    assert_eq!(std::fs::read_to_string(again.join("liga.svg")).unwrap(), liga);
}

#[test]
fn pngs_are_the_cpu_painters_frames_at_any_size() {
    let dir = scratch("png-sizes");
    let pngs = exported(TORTURE, "png", Some(&["shapes", "chart"]), Some("960x540"), &dir);
    assert_eq!(pngs.size, Some([960, 540]));
    let files = pngs.files.unwrap();
    assert_eq!(files.len(), 2);
    assert!(files[0].ends_with("shapes.png") && files[1].ends_with("chart.png"), "{files:?}");
    for (state, file) in [("shapes", &files[0]), ("chart", &files[1])] {
        let png = Raster::from_png(&std::fs::read(file).unwrap()).unwrap();
        assert_eq!(png, cpu(TORTURE, state, Some("960x540")), "{state}");
    }
    assert_eq!(pngs.bytes, Some(files.iter().map(|f| std::fs::metadata(f).unwrap().len()).sum()));
}

#[test]
fn an_image_export_says_what_it_needs() {
    let bundle = scaena_ops::open(Path::new(TORTURE)).unwrap();
    let ask = |req: Request| export(&bundle, &req).unwrap_err().to_string();
    let png = |out: Option<PathBuf>| Request { format: "png".into(), out, ..Request::default() };
    assert!(ask(png(None)).contains("--out DIR"));
    let file = scratch("png-onto-a-file");
    std::fs::create_dir_all(&file).unwrap();
    let file = file.join("taken.txt");
    std::fs::write(&file, "mine").unwrap();
    assert!(ask(png(Some(file.clone()))).contains("is a file"));
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "mine");
    let wrong = Request { size: Some("1000x1000".into()), ..png(Some(scratch("png-square"))) };
    assert!(ask(wrong).contains("aspect ratio"));
    let fps = Request { fps: Some(30), ..png(Some(scratch("png-fps"))) };
    assert!(ask(fps).contains("--fps"));
    let pdf = Request { format: "pdf".into(), size: Some("960x540".into()), ..png(Some(scratch("pdf-size"))) };
    assert!(ask(pdf).contains("--size"));
}
