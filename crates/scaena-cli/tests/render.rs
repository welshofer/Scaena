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
    assert_eq!((&summary["painter"], &summary["adapter"]), (&serde_json::json!("cpu"), &serde_json::Value::Null));
    for stage in ["load", "fonts", "frame", "init", "paint", "png", "total"] {
        assert!(summary["ms"][stage].as_f64().is_some_and(|ms| ms >= 0.0), "{stage}: {summary}");
    }
}

/// `--t` reaches the sampler: a quarter of the 420 ms transition to the next quarter
/// is the `chart-next@0.25` golden (PLAN 0.10).
#[test]
fn t_renders_a_frame_inside_the_transition() {
    let dir = scratch("morph");
    let (png, dl) = (dir.join("morph.png"), dir.join("morph.dl.json"));
    let out = scaena(&[
        "render",
        BUNDLE,
        "--state",
        "chart-next",
        "--t",
        "105",
        "--out",
        png.to_str().unwrap(),
        "--display-list",
        dl.to_str().unwrap(),
    ]);
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let mut list = DisplayList::from_json(&std::fs::read_to_string(&dl).unwrap()).unwrap();
    quantize(&mut list);
    let golden = std::fs::read_to_string(format!("{GOLDEN}/chart-next@0.25.dl.json")).unwrap();
    assert_eq!(list.to_golden_json().unwrap(), golden);
}

/// A copy of the torture bundle in `dir`, its deck edited.
fn edited_bundle(dir: &Path, edit: impl FnOnce(&mut serde_json::Value)) -> PathBuf {
    let bundle = dir.join("bundle");
    for sub in ["fonts", "data", "assets"] {
        std::fs::create_dir_all(bundle.join(sub)).unwrap();
        for entry in std::fs::read_dir(Path::new(BUNDLE).join(sub)).unwrap() {
            let path = entry.unwrap().path();
            std::fs::copy(&path, bundle.join(sub).join(path.file_name().unwrap())).unwrap();
        }
    }
    std::fs::copy(Path::new(BUNDLE).join("theme.json"), bundle.join("theme.json")).unwrap();
    let deck = std::fs::read_to_string(Path::new(BUNDLE).join("deck.json")).unwrap();
    let mut deck: serde_json::Value = serde_json::from_str(&deck).unwrap();
    edit(&mut deck);
    std::fs::write(bundle.join("deck.json"), deck.to_string()).unwrap();
    bundle
}

#[test]
fn unimplemented_paths_exit_3_and_name_their_plan_task() {
    let dir = scratch("unimplemented");
    // A preset called with `params` waits for PLAN 1.12, at rest too: the state's motions
    // set when it comes to rest.
    let params = edited_bundle(&dir, |deck| {
        let mesh = deck["states"].as_array_mut().unwrap().iter_mut().find(|s| s["id"] == "mesh").unwrap();
        mesh["choreography"] =
            serde_json::json!([{ "target": "mesh-bg", "emphasis": { "preset": "fade", "params": { "k": 1 } } }]);
    });
    let mut cases = vec![(params, vec!["--state", "mesh"], "PLAN 1.12")];
    // Without the `gpu` feature the GPU painter is not compiled in.
    if cfg!(not(feature = "gpu")) {
        cases.push((PathBuf::from(BUNDLE), vec!["--state", "axes", "--painter", "gpu"], "--features gpu"));
    }
    for (bundle, args, task) in cases {
        let png = dir.join("x.png");
        let out =
            scaena(&[&["render", bundle.to_str().unwrap(), "--out", png.to_str().unwrap()], args.as_slice()].concat());
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

/// With the `gpu` feature, `--painter gpu` paints the same display list with vello on
/// the GPU, within tolerance of the CPU golden. On a machine with no adapter it skips,
/// unless `SCAENA_REQUIRE_GPU` is set (CI sets it).
#[cfg(feature = "gpu")]
#[test]
fn the_gpu_painter_renders_within_tolerance_of_the_cpu_golden() {
    let dir = scratch("gpu");
    let png = dir.join("pretty.png");
    let out =
        scaena(&["render", BUNDLE, "--state", "pretty", "--painter", "gpu", "--out", png.to_str().unwrap(), "--json"]);
    if code(&out) == 2 && stderr(&out).contains("no adapter") && std::env::var_os("SCAENA_REQUIRE_GPU").is_none() {
        eprintln!("skipping: {}", stderr(&out));
        return;
    }
    assert_eq!(code(&out), 0, "{}", stderr(&out));
    let summary: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(summary["painter"], "gpu");
    assert!(summary["adapter"].is_string(), "{summary}");
    let raster = Raster::from_png(&std::fs::read(&png).unwrap()).unwrap();
    let golden = Raster::from_png(&std::fs::read(format!("{GOLDEN}/pretty.png")).unwrap()).unwrap();
    let d = diff::compare(&golden, &raster).unwrap();
    assert!(d.passes(), "{d}");
}
