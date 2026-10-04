//! The parity harness (PLAN 0.9): every torture state painted three ways and compared
//! pairwise with the SPEC §13.5 metric (`scaena_paint::diff`).
//!
//! - `cpu`: `vello_cpu`, as the golden rasters in `tests/golden/torture/` hold it
//!   (`torture_rasters` keeps those goldens honest).
//! - `gpu`: `vello` on this machine's adapter, with `--features gpu`. Without an adapter
//!   the source is skipped and says so, unless `SCAENA_REQUIRE_GPU=1` (CI sets it).
//! - the browser's: frames painted in Chromium and saved into the directories
//!   `SCAENA_WEB_PNGS` lists (`:`-separated, as `PATH` is), a source each, named by its
//!   directory. `crates/scaena-wasm/www/smoke.mjs` reads `vello` on WebGPU back from its
//!   page's canvas into `wasm-smoke/`; `web/smoke.mjs` screenshots the web player's frames
//!   into `player-webgpu/` and `player-cpu/` (PLAN 2.1). Skipped when the variable is unset;
//!   required, state by state, for each directory it names.
//!
//! A failing pair writes its diff image (`diff::image`: red over ΔE 1, magenta for a
//! half-range step, blue for tolerated differences) to
//! `tests/golden/torture/actual/<state>.<a>-<b>.png`. `just spike` runs the first three.

mod common;

use common::{GOLDEN, assets, goldens};
use scaena_paint::{Painter, Raster, diff};

/// The native GPU painter, if this build and machine have one.
fn gpu() -> Option<(Box<dyn Painter>, String)> {
    #[cfg(feature = "gpu")]
    {
        match scaena_paint::gpu::GpuPainter::new() {
            Ok(p) => {
                let info = p.adapter();
                let name = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
                return Some((Box::new(p), name));
            }
            Err(e) if std::env::var_os("SCAENA_REQUIRE_GPU").is_none() => {
                eprintln!("gpu: skipped: {e} (set SCAENA_REQUIRE_GPU=1 to fail instead)");
            }
            Err(e) => panic!("SCAENA_REQUIRE_GPU is set: {e}"),
        }
    }
    #[cfg(not(feature = "gpu"))]
    eprintln!("gpu: skipped: built without the `gpu` feature");
    None
}

#[test]
fn painters_agree_within_spec_tolerance() {
    let dls = goldens();
    assert!(dls.len() >= 20, "found {} display-list goldens", dls.len());
    let store = assets(&dls);
    let mut gpu = gpu();
    let web: Vec<(String, std::path::PathBuf)> = std::env::var_os("SCAENA_WEB_PNGS")
        .map(|dirs| {
            let named = |dir: std::path::PathBuf| (dir.file_name().unwrap().to_string_lossy().into_owned(), dir);
            std::env::split_paths(&dirs).map(named).collect()
        })
        .unwrap_or_default();
    println!("cpu: vello_cpu goldens");
    if let Some((_, adapter)) = &gpu {
        println!("gpu: {adapter}");
    }
    for (name, dir) in &web {
        println!("{name}: {}", dir.display());
    }
    if web.is_empty() {
        eprintln!("the browser's: skipped: SCAENA_WEB_PNGS is unset (`just spike` and `just web-smoke` set it)");
    }

    let mut failures = Vec::new();
    for (state, dl) in &dls {
        let read = |path: &std::path::Path| {
            let bytes = std::fs::read(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            Raster::from_png(&bytes).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
        };
        let mut rasters = vec![("cpu", read(format!("{GOLDEN}/{state}.png").as_ref()))];
        if let Some((painter, _)) = gpu.as_mut() {
            rasters.push(("gpu", painter.paint(dl, &store, 1.0).unwrap_or_else(|e| panic!("{state}: {e}"))));
        }
        for (name, dir) in &web {
            rasters.push((name, read(&dir.join(format!("{state}.png")))));
        }
        let mut row = format!("{state:<14}");
        for (i, (name_a, a)) in rasters.iter().enumerate() {
            for (name_b, b) in &rasters[i + 1..] {
                let d = diff::compare(a, b).unwrap();
                let verdict = if d.passes() { "ok" } else { "FAIL" };
                row.push_str(&format!(
                    "  {name_a}-{name_b} {verdict} {:>3} over ΔE 1, max step {:>3}",
                    d.over, d.max_channel
                ));
                if !d.passes() {
                    let path = format!("{GOLDEN}/actual/{state}.{name_a}-{name_b}.png");
                    std::fs::create_dir_all(format!("{GOLDEN}/actual")).unwrap();
                    std::fs::write(&path, diff::image(a, b).unwrap().to_png().unwrap()).unwrap();
                    failures.push(format!("{state} {name_a}-{name_b}: {d} (diff image: {path})"));
                }
            }
        }
        println!("{row}");
    }
    assert!(failures.is_empty(), "painters disagree beyond SPEC §13.5:\n{}", failures.join("\n"));
}
