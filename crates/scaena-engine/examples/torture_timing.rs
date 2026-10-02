//! Per-stage timings on the torture deck (SPEC §15, benchmark B4), for the spike
//! report. Not a criterion bench; those arrive with the benchmark decks.
//!
//!     cargo run --release -p scaena-engine --example torture_timing

use scaena_core::Deck;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use std::time::{Duration, Instant};

fn main() {
    let bundle = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/fixtures/torture.scaena");
    let read = |path: &str| std::fs::read(format!("{bundle}/{path}")).expect(path);
    let deck = Deck::from_json(&String::from_utf8(read("deck.json")).unwrap()).unwrap();
    let theme = Theme::from_json(&String::from_utf8(read("theme.json")).unwrap()).unwrap();
    let fonts: Vec<(String, Vec<u8>)> = deck.fonts.iter().map(|f| (f.file.clone(), read(&f.file))).collect();
    let states: Vec<String> = deck.states.iter().map(|s| s.id.clone()).collect();
    let mut data = DataFiles::new();
    for source in deck.data.values() {
        if let Some(path) = source.source.as_str() {
            data.insert(path, read(path));
        }
    }

    let t = Instant::now();
    let mut bundle_fonts = BundleFonts::new();
    for (id, bytes) in &fonts {
        bundle_fonts.register(id, bytes.clone()).unwrap();
    }
    let register = t.elapsed();
    let mut engine = Engine::new(bundle_fonts);
    let req = |state| FrameRequest { deck: &deck, theme: &theme, data: &data, state, t_ms: f64::INFINITY };

    // Cold: first frame of each state on a fresh engine (parley's shaping caches empty).
    let mut cold = Vec::new();
    for state in &states {
        let t = Instant::now();
        engine.frame(&req(state)).unwrap();
        cold.push((state.as_str(), t.elapsed()));
    }
    // Warm: every state again, many times.
    const ROUNDS: u32 = 50;
    let t = Instant::now();
    for _ in 0..ROUNDS {
        for state in &states {
            engine.frame(&req(state)).unwrap();
        }
    }
    let warm_all = t.elapsed() / ROUNDS;
    let encode = {
        let dl = engine.frame(&req("pretty")).unwrap().display_list;
        let t = Instant::now();
        for _ in 0..1000 {
            dl.to_postcard().unwrap();
        }
        (t.elapsed() / 1000, dl.to_postcard().unwrap().len())
    };

    let ms = |d: Duration| d.as_secs_f64() * 1e3;
    println!("register 5 fonts (cold): {:.2} ms", ms(register));
    let cold_total: Duration = cold.iter().map(|(_, d)| *d).sum();
    let (worst, worst_d) = cold.iter().max_by_key(|(_, d)| *d).unwrap();
    println!(
        "first frame, {} states: {:.2} ms total, worst {worst} {:.2} ms",
        cold.len(),
        ms(cold_total),
        ms(*worst_d)
    );
    println!(
        "warm, all {} states: {:.2} ms ({:.3} ms per state)",
        states.len(),
        ms(warm_all),
        ms(warm_all) / states.len() as f64
    );
    println!("postcard encode, `pretty` state: {:.1} µs, {} bytes", encode.0.as_secs_f64() * 1e6, encode.1);

    // The next quarter (PLAN 0.10): laid out once, then every frame samples.
    let t = Instant::now();
    let transition = engine.transition(&deck, &theme, &data, "chart-next").unwrap();
    let build = t.elapsed();
    const FRAMES: u32 = 2000;
    let d = transition.duration_ms();
    let t = Instant::now();
    for i in 0..FRAMES {
        std::hint::black_box(transition.frame(d * f64::from(i) / f64::from(FRAMES)));
    }
    let sample = t.elapsed() / FRAMES;
    let mid = transition.frame(0.5 * d).to_postcard().unwrap().len();
    println!(
        "next-quarter transition: laid out once in {:.2} ms; a frame samples in {:.1} µs ({FRAMES} frames over {d} ms; \
         mid-frame postcard {mid} bytes)",
        ms(build),
        sample.as_secs_f64() * 1e6
    );
}
