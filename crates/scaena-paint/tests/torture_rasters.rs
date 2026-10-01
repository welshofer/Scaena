//! Golden rasters for the torture deck (PLAN 0.6), painted by `vello_cpu` straight
//! from the golden display lists in `tests/golden/torture/`, so this crate tests the
//! painter alone, independent of the engine that wrote them.
//!
//! Rasters are compared with the SPEC §13.5 metric (`scaena_paint::diff`), not byte
//! for byte: SIMD paths differ across CPUs. After a reviewed change, bless with
//! `SCAENA_BLESS=1 cargo test -p scaena-paint --test torture_rasters`; on a mismatch
//! the new raster is written to `actual/` beside the goldens.

mod common;

use common::{GOLDEN, fonts, goldens};
use scaena_core::displaylist::DisplayList;
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{FontStore, Painter, Raster, diff};
use std::sync::atomic::{AtomicUsize, Ordering};

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
