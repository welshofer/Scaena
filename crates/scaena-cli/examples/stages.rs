//! Per-stage timings on one bundle against SPEC §15's budgets (PLAN 0.14): the numbers
//! behind gate 0 criterion 6. Release builds only:
//!
//!     cargo build --release -p scaena-cli --features gpu --bins --examples
//!     target/release/examples/stages tests/bench/b1.scaena
//!
//! Prints a Markdown table. Every stage runs in this process except the cold render,
//! which starts `scaena render` (the binary beside `examples/`) once per state and
//! times the whole process. Without the `gpu` feature or an adapter, the GPU row says
//! why. Not a criterion bench: medians and worst cases, no statistics, no baseline.

use anyhow::{Context, Result};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{Assets, Painter};
use scaena_store::Bundle;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

/// Milliseconds `f` takes.
fn ms<T>(f: impl FnOnce() -> T) -> (f64, T) {
    let t = Instant::now();
    let out = f();
    (t.elapsed().as_secs_f64() * 1e3, out)
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    let n = v.len();
    if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 }
}

/// Median and worst of per-state numbers, and the state that was worst.
fn spread(per_state: &[(String, f64)]) -> (f64, f64, String) {
    let (worst, at) = per_state.iter().map(|(s, v)| (*v, s.clone())).max_by(|a, b| a.0.total_cmp(&b.0)).unwrap();
    (median(per_state.iter().map(|(_, v)| *v).collect()), worst, at)
}

struct Loaded {
    deck: scaena_core::Deck,
    theme: Theme,
    data: DataFiles,
    fonts: Vec<(String, Vec<u8>)>,
    images: Vec<(String, Vec<u8>)>,
}

fn load(path: &Path) -> Result<Loaded> {
    let b = Bundle::open(path)?;
    let theme = Theme::from_json(b.theme_json.as_deref().context("the deck names no theme")?)?;
    let fonts = b.read_fonts()?;
    let images = b.read_images()?;
    let mut data = DataFiles::new();
    for (p, bytes) in b.read_data()? {
        data.insert(p, bytes);
    }
    Ok(Loaded { deck: b.deck, theme, data, fonts, images })
}

/// The frame of `state` at rest.
fn req<'a>(l: &'a Loaded, state: &'a str) -> FrameRequest<'a> {
    FrameRequest { deck: &l.deck, theme: &l.theme, data: &l.data, state, t_ms: f64::INFINITY }
}

fn register(l: &Loaded) -> Result<BundleFonts> {
    let mut fonts = BundleFonts::new();
    for (id, bytes) in &l.fonts {
        fonts.register(id, bytes.clone())?;
    }
    fonts.check_theme(&l.theme)?;
    Ok(fonts)
}

fn images(l: &Loaded) -> Result<BundleImages> {
    let mut images = BundleImages::new();
    for (path, bytes) in &l.images {
        images.register(path, bytes)?;
    }
    Ok(images)
}

