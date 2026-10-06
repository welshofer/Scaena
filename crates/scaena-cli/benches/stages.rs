//! SPEC §15's stages, timed by criterion on the benchmark decks (PLAN 1.24): B1 (text),
//! B2 (charts), B3 (shaders), and B4 (the torture deck). CI runs them on each runner and
//! judges every run against that runner's history of `main` (`scripts/bench_gate.py`).
//!
//!     cargo bench -p scaena-cli --features gpu --bench stages       # every stage, every deck
//!     cargo bench -p scaena-cli --bench stages -- layout/b1         # one bench
//!     cargo test -p scaena-cli --bench stages                       # each once, as a test
//!
//! A bench is `stage/deck`. A stage that goes over a deck's states, cues, or frames counts
//! them as criterion's elements, so its time per element is the time per state, cue, or
//! frame. A `_one` stage times the deck's slowest state, found by timing each beforehand.
//! Frames are 1080 pixels high and painted at rest.
//!
//! | Stage | One iteration | SPEC §15 |
//! |---|---|---|
//! | `load` | open the bundle; read its fonts, images, and data; parse its theme | cold start |
//! | `fonts` | register the fonts and check the theme's roles against them | cold start |
//! | `layout_fresh` | lay out every state on a fresh engine, its shaping caches empty | — |
//! | `layout` | lay out every state, warm | all snapshots ≤ 400 ms (B1) |
//! | `layout_one` | lay out the slowest state, warm | one snapshot ≤ 15 ms (B1) |
//! | `transition` | build every cue: lay out both its ends | — |
//! | `sample` | sample 60 frames of every cue | ≤ 1 ms a frame (B1, B2) |
//! | `cpu_paint` | paint every state with `vello_cpu`, on one thread | — |
//! | `cpu_paint_one` | paint the slowest state | ≤ 12 ms (B1, B2), ≤ 25 ms (B3) |
//! | `gpu_paint` | paint every state with vello and read it back (`--features gpu`, an adapter) | ≤ 6 ms a frame |
//! | `lint_document` | validation and the document rules | ≤ 100 ms (B1) |
//! | `lint_layout` | the layout rules, in every format, contrast painted | ≤ 1 s (B1) |
//! | `render_cold` | `scaena render` the slowest state: the whole process | ≤ 300 ms (B1) |
//! | `mcp_render` | start `scaena mcp`, `deck_render` the slowest state, stop it | ≤ 1 s (B1) |
//! | `video` | the longest cue at 1080p60: frames sampled, painted on every core, and handed to an ffmpeg that discards them | ≥ 1× realtime (B1, B2), ≥ 0.5× (B3) |
//! | `video_gpu` | the same, painted by vello on the GPU, each frame while the ones before it are read back (`--features gpu`, an adapter) | ≥ 2× realtime |
//! | `probe/cpu` | sort a fixed list: how fast the machine is, not the code | — |
//!
//! The decks are read from this checkout, or, where `SCAENA_BENCH_DECKS` names a directory,
//! from that one, laid out as the repository's root. CI times a pull request's build on its
//! base's decks, so the two builds run the same work.
//!
//! SPEC's CPU paint budget is for 8 threads. The painter paints a frame on one; the video
//! stage paints its frames on every core.

use criterion::measurement::WallTime;
use criterion::{BatchSize, BenchmarkGroup, Criterion, SamplingMode, Throughput};
use scaena_core::displaylist::DisplayList;
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::sample::Transition;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, FrameRequest};
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{Assets, Painter};
use scaena_store::Bundle;
use std::cell::RefCell;
use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// The benchmark decks, by the names SPEC §15 gives them.
const DECKS: [(&str, &str); 4] = [
    ("b1", "tests/bench/b1.scaena"),
    ("b2", "tests/bench/b2.scaena"),
    ("b3", "tests/bench/b3.scaena"),
    ("b4", "tests/fixtures/torture.scaena"),
];

/// The directory the decks' paths are under: `SCAENA_BENCH_DECKS`, or this checkout's root.
fn root() -> PathBuf {
    match std::env::var_os("SCAENA_BENCH_DECKS") {
        Some(dir) => PathBuf::from(dir),
        None => Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."),
    }
}

/// Frames sampled from each cue, and a video's frames a second.
const FRAMES: u32 = 60;

