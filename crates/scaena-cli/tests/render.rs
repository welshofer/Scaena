//! `scaena render` end to end (PLAN 0.6): the binary renders a torture state whose
//! display list, once quantized, is the engine's golden, and whose PNG is within
//! SPEC §13.5 tolerance of the golden raster. The CLI paints the unquantized list
//! (SPEC §13.4: only comparisons round), so the PNG is close to the golden, not
//! byte-identical.

use scaena_core::displaylist::{DisplayList, quantize};
use scaena_paint::{Raster, diff};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";
const GOLDEN: &str = "../../tests/golden/torture";

fn scaena(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_scaena")).args(args).output().unwrap()
}

/// A fresh scratch directory per test under cargo's integration-test temp dir.
fn scratch(test: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(format!("render-{test}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn code(out: &Output) -> i32 {
    out.status.code().unwrap()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn renders_a_state_to_a_png_and_a_display_list_matching_the_goldens() {
    let dir = scratch("goldens");
    let (png, dl) = (dir.join("anchors.png"), dir.join("anchors.dl.json"));
    let out = scaena(&[
        "render",
        BUNDLE,
        "--state",
        "anchors",
        "--out",
        png.to_str().unwrap(),
        "--display-list",
        dl.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));

    let mut list = DisplayList::from_json(&std::fs::read_to_string(&dl).unwrap()).unwrap();
    quantize(&mut list);
    let golden = std::fs::read_to_string(format!("{GOLDEN}/anchors.dl.json")).unwrap();
    assert_eq!(list.to_golden_json().unwrap(), golden);

    let raster = Raster::from_png(&std::fs::read(&png).unwrap()).unwrap();
    let golden = Raster::from_png(&std::fs::read(format!("{GOLDEN}/anchors.png")).unwrap()).unwrap();
    let d = diff::compare(&golden, &raster).unwrap();
    assert!(d.passes(), "{d}");
}

#[test]
fn size_scales_uniformly_and_json_reports_the_stages() {
    let dir = scratch("size");
    let png = dir.join("half.png");
    let out = scaena(&[
        "render",
        BUNDLE,
        "--state",
        "balance",
        "--size",
        "960x540",
        "--out",
        png.to_str().unwrap(),
        "--json",
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let raster = Raster::from_png(&std::fs::read(&png).unwrap()).unwrap();
    assert_eq!((raster.width, raster.height), (960, 540));

    let summary: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(summary["state"], "balance");
    assert_eq!(summary["t_ms"], serde_json::Value::Null, "omitted --t renders the state at rest");
    assert_eq!(summary["size"], serde_json::json!([960, 540]));
    for stage in ["load", "fonts", "frame", "paint", "png", "total"] {
        assert!(summary["ms"][stage].as_f64().is_some_and(|ms| ms >= 0.0), "{stage}: {summary}");
    }
}

#[test]
fn unimplemented_paths_exit_3_and_name_their_plan_task() {
    for (args, task) in [
        (vec!["--state", "chart"], "PLAN 0.10"),
        (vec!["--state", "mesh"], "PLAN 0.11"),
        (vec!["--state", "axes", "--painter", "gpu"], "0.7"),
    ] {
        let dir = scratch(&format!("unimplemented-{}", args[1]));
        let png = dir.join("x.png");
        let out = scaena(&[&["render", BUNDLE, "--out", png.to_str().unwrap()], args.as_slice()].concat());
        assert_eq!(code(&out), 3, "{args:?}: {}", stderr(&out));
        assert!(stderr(&out).contains(task), "{args:?}: {}", stderr(&out));
        assert!(!png.exists(), "{args:?} wrote a PNG");
    }
}

#[test]
fn invalid_input_exits_2() {
    for args in [
        vec!["--state", "nope"],
        vec!["--state", "axes", "--size", "1000x1000"],
        vec!["--state", "axes", "--size", "0x0"],
        vec!["--state", "axes", "--size", "wide"],
        vec!["--state", "axes", "--t", "NaN"],
        vec!["--state", "axes", "--t", "inf"],
    ] {
        let out = scaena(&[&["render", BUNDLE], args.as_slice()].concat());
        assert_eq!(code(&out), 2, "{args:?}: {}", stderr(&out));
    }
}