fn main() -> Result<()> {
    let path = PathBuf::from(std::env::args().nth(1).context("usage: stages <bundle>")?);
    let mut rows: Vec<(String, String, String, &str)> = Vec::new();
    let mut row = |stage: &str, median: String, worst: String, budget: &'static str| {
        rows.push((stage.to_string(), median, worst, budget))
    };
    let f = |v: f64| format!("{v:.2} ms");

    // Bundle load and font registration: what a cold start pays before any layout.
    let loads: Vec<f64> = (0..10).map(|_| ms(|| load(&path)).0).collect();
    let l = load(&path)?;
    let states: Vec<String> = l.deck.states.iter().map(|s| s.id.clone()).collect();
    let font_kb = l.fonts.iter().map(|(_, b)| b.len()).sum::<usize>() / 1024;
    row(
        &format!("Load bundle (deck, theme, {} fonts {font_kb} KB)", l.fonts.len()),
        f(median(loads)),
        "—".into(),
        "cold start",
    );
    let regs: Vec<f64> = (0..10).map(|_| ms(|| register(&l)).0).collect();
    row("Register and check fonts", f(median(regs)), "—".into(), "cold start");

    // First pass on a fresh engine: shaping caches start empty and fill as states go by.
    let mut engine = Engine::new(register(&l)?).with_images(images(&l)?);
    let mut first = Vec::new();
    for s in &states {
        let (t, frame) = ms(|| engine.frame(&req(&l, s)));
        frame?;
        first.push((s.clone(), t));
    }
    let (m, w, at) = spread(&first);
    let total: f64 = first.iter().map(|(_, t)| t).sum();
    row("Resolve + layout one snapshot, first pass (fresh engine)", f(m), format!("{} (`{at}`)", f(w)), "—");
    row(&format!("Resolve + layout all {} snapshots, first pass", states.len()), f(total), "—".into(), "≤ 400 ms (B1)");
    // Warm: each state laid out again, median of 7.
    let mut warm = Vec::new();
    for s in &states {
        let runs: Vec<f64> = (0..7).map(|_| ms(|| black_box(engine.frame(&req(&l, s)))).0).collect();
        warm.push((s.clone(), median(runs)));
    }
    let (m, w, at) = spread(&warm);
    row("Resolve + layout one snapshot, warm", f(m), format!("{} (`{at}`)", f(w)), "≤ 15 ms (B1)");
    row(
        &format!("Resolve + layout all {} snapshots, warm", states.len()),
        f(warm.iter().map(|(_, t)| t).sum()),
        "—".into(),
        "≤ 400 ms (B1)",
    );

    // Transitions: built once (two snapshots laid out), then every frame samples.
    let (mut builds, mut samples) = (Vec::new(), Vec::new());
    for s in &states {
        let (t, transition) = ms(|| engine.transition(&l.deck, &l.theme, &l.data, s));
        let transition = transition?;
        let d = transition.duration_ms();
        if d <= 0.0 {
            continue;
        }
        builds.push((s.clone(), t));
        const FRAMES: u32 = 200;
        let (t, ()) = ms(|| {
            for i in 0..FRAMES {
                black_box(transition.frame(d * f64::from(i) / f64::from(FRAMES)));
            }
        });
        samples.push((s.clone(), t / f64::from(FRAMES)));
    }
    if !samples.is_empty() {
        let (m, w, at) = spread(&builds);
        row(
            &format!("Build a transition (lay out both ends), {} transitions", builds.len()),
            f(m),
            format!("{} (`{at}`)", f(w)),
            "—",
        );
        let (m, w, at) = spread(&samples);
        row(
            "Sample one frame (interpolate laid-out geometry)",
            format!("{:.1} µs", m * 1e3),
            format!("{:.1} µs (`{at}`)", w * 1e3),
            "≤ 1 ms (B1, B2)",
        );
    }

    // Painting: one 1080p frame per state at rest, warm (median of 3 after one warm-up).
    let store = {
        let mut store = Assets::new();
        for (id, bytes) in &l.fonts {
            store.insert_font(id, bytes.clone());
        }
        for (path, bytes) in &l.images {
            store.insert_image(&images(&l)?.get(path).context("registered above")?.id, bytes)?;
        }
        store
    };
    let dls: Vec<(String, scaena_core::displaylist::DisplayList)> =
        states.iter().map(|s| Ok((s.clone(), engine.frame(&req(&l, s))?.display_list))).collect::<Result<_>>()?;
    let mut cpu = CpuPainter::default();
    let (mut paints, mut pngs) = (Vec::new(), Vec::new());
    for (s, dl) in &dls {
        cpu.paint(dl, &store, 1.0)?;
        let runs: Vec<f64> = (0..3).map(|_| ms(|| black_box(cpu.paint(dl, &store, 1.0))).0).collect();
        paints.push((s.clone(), median(runs)));
        let raster = cpu.paint(dl, &store, 1.0)?;
        let runs: Vec<f64> = (0..3).map(|_| ms(|| black_box(raster.to_png_fast())).0).collect();
        pngs.push((s.clone(), median(runs)));
    }
    let (m, w, at) = spread(&paints);
    let level = format!("{:?}", cpu.level);
    let level = level.split('(').next().unwrap_or_default();
    row(
        &format!("CPU paint, one frame at 1080p (vello_cpu, {level}, 1 thread)"),
        f(m),
        format!("{} (`{at}`)", f(w)),
        "≤ 12 ms with 8 threads (B1)",
    );
    let (m, w, at) = spread(&pngs);
    row("PNG encode (fast compression)", f(m), format!("{} (`{at}`)", f(w)), "—");
    gpu_rows(&dls, &store, &mut row);

    // Lint: validation and the document-level rules.
    let lints: Vec<f64> = (0..20).map(|_| ms(|| black_box(scaena_core::lint::lint_document(&l.deck))).0).collect();
    row("Validate + document-level lint", f(median(lints)), "—".into(), "≤ 100 ms (B1)");

    // Cold render: the whole `scaena render` process, once per state.
    let cli = std::env::current_exe()?.parent().and_then(Path::parent).map(|d| d.join("scaena"));
    match cli.filter(|c| c.exists()) {
        Some(cli) => {
            let out = std::env::temp_dir().join("scaena-stages.png");
            let render = |s: &str| -> Result<f64> {
                let (t, status) =
                    ms(|| Command::new(&cli).arg("render").arg(&path).args(["--state", s, "--out"]).arg(&out).output());
                let status = status?;
                anyhow::ensure!(status.status.success(), "render {s}: {}", String::from_utf8_lossy(&status.stderr));
                Ok(t)
            };
            render(&states[0])?; // the binary into the page cache; not counted
            let colds: Vec<(String, f64)> =
                states.iter().map(|s| Ok((s.clone(), render(s)?))).collect::<Result<_>>()?;
            let (m, w, at) = spread(&colds);
            row(
                "Headless PNG render, cold (`scaena render`, whole process)",
                f(m),
                format!("{} (`{at}`)", f(w)),
                "≤ 300 ms (B1)",
            );
        }
        None => row(
            "Headless PNG render, cold",
            "not run: build the `scaena` binary first".into(),
            "—".into(),
            "≤ 300 ms (B1)",
        ),
    }

    let host = format!(
        "{} {}, {} threads available",
        std::env::consts::ARCH,
        std::env::consts::OS,
        std::thread::available_parallelism().map_or(0, |n| n.get())
    );
    println!(
        "**{}** (`{}`): {} states; {host}\n",
        l.deck.meta.as_ref().and_then(|m| m.title.as_deref()).unwrap_or("deck"),
        path.display(),
        states.len()
    );
    println!("| Stage | Median | Worst | SPEC §15 budget |\n|---|---|---|---|");
    for (stage, median, worst, budget) in rows {
        println!("| {stage} | {median} | {worst} | {budget} |");
    }
    Ok(())
}

