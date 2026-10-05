//! Golden rasters for the torture deck (PLAN 0.6), painted by `vello_cpu` straight
//! from the golden display lists in `tests/golden/torture/`, so this crate tests the
//! painter alone, independent of the engine that wrote them.
//!
//! Rasters are compared with the SPEC §13.5 metric (`scaena_paint::diff`), not byte
//! for byte: SIMD paths differ across CPUs. After a reviewed change, bless with
//! `SCAENA_BLESS=1 cargo test -p scaena-paint --test torture_rasters`; on a mismatch
//! the new raster is written to `actual/` beside the goldens.

mod common;

use common::{GOLDEN, assets, goldens};
use scaena_core::displaylist::{Blend, Color, DisplayList, Glyph, Op, Paint};
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{Assets, PaintError, Painter, Raster, diff};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Paints one state and blesses or checks its golden; returns a failure line if it fails.
fn check(state: &str, dl: &DisplayList, store: &Assets, bless: bool) -> Option<String> {
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
    let store = assets(&dls);
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

/// A painter keeps its render context from frame to frame (`Kept`), reset between them. One
/// painter that paints every state in turn, through the formats' sizes and a frame that
/// stops with an error inside a layer, draws each byte for byte as a new painter does.
#[test]
fn a_kept_context_paints_what_a_new_one_does() {
    let dls = goldens();
    let store = assets(&dls);
    // A frame that stops partway: a layer pushed, then glyphs in a font the list lacks.
    let glyphs = Op::Glyphs {
        font: 9,
        size: 12.0,
        coords: Vec::new(),
        paint: Paint::Solid(Color([0, 0, 0, 255])),
        text: String::new(),
        glyphs: vec![Glyph { id: 1, x: 0.0, y: 12.0 }],
        clusters: Vec::new(),
    };
    let layer = Op::Layer {
        node: None,
        cell: None,
        transform: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        opacity: 0.5,
        blend: Blend::Normal,
        clip: None,
        ops: vec![glyphs],
    };
    let mut kept = CpuPainter::default();
    let mut sizes = std::collections::BTreeSet::new();
    for (i, (state, dl)) in dls.iter().enumerate() {
        if i % 7 == 3 {
            // The size of the frame after it, so that frame gets the context it left.
            let mut broken = DisplayList::new(dl.viewport);
            broken.ops.push(layer.clone());
            assert!(matches!(kept.paint(&broken, &store, 1.0), Err(PaintError::FontIndex(9))));
        }
        let again = kept.paint(dl, &store, 1.0).unwrap_or_else(|e| panic!("{state}: {e}"));
        let new = CpuPainter::default().paint(dl, &store, 1.0).unwrap();
        sizes.insert((new.width, new.height));
        assert!(again.rgba == new.rgba, "{state}: a kept context paints other bytes than a new one");
    }
    assert!(sizes.len() > 1, "the states change the frame's size: {sizes:?}");
}
