//! The spine projection (PLAN 1.22, SPEC §10): placed on the global timeline, each beat
//! drawn as a thumbnail and in each of the deck's other formats, and written as
//! `docs/schema/spine.schema.json` says.

use scaena_core::model::check::Checker;
use scaena_ops::export::{Request, export};
use scaena_paint::Raster;
use std::path::{Path, PathBuf};

const REVENUE: &str = "../../docs/examples/revenue.deck.json";

fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn spine(out: Option<PathBuf>, size: Option<&str>) -> Result<scaena_ops::export::Exported, scaena_ops::OpsError> {
    let req = Request { format: "spine".into(), out, size: size.map(Into::into), ..Request::default() };
    export(&scaena_ops::open(Path::new(REVENUE)).unwrap(), &req)
}

#[test]
fn each_beat_is_drawn_beside_the_spine_and_placed_on_the_timeline() {
    let dir = scratch("spine-revenue");
    let file = dir.join("spine.json");
    let exported = spine(Some(file.clone()), Some("640x360")).unwrap();
    assert_eq!(exported.size, Some([640, 360]));
    assert!(exported.spine.is_none(), "a written spine is in its file");
    // Three beats; the deck lists 16:9, its own shape, and 9:16, the one other format.
    assert_eq!(exported.files.as_ref().map(Vec::len), Some(6), "{:#?}", exported.files);
    let text = std::fs::read_to_string(&file).unwrap();
    let written: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(Checker::spine().check(&written), [], "the spine file is what its schema says");
    let doubled = &written["beats"]["doubled"];
    assert_eq!(doubled["state"], "mix", "a beat shows the last of its states");
    // `revenue` starts after `intro`'s 660 ms of motion and 4 s hold; `mix` holds 6 s.
    assert_eq!((doubled["start"].as_f64(), doubled["end"].as_f64()), (Some(4660.0), Some(18900.0)));
    // Its renders, by paths relative to the spine file.
    assert_eq!(doubled["thumbnail"], "renders/doubled.png");
    assert_eq!(doubled["formats"]["9:16"], "renders/doubled@9x16.png");
    let size = |path: &str| {
        let png = Raster::from_png(&std::fs::read(dir.join(path)).unwrap()).unwrap();
        [png.width, png.height]
    };
    assert_eq!(size("renders/doubled.png"), [640, 360]);
    assert_eq!(size("renders/doubled@9x16.png"), [1080, 1920], "a format's render is its canvas, laid out again");
    // Every state is on the timeline too.
    let mix = written["states"].as_array().unwrap().iter().find(|s| s["id"] == "mix").unwrap();
    assert_eq!(
        (mix["start"].as_f64(), mix["span"].as_f64(), mix["hold"].as_f64()),
        (Some(12100.0), Some(800.0), Some(6000.0))
    );
}

#[test]
fn a_thumbnail_is_480_pixels_wide_unless_asked_and_unwritten_spines_draw_nothing() {
    let dir = scratch("spine-default");
    let exported = spine(Some(dir.join("spine.json")), None).unwrap();
    assert_eq!(exported.size, Some([480, 270]));
    // Printed rather than written, the spine is placed on the timeline and draws nothing.
    let printed = spine(None, None).unwrap();
    let projection = printed.spine.unwrap();
    assert!(printed.files.is_none());
    assert_eq!(projection.beats["doubled"].start, Some(4660.0));
    assert!(projection.beats.values().all(|b| b.thumbnail.is_none() && b.formats.is_empty()));
    // `spine_read` is the spine as the deck holds it: neither timed nor drawn.
    let read = scaena_ops::read::spine(&scaena_ops::open(Path::new(REVENUE)).unwrap());
    assert!(read.states.iter().all(|s| s.start.is_none()) && read.beats["doubled"].start.is_none());
    assert_eq!(read.beats["doubled"].state.as_deref(), Some("mix"));
    let wrong = spine(Some(scratch("spine-square").join("spine.json")), Some("500x500")).unwrap_err();
    assert!(wrong.to_string().contains("aspect ratio"), "{wrong}");
}
