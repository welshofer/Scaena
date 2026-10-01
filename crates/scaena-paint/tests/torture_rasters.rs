//! Golden rasters for the torture deck (PLAN 0.6), painted by `vello_cpu` straight
//! from the golden display lists in `tests/golden/torture/`, so this crate tests the
//! painter alone, independent of the engine that wrote them.
//!
//! Rasters are compared with the SPEC §13.5 metric (`scaena_paint::diff`), not byte
//! for byte: SIMD paths differ across CPUs. After a reviewed change, bless with
//! `SCAENA_BLESS=1 cargo test -p scaena-paint --test torture_rasters`; on a mismatch
//! the new raster is written to `actual/` beside the goldens.

use scaena_core::displaylist::DisplayList;
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{FontStore, Painter, Raster, diff};
use std::sync::atomic::{AtomicUsize, Ordering};

const BUNDLE: &str = "../../tests/fixtures/torture.scaena";
const GOLDEN: &str = "../../tests/golden/torture";

fn goldens() -> Vec<(String, DisplayList)> {
    let mut out: Vec<(String, DisplayList)> = std::fs::read_dir(GOLDEN)
        .unwrap()
        .filter_map(|e| {
            let name = e.unwrap().file_name().into_string().unwrap();
            let state = name.strip_suffix(".dl.json")?.to_string();
            let json = std::fs::read_to_string(format!("{GOLDEN}/{name}")).unwrap();
            Some((state, DisplayList::from_json(&json).unwrap()))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

fn fonts(dls: &[(String, DisplayList)]) -> FontStore {
    let mut store = FontStore::new();
    let ids: std::collections::BTreeSet<&str> =
        dls.iter().flat_map(|(_, dl)| dl.fonts.iter().map(|f| f.id.as_str())).collect();
    for id in ids {
        store.insert(id, std::fs::read(format!("{BUNDLE}/{id}")).unwrap());
    }
    store
}

/// Paints one state and blesses or checks its golden; returns a failure line if it fails.
fn check(state: &str, dl: &DisplayList, store: &FontStore, bless: bool) -> Option<String> {
    let raster = CpuPainter::default().paint(dl, store, 1.0).unwrap_or_else(|e| panic!("{state}: {e}"));
    let path = format!("{GOLDEN}/{state}.png");
    if bless {
        std::fs::write(&path, raster.to_png().unwrap()).unwrap();
        return None;
    }
    let golden =
        Raster::from_png(&std::fs::read(&path).unwrap_or_else(|_| panic!("missing {path}; bless first"))).unwrap();
    let d = diff::compare(&golden, &raster).unwrap();
    println!("{state}: {d}");
    if d.passes() {
        return None;
    }
    std::fs::create_dir_all(format!("{GOLDEN}/actual")).unwrap();
    std::fs::write(format!("{GOLDEN}/actual/{state}.png"), raster.to_png().unwrap()).unwrap();
    Some(format!("{state}: {d}"))
}

#[test]
fn rasters_match_goldens_within_spec_tolerance() {
    let dls = goldens();
    assert!(dls.len() >= 20, "found {} display-list goldens", dls.len());
    let store = fonts(&dls);
    let bless = std::env::var_os("SCAENA_BLESS").is_some();
    // States are independent: one worker per core takes the next state off a shared index.
    let next = AtomicUsize::new(0);
    let workers = std::thread::available_parallelism().map_or(1, |n| n.get()).min(dls.len());
    let mut failures: Vec<String> = std::thread::scope(|s| {
        let handles: Vec<_> = (0..workers)
            .map(|_| {
                s.spawn(|| {
                    let mut failures = Vec::new();
                    while let Some((state, dl)) = dls.get(next.fetch_add(1, Ordering::Relaxed)) {
                        failures.extend(check(state, dl, &store, bless));
                    }
                    failures
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().unwrap()).collect()
    });
    failures.sort();
    assert!(failures.is_empty(), "rasters outside SPEC §13.5 tolerance:\n{}", failures.join("\n"));
}

/// A GPU painter, or `None` with the reason printed on a machine without an adapter.
/// `SCAENA_REQUIRE_GPU=1` (set in CI) makes a missing adapter a failure instead.
#[cfg(feature = "gpu")]
fn gpu_painter() -> Option<scaena_paint::gpu::GpuPainter> {
    match scaena_paint::gpu::GpuPainter::new() {
        Ok(p) => Some(p),
        Err(e) if std::env::var_os("SCAENA_REQUIRE_GPU").is_none() => {
            eprintln!("skipping: {e} (set SCAENA_REQUIRE_GPU=1 to fail instead)");
            None
        }
        Err(e) => panic!("SCAENA_REQUIRE_GPU is set: {e}"),
    }
}

/// PLAN 0.7: `vello` on the GPU paints the same display lists, judged against the CPU
/// goldens by the same SPEC §13.5 metric. The goldens stay the CPU painter's (SPEC
/// §13.6: export frames come from the CPU); a GPU raster outside tolerance is written
/// to `actual/<state>.gpu.png`.
#[cfg(feature = "gpu")]
#[test]
fn gpu_rasters_match_the_cpu_goldens_within_spec_tolerance() {
    let Some(mut gpu) = gpu_painter() else { return };
    let info = gpu.adapter();
    println!("adapter: {} ({:?}, {:?})", info.name, info.backend, info.device_type);
    let dls = goldens();
    let store = fonts(&dls);
    let mut failures = Vec::new();
    for (state, dl) in &dls {
        let raster = gpu.paint(dl, &store, 1.0).unwrap_or_else(|e| panic!("{state}: {e}"));
        let golden = Raster::from_png(&std::fs::read(format!("{GOLDEN}/{state}.png")).unwrap()).unwrap();
        let d = diff::compare(&golden, &raster).unwrap();
        println!("gpu {state}: {d}");
        if !d.passes() {
            std::fs::create_dir_all(format!("{GOLDEN}/actual")).unwrap();
            std::fs::write(format!("{GOLDEN}/actual/{state}.gpu.png"), raster.to_png().unwrap()).unwrap();
            failures.push(format!("{state}: {d}"));
        }
    }
    assert!(
        failures.is_empty(),
        "GPU rasters outside SPEC §13.5 tolerance of the CPU goldens:\n{}",
        failures.join("\n")
    );
}
