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
    // Each file is the page at its place: an export names it by its slide (`Slide 03.svg`).
    let (pages, files) = (svgs.pages.clone().unwrap(), svgs.files.clone().unwrap());
    let read = |state: &str| std::fs::read_to_string(&files[pages.iter().position(|p| p == state).unwrap()]).unwrap();
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
    let files = exported(TORTURE, "svg", Some(&["liga"]), None, &again).files.unwrap();
    assert_eq!(std::fs::read_to_string(&files[0]).unwrap(), liga);
}

#[test]
fn pngs_are_the_cpu_painters_frames_at_any_size() {
    let dir = scratch("png-sizes");
    let pngs = exported(TORTURE, "png", Some(&["shapes", "chart"]), Some("960x540"), &dir);
    assert_eq!(pngs.size, Some([960, 540]));
    let files = pngs.files.unwrap();
    assert_eq!(files.len(), 2);
    // Each named by where it stands among the deck's slides: `chart` is the second step of its slide.
    let deck = scaena_ops::open(Path::new(TORTURE)).unwrap().deck;
    let said = scaena_core::words::slides(&deck);
    let digits = scaena_core::words::last_steps(&deck).len().to_string().len();
    for (state, file) in [("shapes", &files[0]), ("chart", &files[1])] {
        let at = deck.states.iter().position(|s| s.id == state).unwrap();
        let (number, step) =
            said[at].trim_start_matches("Slide ").split_once(", step ").unwrap_or((&said[at][6..], ""));
        let number: usize = number.parse().unwrap();
        let named = Path::new(file).file_name().unwrap().to_string_lossy().into_owned();
        assert!(named.starts_with(&format!("Slide {number:0digits$}")) && named.ends_with(".png"), "{state}: {named}");
        if !step.is_empty() {
            assert!(named.contains(&format!(", step {step}")), "{state}: {named}");
        }
    }
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
    // Each names its field, as an MCP client writes it, and its flag (lift-up 10-25b USE-5).
    assert!(ask(fps).contains("`fps` (--fps)"));
    let pdf = Request { format: "pdf".into(), size: Some("960x540".into()), ..png(Some(scratch("pdf-size"))) };
    assert!(ask(pdf).contains("`size` (--size)"));
    let nowhere = Request { format: "pdf".into(), out: None, ..png(None) };
    assert!(ask(nowhere).contains("give it `out`, a file's path (--out FILE)"));
    let guessed = Request { states: Some(vec!["revenu".into()]), ..png(Some(scratch("png-guessed"))) };
    let said = ask(guessed);
    assert!(said.contains("`states` (--states) names `revenu`, which is not a state of the deck; "), "{said}");
}

/// An image of each state is named by where it stands among the slides, as the editor's zip of
/// slides names them (PLAN 2.122), its number given as many digits as the last slide's: the files
/// sort in the deck's order, a step after its slide, whatever its ids say; and each SVG is titled
/// by its slide's headline, not the deck's title. The higher-ed deck runs past nine slides, steps among them.
#[test]
fn images_are_named_by_their_slide_and_step_and_titled_by_its_headline() {
    const DECK: &str = "../../docs/examples/higher-ed.deck.json";
    let deck = scaena_ops::open(Path::new(DECK)).unwrap().deck;
    let dir = scratch("svg-slides");
    let svgs = exported(DECK, "svg", None, Some("480x270"), &dir);
    let (pages, files) = (svgs.pages.unwrap(), svgs.files.unwrap());
    let names: Vec<String> =
        files.iter().map(|f| Path::new(f).file_name().unwrap().to_string_lossy().into_owned()).collect();
    assert_eq!(names[0], "Slide 01.svg");
    assert_eq!(names[2], "Slide 03, step 1.svg", "`era`, the first of slide 3's four states: {names:?}");
    assert_eq!(names[3], "Slide 03, step 2.svg", "the second, a step of slide 3: {names:?}");
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(sorted, names, "the files sort in the deck's order");
    assert_eq!(pages.len(), deck.states.len());
    // Each is titled by its slide's headline as it shows there.
    let snaps = scaena_core::tracking::resolve_states(&deck).unwrap();
    let svg = std::fs::read_to_string(&files[1]).unwrap();
    let headline = (snaps[1].nodes.iter())
        .find(|(_, p)| matches!(p.get("role").and_then(|r| r.as_str()), Some("display" | "headline" | "title")))
        .map(|(_, p)| scaena_core::reading::words(p))
        .expect("the second slide has a headline");
    let title = svg.split("<title>").nth(1).and_then(|t| t.split("</title>").next()).unwrap();
    assert_eq!(title, headline.split_whitespace().collect::<Vec<_>>().join(" ").replace('&', "&amp;"));
    let deck_title = deck.meta.as_ref().and_then(|m| m.title.clone()).unwrap_or_default();
    assert_ne!(title, deck_title, "not the deck's title");
}
