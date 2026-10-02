//! `scaena` — the command-line tool (SPEC §7.1). The first client (ADR-0003).
//!
//! Exit codes: 0 ok · 1 lint errors · 2 invalid input · 3 internal/not implemented.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use scaena_core::{Finding, Severity};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, EngineError, FrameRequest};
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{FontStore, PaintError, Painter};
use scaena_store::{Bundle, SaveOptions};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

#[derive(Parser)]
#[command(
    name = "scaena",
    version,
    about = "A presentation engine: states over a scene graph, rendered deterministically."
)]
struct Cli {
    /// Machine-readable output.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Schema + semantic validation of a bundle or deck.json.
    Validate { bundle: PathBuf },
    /// Run lint rules; exit 1 on errors.
    Lint {
        bundle: PathBuf,
        #[arg(long)]
        state: Option<String>,
        #[arg(long, value_enum, default_value_t = SeverityArg::Info)]
        severity: SeverityArg,
        /// Apply safe fixes (PLAN 1.15).
        #[arg(long)]
        fix: bool,
    },
    /// Resolved snapshot for a state (tracking applied), or all states.
    Inspect {
        bundle: PathBuf,
        #[arg(long)]
        state: Option<String>,
    },
    /// What changes between two states (resolved).
    Diff {
        bundle: PathBuf,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    /// DSL → JSON: compile a `.scn` file to `deck.json`, checked as `validate` checks it,
    /// with each problem shown where the source says it.
    Compile {
        input: PathBuf,
        /// The `deck.json` to write; its directory is the bundle it is checked in. Default:
        /// stdout, checked in the source's directory.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// JSON → DSL: a deck (a bundle, a `.scaena` zip, or a `deck.json`) as canonical `.scn`.
    Decompile {
        input: PathBuf,
        /// The `.scn` file to write. Default: stdout.
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Render one state to a PNG, and optionally its display list.
    Render(RenderArgs),
    /// Write the bundle back as SPEC §3.1 lays it out: canonical JSON, fonts subset to
    /// what the deck draws, files named by their content, and a manifest.
    Save {
        bundle: PathBuf,
        /// Where to: a directory, or a zip when it ends in `.scaena`. Default: in place.
        #[arg(long)]
        to: Option<PathBuf>,
        /// Keep fonts whole instead of subsetting them.
        #[arg(long)]
        keep_fonts: bool,
    },
    /// Export a projection: pdf|png|svg|mp4|webm|html|spine.
    Export {
        bundle: PathBuf,
        #[arg(long)]
        format: String,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long, default_value_t = 60)]
        fps: u32,
    },
    /// Apply a JSON Patch / semantic ops file (PLAN 1.16).
    Patch {
        bundle: PathBuf,
        #[arg(long)]
        ops: PathBuf,
        #[arg(long)]
        dry_run: bool,
    },
    /// Re-theme (PLAN 1.6).
    Theme {
        bundle: PathBuf,
        #[arg(long)]
        apply: PathBuf,
    },
    /// Dev server with live preview (PLAN 2.x).
    Serve {
        bundle: PathBuf,
        #[arg(long, default_value_t = 4848)]
        port: u16,
    },
    /// MCP server on stdio (PLAN 1.17).
    Mcp,
}

#[derive(Args)]
struct RenderArgs {
    bundle: PathBuf,
    #[arg(long)]
    state: String,
    /// Milliseconds into the transition into the state. Omitted: the state at rest.
    #[arg(long)]
    t: Option<f64>,
    /// Output pixels, `WxH`, in the canvas's aspect ratio. Default: the canvas size.
    #[arg(long)]
    size: Option<String>,
    /// PNG to write. Default: `<state>.png`.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Also write the display list as JSON (one op and one glyph per line).
    #[arg(long)]
    display_list: Option<PathBuf>,
    #[arg(long, value_enum, default_value_t = PainterArg::Cpu)]
    painter: PainterArg,
}

#[derive(Clone, Copy, PartialEq, ValueEnum)]
enum PainterArg {
    /// `vello_cpu`: headless, deterministic per SIMD level (ADR-0004).
    Cpu,
    /// `vello` on `wgpu`, read back from the GPU (needs a CLI built with `--features gpu`).
    Gpu,
}

#[derive(Clone, Copy, ValueEnum)]
enum SeverityArg {
    Error,
    Warning,
    Info,
}

impl From<SeverityArg> for Severity {
    fn from(s: SeverityArg) -> Self {
        match s {
            SeverityArg::Error => Severity::Error,
            SeverityArg::Warning => Severity::Warning,
            SeverityArg::Info => Severity::Info,
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.cmd {
        Cmd::Validate { bundle } => {
            let (deck, files) =
                scaena_store::open_unparsed(&bundle).with_context(|| format!("opening {}", bundle.display()))?;
            let findings = scaena_core::validate::validate_bundle(&deck, &files).context("deck.json is not JSON")?;
            report(&findings, cli.json);
            Ok(if findings.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
        }
        Cmd::Save { bundle, to, keep_fonts } => {
            let b = open(&bundle)?;
            let to = to.unwrap_or(bundle);
            let opts = SaveOptions { subset_fonts: !keep_fonts, now: now_rfc3339() };
            let saved = b.save(&to, &opts).with_context(|| format!("saving to {}", to.display()))?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&saved)?);
            } else {
                println!("saved {}: {} files", to.display(), saved.manifest.files.len() + 1);
                for (path, before, after) in &saved.subset {
                    println!("  subset {path}: {} KB → {} KB", before / 1024, after / 1024);
                }
                for (old, new) in &saved.renamed {
                    println!("  {old} → {new}");
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Lint { bundle, state, severity, fix } => {
            if fix {
                return not_yet("lint --fix", "1.15");
            }
            let b = open(&bundle)?;
            let min: Severity = severity.into();
            let findings: Vec<Finding> = scaena_core::lint::lint_document(&b.deck)
                .into_iter()
                .filter(|f| f.severity >= min)
                .filter(|f| state.as_ref().is_none_or(|s| f.state.as_deref() == Some(s)))
                .collect();
            report(&findings, cli.json);
            if !cli.json {
                eprintln!("(layout-level rules — overflow, contrast, collisions — arrive with the engine: PLAN 1.15)");
            }
            let has_errors = findings.iter().any(|f| f.severity == Severity::Error);
            Ok(if has_errors { ExitCode::from(1) } else { ExitCode::SUCCESS })
        }
        Cmd::Inspect { bundle, state } => {
            let b = open(&bundle)?;
            let snaps = scaena_core::resolve_states(&b.deck).context("tracking")?;
            let selected: Vec<_> = snaps.iter().filter(|s| state.as_ref().is_none_or(|id| &s.state_id == id)).collect();
            if selected.is_empty() {
                anyhow::bail!("unknown state `{}`", state.unwrap_or_default());
            }
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&selected)?);
            } else {
                for s in selected {
                    println!(
                        "state {}  slide={}  layout={}",
                        s.state_id,
                        s.slide_id,
                        s.layout.as_deref().unwrap_or("-")
                    );
                    for (id, props) in &s.nodes {
                        let marker = if s.entered.contains(id) { "+" } else { " " };
                        println!("  {marker} {id:<12} {}", summarize(props));
                    }
                    if !s.exited.is_empty() {
                        println!("  - exited: {}", s.exited.join(", "));
                    }
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Diff { bundle, from, to } => {
            let b = open(&bundle)?;
            let snaps = scaena_core::resolve_states(&b.deck).context("tracking")?;
            let a = snaps.iter().find(|s| s.state_id == from).with_context(|| format!("unknown state `{from}`"))?;
            let z = snaps.iter().find(|s| s.state_id == to).with_context(|| format!("unknown state `{to}`"))?;
            let mut changes = serde_json::Map::new();
            for (id, props) in &z.nodes {
                match a.nodes.get(id) {
                    None => {
                        changes.insert(id.clone(), serde_json::json!({"enter": props}));
                    }
                    Some(prev) if prev != props => {
                        let delta: serde_json::Map<String, serde_json::Value> = props
                            .iter()
                            .filter(|(k, v)| prev.get(*k) != Some(*v))
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect();
                        changes.insert(id.clone(), serde_json::json!({"change": delta}));
                    }
                    _ => {}
                }
            }
            for id in a.nodes.keys().filter(|id| !z.nodes.contains_key(*id)) {
                changes.insert(id.clone(), serde_json::json!({"exit": true}));
            }
            println!("{}", serde_json::to_string_pretty(&changes)?);
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Export { bundle, format, out, .. } => {
            let fmt: scaena_export::Format = format.parse().map_err(anyhow::Error::msg)?;
            match fmt {
                scaena_export::Format::Spine => {
                    let b = open(&bundle)?;
                    let v = scaena_export::spine_json(&b.deck);
                    let s = serde_json::to_string_pretty(&v)?;
                    match out {
                        Some(p) => std::fs::write(p, s)?,
                        None => println!("{s}"),
                    }
                    Ok(ExitCode::SUCCESS)
                }
                _ => not_yet(&format!("export --format {format}"), "1.20–1.21 / 2.5"),
            }
        }
        Cmd::Compile { input, out } => compile(&input, out.as_deref(), cli.json),
        Cmd::Decompile { input, out } => {
            let scn = scaena_core::dsl::decompile(&open(&input)?.deck);
            match out {
                Some(p) => std::fs::write(&p, scn).with_context(|| format!("writing {}", p.display()))?,
                None => print!("{scn}"),
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Render(args) => render(args, cli.json),
        Cmd::Patch { .. } => not_yet("patch", "1.16"),
        Cmd::Theme { .. } => not_yet("theme --apply", "1.6"),
        Cmd::Serve { .. } => not_yet("serve", "2.x"),
        Cmd::Mcp => not_yet("mcp", "1.17"),
    }
}

/// `scaena compile` (PLAN 1.5): `.scn` → `deck.json`. A source that does not parse exits 2;
/// a deck that does not validate exits 1, each finding shown at the source it came from.
/// Either way nothing is written: `deck.json` is the truth, and only a valid deck replaces it.
fn compile(input: &Path, out: Option<&Path>, json: bool) -> Result<ExitCode> {
    let source = std::fs::read_to_string(input).with_context(|| format!("reading {}", input.display()))?;
    let name = input.display().to_string();
    let (doc, map) = match scaena_core::dsl::compile_json(&source) {
        Ok(compiled) => compiled,
        Err(e) => {
            if json {
                let mut v =
                    serde_json::json!({ "severity": "error", "message": e.message, "line": e.line, "col": e.col });
                if let Some(pointer) = &e.pointer {
                    v["path"] = serde_json::json!(pointer);
                }
                println!("{}", serde_json::to_string_pretty(&[v])?);
            } else {
                let label = e.pointer.clone();
                eprint!("{}", diagnostic(&name, &source, None, &e.message, Some((e.offset, e.len)), label, None));
            }
            return Ok(ExitCode::from(2));
        }
    };
    let deck_json = serde_json::to_string(&doc)?;
    // The bundle the deck is checked in: the one it is written to, or the source's.
    let root = out.unwrap_or(input).parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let findings = scaena_core::validate::validate_bundle(&deck_json, &scaena_store::Files::Dir(root.to_path_buf()))?;
    if !findings.is_empty() {
        // A finding about the deck is about the source that wrote that part of it; one
        // about another file (the theme) is about that file.
        let span = |f: &Finding| match (&f.file, &f.path) {
            (None, Some(path)) => map.locate(path),
            _ => None,
        };
        if json {
            let located: Vec<serde_json::Value> = findings
                .iter()
                .map(|f| {
                    let mut v = serde_json::to_value(f).expect("a finding is JSON");
                    if let Some((offset, _)) = span(f) {
                        let (line, col) = line_col(&source, offset);
                        v["line"] = serde_json::json!(line);
                        v["col"] = serde_json::json!(col);
                    }
                    v
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&located)?);
        } else {
            for f in &findings {
                let message = match &f.file {
                    Some(file) => format!("{file} {}: {}", f.path.as_deref().unwrap_or(""), f.message),
                    None => f.message.clone(),
                };
                let (span, label) = match span(f) {
                    Some(span) => (Some(span), f.path.clone()),
                    None => (None, None),
                };
                eprint!("{}", diagnostic(&name, &source, Some(&f.code), &message, span, label, f.hint.as_deref()));
            }
        }
        return Ok(ExitCode::from(1));
    }
    let deck = scaena_core::document::Deck::from_json(&deck_json).context("the compiled deck")?;
    let canonical = deck.to_json()? + "\n";
    match out {
        Some(p) => std::fs::write(p, canonical).with_context(|| format!("writing {}", p.display()))?,
        None => print!("{canonical}"),
    }
    Ok(ExitCode::SUCCESS)
}

/// One problem in `source`, drawn by miette: the message, and the source it is about
/// underlined with `label`. Colors only on a terminal, and never under `NO_COLOR`.
fn diagnostic(
    name: &str,
    source: &str,
    code: Option<&str>,
    message: &str,
    span: Option<(usize, usize)>,
    label: Option<String>,
    hint: Option<&str>,
) -> String {
    use miette::{GraphicalReportHandler, GraphicalTheme, LabeledSpan, MietteDiagnostic, NamedSource};
    use std::io::IsTerminal;
    let mut d = MietteDiagnostic::new(message);
    if let Some(code) = code {
        d = d.with_code(code);
    }
    if let Some((offset, len)) = span {
        d = d.with_label(LabeledSpan::new(label, offset, len));
    }
    if let Some(hint) = hint {
        d = d.with_help(hint);
    }
    let report = miette::Report::new(d).with_source_code(NamedSource::new(name, source.to_string()));
    let color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
    let theme = if color { GraphicalTheme::unicode() } else { GraphicalTheme::unicode_nocolor() };
    let mut out = String::new();
    GraphicalReportHandler::new_themed(theme)
        .with_width(100)
        .with_links(false)
        .render_report(&mut out, report.as_ref())
        .expect("drawing to a string");
    out
}

/// 1-based line and column (in characters) of a byte offset into `source`.
fn line_col(source: &str, offset: usize) -> (usize, usize) {
    let before = &source[..offset.min(source.len())];
    let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (before.matches('\n').count() + 1, col)
}

/// `scaena render` (PLAN 0.6, 0.7): bundle fonts → `Engine::frame` → painter → PNG.
/// Timings are wall clock in this client; the render path itself never reads a clock.
fn render(args: RenderArgs, json: bool) -> Result<ExitCode> {
    let RenderArgs { bundle, state, t, size, out, display_list, painter } = args;
    let state = state.as_str();
    if let Some(t) = t.filter(|t| !t.is_finite() || *t < 0.0) {
        anyhow::bail!("--t {t}: expected a finite, non-negative number of milliseconds");
    }
    let start = Instant::now();
    let mut lap = {
        let mut last = start;
        move || {
            let now = Instant::now();
            let ms = (now - last).as_secs_f64() * 1e3;
            last = now;
            ms
        }
    };
    let b = open(&bundle)?;
    let theme = Theme::from_json(b.theme_json.as_deref().context("the deck names no theme")?)?;
    let files = b.read_fonts()?;
    let mut data = DataFiles::new();
    for (path, bytes) in b.read_data()? {
        data.insert(path, bytes);
    }
    let load = lap();

    let mut fonts = BundleFonts::new();
    let mut store = FontStore::new();
    for (id, bytes) in files {
        store.insert(&id, bytes.clone());
        fonts.register(&id, bytes)?;
    }
    fonts.check_theme(&theme)?;
    let register = lap();

    let req = FrameRequest { deck: &b.deck, theme: &theme, data: &data, state, t_ms: t.unwrap_or(f64::INFINITY) };
    let frame = match Engine::new(fonts).frame(&req) {
        Err(e @ EngineError::NotImplemented(_)) => return unimplemented(e),
        frame => frame?,
    };
    let dl = frame.display_list;
    let layout = lap();
    if let Some(path) = &display_list {
        std::fs::write(path, dl.to_golden_json()?).with_context(|| format!("writing {}", path.display()))?;
    }

    let scale = match size.as_deref() {
        Some(size) => scale_for(size, dl.viewport)?,
        None => 1.0,
    };
    // Built after the frame, so a state the engine cannot draw costs no GPU start-up.
    let (mut painter, adapter): (Box<dyn Painter>, Option<String>) = match painter {
        PainterArg::Cpu => (Box::new(CpuPainter::default()), None),
        #[cfg(feature = "gpu")]
        PainterArg::Gpu => {
            let gpu = scaena_paint::gpu::GpuPainter::new()?;
            let info = gpu.adapter();
            let adapter = format!("{} ({:?}, {:?})", info.name, info.backend, info.device_type);
            (Box::new(gpu), Some(adapter))
        }
        #[cfg(not(feature = "gpu"))]
        PainterArg::Gpu => {
            return unimplemented("`--painter gpu` needs the CLI built with `--features gpu` (PLAN 0.7)");
        }
    };
    let init = lap();
    let raster = match painter.paint(&dl, &store, scale) {
        Err(e @ PaintError::NotImplemented(_)) => return unimplemented(e),
        raster => raster?,
    };
    let paint = lap();
    let out = out.unwrap_or_else(|| PathBuf::from(format!("{state}.png")));
    std::fs::write(&out, raster.to_png_fast()?).with_context(|| format!("writing {}", out.display()))?;
    let encode = lap();
    let total = (Instant::now() - start).as_secs_f64() * 1e3;

    if json {
        let us = |ms: f64| (ms * 1e3).round() / 1e3;
        let summary = serde_json::json!({
            "state": state,
            "t_ms": t,
            "painter": painter.name(),
            "adapter": adapter,
            "size": [raster.width, raster.height],
            "out": out,
            "display_list": display_list,
            "ms": {
                "load": us(load), "fonts": us(register), "frame": us(layout), "init": us(init),
                "paint": us(paint), "png": us(encode), "total": us(total),
            },
        });
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        println!(
            "{} ({}×{}, {}) in {total:.0} ms: load {load:.1} · fonts {register:.1} · frame {layout:.1} · init {init:.1} · paint {paint:.1} · png {encode:.1}",
            out.display(),
            raster.width,
            raster.height,
            adapter.as_deref().unwrap_or(painter.name()),
        );
    }
    Ok(ExitCode::SUCCESS)
}

/// `WxH` → output pixels per canvas unit. The size must have the canvas's aspect ratio,
/// to the nearest pixel: painters scale uniformly and never stretch.
fn scale_for(size: &str, canvas: [f32; 2]) -> Result<f32> {
    let parsed = size.split_once('x').and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)));
    let (w, h) = parsed
        .filter(|&(w, h)| w > 0 && h > 0)
        .with_context(|| format!("--size `{size}`: expected WxH, like 1920x1080"))?;
    let scale = w as f32 / canvas[0];
    if (canvas[1] * scale).round() != h as f32 {
        anyhow::bail!("--size {w}x{h} does not have the canvas's aspect ratio ({}x{} cu)", canvas[0], canvas[1]);
    }
    Ok(scale)
}

/// Exit 3 for a path that a later PLAN task implements; the error names the task.
fn unimplemented(e: impl std::fmt::Display) -> Result<ExitCode> {
    eprintln!("error: {e}");
    Ok(ExitCode::from(3))
}

fn open(path: &Path) -> Result<Bundle> {
    Bundle::open(path).with_context(|| format!("opening {}", path.display()))
}

/// Now, in RFC 3339 UTC: `SOURCE_DATE_EPOCH` when set (reproducible saves), else the clock.
fn now_rfc3339() -> String {
    let secs = std::env::var("SOURCE_DATE_EPOCH").ok().and_then(|s| s.parse::<u64>().ok()).unwrap_or_else(|| {
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs())
    });
    rfc3339(secs)
}

/// Seconds since 1970-01-01T00:00:00Z, in RFC 3339 UTC.
fn rfc3339(secs: u64) -> String {
    let (days, rest) = (secs / 86_400, secs % 86_400);
    // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
    let z = days as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z", rest / 3_600, rest % 3_600 / 60, rest % 60)
}

fn not_yet(what: &str, plan: &str) -> Result<ExitCode> {
    eprintln!("`{what}` is not implemented yet — see docs/PLAN.md task {plan}");
    Ok(ExitCode::from(3))
}

fn report(findings: &[Finding], json: bool) {
    if json {
        println!("{}", serde_json::to_string_pretty(findings).unwrap());
        return;
    }
    if findings.is_empty() {
        println!("ok: no findings");
        return;
    }
    for f in findings {
        let sev = match f.severity {
            Severity::Error => "error",
            Severity::Warning => "warn ",
            Severity::Info => "info ",
        };
        let loc = match &f.file {
            Some(file) => format!("{file} {}", f.path.as_deref().unwrap_or("")),
            None => f.path.clone().unwrap_or_default(),
        };
        println!("{sev} {} {loc}: {}", f.code, f.message);
        if let Some(h) = &f.hint {
            println!("        hint: {h}");
        }
    }
}

fn summarize(props: &scaena_core::document::Props) -> String {
    let mut parts: Vec<String> = Vec::new();
    for key in ["role", "text", "kind", "data", "at"] {
        if let Some(v) = props.get(key) {
            let s = match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            parts.push(format!("{key}={}", truncate(&s, 32)));
        }
    }
    parts.join("  ")
}

/// The first `max` characters of `s`, with an ellipsis when cut. Counts chars, not
/// bytes: a byte offset can land inside a multi-byte character and panic (the
/// torture deck's accented-Latin state did exactly that).
fn truncate(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::truncate;

    #[test]
    fn truncate_counts_chars_not_bytes() {
        assert_eq!(truncate("Ærøskøbing · Łódź", 5), "Ærøsk…");
        assert_eq!(truncate("שלום Scaena", 4), "שלום…");
        assert_eq!(truncate("exactly", 7), "exactly");
        assert_eq!(truncate("", 3), "");
    }

    #[test]
    fn rfc3339_counts_days_like_the_calendar() {
        assert_eq!(super::rfc3339(0), "1970-01-01T00:00:00Z");
        assert_eq!(super::rfc3339(951_782_400), "2000-02-29T00:00:00Z");
        assert_eq!(super::rfc3339(1_790_000_000), "2026-09-21T14:13:20Z");
        assert_eq!(super::rfc3339(4_102_444_799), "2099-12-31T23:59:59Z");
        assert_eq!(super::rfc3339(4_102_444_800), "2100-01-01T00:00:00Z");
    }
}