#[cfg(feature = "gpu")]
fn gpu_rows(
    dls: &[(String, scaena_core::displaylist::DisplayList)],
    store: &Assets,
    row: &mut impl FnMut(&str, String, String, &'static str),
) {
    let mut gpu = match scaena_paint::gpu::GpuPainter::new() {
        Ok(gpu) => gpu,
        Err(e) => {
            return row("GPU paint + readback, one frame at 1080p", format!("not run: {e}"), "—".into(), "≤ 6 ms");
        }
    };
    let info = gpu.adapter().clone();
    let mut paints = Vec::new();
    for (s, dl) in dls {
        if let Err(e) = gpu.paint(dl, store, 1.0) {
            return row(
                "GPU paint + readback, one frame at 1080p",
                format!("failed on `{s}`: {e}"),
                "—".into(),
                "≤ 6 ms",
            );
        }
        let runs: Vec<f64> = (0..3).map(|_| ms(|| black_box(gpu.paint(dl, store, 1.0))).0).collect();
        paints.push((s.clone(), median(runs)));
    }
    let (m, w, at) = spread(&paints);
    row(
        &format!("GPU paint + readback, one frame at 1080p ({}, {:?})", info.name, info.backend),
        format!("{m:.2} ms"),
        format!("{w:.2} ms (`{at}`)"),
        "≤ 6 ms, paint alone (B1)",
    );
}

#[cfg(not(feature = "gpu"))]
fn gpu_rows(
    _: &[(String, scaena_core::displaylist::DisplayList)],
    _: &Assets,
    row: &mut impl FnMut(&str, String, String, &'static str),
) {
    row("GPU paint, one frame at 1080p", "not run: build with `--features gpu`".into(), "—".into(), "≤ 6 ms");
}