/// A deck as the stages use it: read once, laid out warm, its slowest parts found.
struct Deck {
    name: &'static str,
    path: PathBuf,
    bundle: Bundle,
    theme: Theme,
    data: DataFiles,
    states: Vec<String>,
    /// Has laid out every state: its shaping caches are warm.
    engine: RefCell<Engine>,
    assets: Assets,
    /// Device pixels to the canvas unit: frames are 1080 pixels high.
    scale: f32,
    /// Every state at rest.
    lists: Vec<DisplayList>,
    /// Every cue (a state's transition and motions, where it has any), by its state.
    cues: Vec<(String, Transition)>,
    /// The state slowest to lay out, and the one slowest to paint.
    slowest_layout: String,
    slowest_paint: String,
    /// The longest cue, by its index in `cues`: what the video stage plays.
    longest_cue: Option<usize>,
}

impl Deck {
    fn open(name: &'static str, rel: &str) -> Deck {
        let path = root().join(rel);
        let bundle = Bundle::open(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let theme = scaena_ops::theme(&bundle).unwrap();
        let data = scaena_ops::lint::data_files(&bundle).unwrap();
        let mut assets = Assets::new();
        let mut engine = scaena_ops::lint::engine_with(&bundle, &theme, Some(&mut assets)).unwrap();
        let states: Vec<String> = bundle.deck.states.iter().map(|s| s.id.clone()).collect();
        let scale = 1080.0 / bundle.deck.canvas.height as f32;
        let req = |state| FrameRequest {
            deck: &bundle.deck,
            theme: &theme,
            data: &data,
            state,
            t_ms: f64::INFINITY,
            format: None,
        };
        // Each state laid out once to warm the caches, then timed: the best of three.
        let mut layouts = Vec::new();
        for s in &states {
            engine.frame(&req(s)).unwrap();
            layouts.push((0..3).map(|_| seconds(|| engine.frame(&req(s)))).fold(f64::INFINITY, f64::min));
        }
        let lists: Vec<DisplayList> = states.iter().map(|s| engine.frame(&req(s)).unwrap().display_list).collect();
        let mut cpu = CpuPainter::default();
        let paints: Vec<f64> = lists
            .iter()
            .map(|dl| {
                cpu.paint(dl, &assets, scale).unwrap();
                seconds(|| cpu.paint(dl, &assets, scale))
            })
            .collect();
        let mut cues = Vec::new();
        for s in &states {
            let cue = engine.transition(&bundle.deck, &theme, &data, s).unwrap();
            if cue.duration_ms() > 0.0 {
                cues.push((s.clone(), cue));
            }
        }
        let slowest = |times: &[f64], among: &mut dyn Iterator<Item = usize>| {
            among.max_by(|&a, &b| times[a].total_cmp(&times[b])).map(|i| states[i].clone())
        };
        let slowest_layout = slowest(&layouts, &mut (0..states.len())).expect("a deck has states");
        let slowest_paint = slowest(&paints, &mut (0..states.len())).expect("a deck has states");
        // The longest cue, and of equals the one that draws the most, then the earliest: picked
        // from the document, not a clock, so every run plays the same frames.
        let ops = |state: &str| lists[states.iter().position(|s| s == state).expect("a cue's state")].ops.len();
        let longest_cue = (0..cues.len()).rev().max_by(|&a, &b| {
            let (a, b) = (&cues[a], &cues[b]);
            a.1.duration_ms().total_cmp(&b.1.duration_ms()).then(ops(&a.0).cmp(&ops(&b.0)))
        });
        eprintln!(
            "{name}: {} states, {} cues; slowest to lay out `{slowest_layout}`, to paint `{slowest_paint}`; longest cue: {}",
            states.len(),
            cues.len(),
            longest_cue.map_or("none".into(), |i| format!("`{}`, {} ms", cues[i].0, cues[i].1.duration_ms())),
        );
        Deck {
            name,
            path,
            bundle,
            theme,
            data,
            states,
            engine: RefCell::new(engine),
            assets,
            scale,
            lists,
            cues,
            slowest_layout,
            slowest_paint,
            longest_cue,
        }
    }

    fn req<'a>(&'a self, state: &'a str) -> FrameRequest<'a> {
        FrameRequest {
            deck: &self.bundle.deck,
            theme: &self.theme,
            data: &self.data,
            state,
            t_ms: f64::INFINITY,
            format: None,
        }
    }

    fn rest(&self, state: &str) -> &DisplayList {
        &self.lists[self.states.iter().position(|s| s == state).unwrap()]
    }
}

/// Seconds `f` takes.
fn seconds<T>(f: impl FnOnce() -> T) -> f64 {
    let t = Instant::now();
    black_box(f());
    t.elapsed().as_secs_f64()
}

fn main() {
    let mut c = Criterion::default()
        .sample_size(20)
        .warm_up_time(Duration::from_secs(1))
        .measurement_time(Duration::from_secs(3))
        .configure_from_args();
    let out = std::env::temp_dir().join(format!("scaena-bench-{}", std::process::id()));
    std::fs::create_dir_all(&out).unwrap();
    let decks: Vec<Deck> = DECKS.iter().map(|&(name, rel)| Deck::open(name, rel)).collect();
    cold(&mut c, &decks);
    layout(&mut c, &decks);
    paint(&mut c, &decks);
    lint(&mut c, &decks);
    processes(&mut c, &decks, &out);
    video(&mut c, &decks, &out);
    probe(&mut c);
    c.final_summary();
    let _ = std::fs::remove_dir_all(&out);
}

/// A group whose iterations can take tens of milliseconds or more: ten samples of as many
/// iterations each as fit.
fn slow(g: &mut BenchmarkGroup<'_, WallTime>) {
    g.sample_size(10).sampling_mode(SamplingMode::Flat);
}

/// What a cold start pays before any layout: reading the bundle, and its fonts.
fn cold(c: &mut Criterion, decks: &[Deck]) {
    let mut g = c.benchmark_group("load");
    for d in decks {
        g.bench_function(d.name, |b| {
            b.iter(|| {
                let bundle = Bundle::open(&d.path).unwrap();
                let theme = Theme::from_json(bundle.theme_json.as_deref().unwrap()).unwrap();
                (bundle.read_fonts().unwrap(), bundle.read_images().unwrap(), bundle.read_data().unwrap(), theme)
            })
        });
    }
    g.finish();
    let mut g = c.benchmark_group("fonts");
    for d in decks {
        let fonts = d.bundle.read_fonts().unwrap();
        g.bench_function(d.name, |b| {
            b.iter(|| {
                let mut registered = BundleFonts::new();
                for (id, bytes) in &fonts {
                    registered.register(id, bytes.clone()).unwrap();
                }
                registered.check_theme(&d.theme).unwrap();
                registered
            })
        });
    }
    g.finish();
}

/// Layout, cues, and sampling: what an edit costs, and what a frame between states does.
fn layout(c: &mut Criterion, decks: &[Deck]) {
    let mut g = c.benchmark_group("layout_fresh");
    slow(&mut g);
    for d in decks {
        g.throughput(Throughput::Elements(d.states.len() as u64));
        g.bench_function(d.name, |b| {
            b.iter_batched(
                || scaena_ops::lint::engine_with(&d.bundle, &d.theme, None).unwrap(),
                |mut engine| {
                    for s in &d.states {
                        black_box(engine.frame(&d.req(s)).unwrap());
                    }
                },
                BatchSize::PerIteration,
            )
        });
    }
    g.finish();
    let mut g = c.benchmark_group("layout");
    slow(&mut g);
    for d in decks {
        g.throughput(Throughput::Elements(d.states.len() as u64));
        g.bench_function(d.name, |b| {
            let mut engine = d.engine.borrow_mut();
            b.iter(|| {
                for s in &d.states {
                    black_box(engine.frame(&d.req(s)).unwrap());
                }
            })
        });
    }
    g.finish();
    let mut g = c.benchmark_group("layout_one");
    for d in decks {
        g.bench_function(d.name, |b| {
            let mut engine = d.engine.borrow_mut();
            b.iter(|| engine.frame(&d.req(&d.slowest_layout)).unwrap())
        });
    }
    g.finish();
    let mut g = c.benchmark_group("transition");
    slow(&mut g);
    for d in decks {
        g.throughput(Throughput::Elements(d.cues.len() as u64));
        g.bench_function(d.name, |b| {
            let mut engine = d.engine.borrow_mut();
            b.iter(|| {
                for (s, _) in &d.cues {
                    black_box(engine.transition(&d.bundle.deck, &d.theme, &d.data, s).unwrap());
                }
            })
        });
    }
    g.finish();
    let mut g = c.benchmark_group("sample");
    for d in decks {
        g.throughput(Throughput::Elements(d.cues.len() as u64 * u64::from(FRAMES)));
        g.bench_function(d.name, |b| {
            b.iter(|| {
                for (_, cue) in &d.cues {
                    let span = cue.duration_ms();
                    for i in 0..FRAMES {
                        black_box(cue.frame(span * f64::from(i) / f64::from(FRAMES)));
                    }
                }
            })
        });
    }
    g.finish();
}

/// Painting 1080p frames at rest: on the CPU, and on the GPU where there is one.
fn paint(c: &mut Criterion, decks: &[Deck]) {
    let mut g = c.benchmark_group("cpu_paint");
    slow(&mut g);
    for d in decks {
        g.throughput(Throughput::Elements(d.lists.len() as u64));
        g.bench_function(d.name, |b| {
            let mut cpu = CpuPainter::default();
            b.iter(|| {
                for dl in &d.lists {
                    black_box(cpu.paint(dl, &d.assets, d.scale).unwrap());
                }
            })
        });
    }
    g.finish();
    let mut g = c.benchmark_group("cpu_paint_one");
    slow(&mut g);
    for d in decks {
        let dl = d.rest(&d.slowest_paint);
        g.bench_function(d.name, |b| {
            let mut cpu = CpuPainter::default();
            b.iter(|| cpu.paint(dl, &d.assets, d.scale).unwrap())
        });
    }
    g.finish();
    gpu(c, decks);
}

#[cfg(feature = "gpu")]
fn gpu(c: &mut Criterion, decks: &[Deck]) {
    let mut gpu = match scaena_paint::gpu::GpuPainter::new() {
        Ok(gpu) => gpu,
        Err(e) => return eprintln!("gpu_paint: not run: {e}"),
    };
    eprintln!("gpu_paint: {} ({:?})", gpu.adapter().name, gpu.adapter().backend);
    let mut g = c.benchmark_group("gpu_paint");
    slow(&mut g);
    for d in decks {
        g.throughput(Throughput::Elements(d.lists.len() as u64));
        g.bench_function(d.name, |b| {
            b.iter(|| {
                for dl in &d.lists {
                    black_box(gpu.paint(dl, &d.assets, d.scale).unwrap());
                }
            })
        });
    }
    g.finish();
}

#[cfg(not(feature = "gpu"))]
fn gpu(_: &mut Criterion, _: &[Deck]) {
    eprintln!("gpu_paint: not run: build with `--features gpu`");
}

/// Lint: the document's rules alone, then the rules that lay every state out.
fn lint(c: &mut Criterion, decks: &[Deck]) {
    let mut g = c.benchmark_group("lint_document");
    for d in decks {
        g.bench_function(d.name, |b| b.iter(|| scaena_core::lint::lint_document(&d.bundle.deck, Some(&*d.theme))));
    }
    g.finish();
    let mut g = c.benchmark_group("lint_layout");
    slow(&mut g);
    for d in decks {
        g.bench_function(d.name, |b| {
            let mut engine = d.engine.borrow_mut();
            b.iter(|| {
                let mut backdrop = scaena_paint::Backdrop { painter: CpuPainter::default(), assets: &d.assets };
                scaena_engine::lint::lint(&mut engine, &d.bundle.deck, &d.theme, &d.data, Some(&mut backdrop)).unwrap()
            })
        });
    }
    g.finish();
}

/// What a person or an agent waits for, cold: a render, and a tool call. Whole processes.
fn processes(c: &mut Criterion, decks: &[Deck], out: &Path) {
    let scaena = env!("CARGO_BIN_EXE_scaena");
    let mut g = c.benchmark_group("render_cold");
    slow(&mut g);
    for d in decks {
        let png = out.join(format!("{}.png", d.name));
        g.bench_function(d.name, |b| {
            b.iter(|| {
                let done = std::process::Command::new(scaena)
                    .arg("render")
                    .arg(&d.path)
                    .args(["--state", &d.slowest_paint, "--out"])
                    .arg(&png)
                    .output()
                    .unwrap();
                assert!(done.status.success(), "{}", String::from_utf8_lossy(&done.stderr));
            })
        });
    }
    g.finish();
    let rt = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let mut g = c.benchmark_group("mcp_render");
    slow(&mut g);
    for d in decks {
        let args = serde_json::json!({ "bundle": d.path, "state": d.slowest_paint });
        let args = args.as_object().unwrap();
        g.bench_function(d.name, |b| b.iter(|| rt.block_on(mcp::render(scaena, args.clone()))));
    }
    g.finish();
}

/// Video export's frame loop (SPEC §10) over the deck's longest cue: each frame sampled,
/// painted on every core, and handed to an ffmpeg that discards it, so it times Scaena's
/// side, not an encoder. The cue is laid out beforehand, as the exporter lays out each cue
/// once; holds are left out, since a still hold paints one frame and repeats it. With the
/// `gpu` feature and an adapter, `video_gpu` does the same with vello painting the frames
/// on the GPU (PLAN 2.22).
#[cfg(unix)]
fn video(c: &mut Criterion, decks: &[Deck], out: &Path) {
    use scaena_export::video::Painter;
    reel(c, decks, out, "video", Painter::Cpu);
    #[cfg(feature = "gpu")]
    match scaena_paint::gpu::GpuPainter::new() {
        Ok(_) => reel(c, decks, out, "video_gpu", Painter::Gpu),
        Err(e) => eprintln!("video_gpu: not run: {e}"),
    }
    #[cfg(not(feature = "gpu"))]
    eprintln!("video_gpu: not run: build with `--features gpu`");
}

#[cfg(unix)]
fn reel(c: &mut Criterion, decks: &[Deck], out: &Path, stage: &str, painter: scaena_export::video::Painter) {
    use scaena_export::video::{VideoSettings, encode};
    let ffmpeg = sink(out);
    let mut g = c.benchmark_group(stage);
    slow(&mut g);
    for d in decks {
        let Some(i) = d.longest_cue else { continue };
        let cue = &d.cues[i].1;
        let frames = (cue.duration_ms() * f64::from(FRAMES) / 1000.0).ceil() as u64;
        let canvas = [d.bundle.deck.canvas.width as f32, d.bundle.deck.canvas.height as f32];
        let settings =
            VideoSettings { painter, scale: d.scale, fps: FRAMES, ffmpeg: ffmpeg.clone(), ..VideoSettings::default() };
        let mp4 = out.join(format!("{}.mp4", d.name));
        g.throughput(Throughput::Elements(frames));
        g.bench_function(d.name, |b| {
            b.iter(|| {
                let mut k = 0;
                let next = || {
                    let ms = (k < frames).then(|| k as f64 * 1000.0 / f64::from(FRAMES))?;
                    k += 1;
                    Some(Ok(cue.frame(ms)))
                };
                encode(&mp4, canvas, &settings, &d.assets, next).unwrap()
            })
        });
    }
    g.finish();
}

#[cfg(not(unix))]
fn video(_: &mut Criterion, _: &[Deck], _: &Path) {
    eprintln!("video: not run: its stand-in ffmpeg is a shell script");
}

/// An ffmpeg that reads the frames it is given and writes an empty file where it was told
/// to write the video.
#[cfg(unix)]
fn sink(dir: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt as _;
    let ffmpeg = dir.join("ffmpeg");
    std::fs::write(&ffmpeg, "#!/bin/sh\nfor out; do :; done\ncat > /dev/null && : > \"$out\"\n").unwrap();
    std::fs::set_permissions(&ffmpeg, std::fs::Permissions::from_mode(0o755)).unwrap();
    ffmpeg
}

/// How fast this machine runs code that is not Scaena's: a fixed list sorted. The gate
/// shows it beside the stages, to tell a slow runner from a slow change.
fn probe(c: &mut Criterion) {
    let mut x = 0x9E37_79B9_7F4A_7C15_u64;
    let list: Vec<u64> = (0..1 << 17)
        .map(|_| {
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            x.wrapping_mul(0x2545_F491_4F6C_DD1D)
        })
        .collect();
    let mut g = c.benchmark_group("probe");
    g.bench_function("cpu", |b| b.iter_batched(|| list.clone(), |mut v| v.sort_unstable(), BatchSize::LargeInput));
    g.finish();
}

mod mcp {
    use rmcp::ServiceExt;
    use rmcp::model::{CallToolRequestParams, ClientConfig, JsonObject};
    use std::process::Stdio;

    /// `scaena mcp` started, `deck_render` called on it, and the server stopped.
    pub async fn render(scaena: &str, args: JsonObject) {
        let mut child = tokio::process::Command::new(scaena)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .unwrap();
        let io = (child.stdout.take().unwrap(), child.stdin.take().unwrap());
        let client = ClientConfig::default().serve(io).await.unwrap();
        let rendered = client.call_tool(CallToolRequestParams::new("deck_render").with_arguments(args)).await.unwrap();
        assert_ne!(rendered.is_error, Some(true), "{:?}", rendered.content);
        client.cancel().await.unwrap();
        assert!(child.wait().await.unwrap().success());
    }
}
