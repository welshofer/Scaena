//! `scaena` — the command-line tool (SPEC §7.1). The first client (ADR-0003).
//!
//! Exit codes: 0 ok · 1 lint errors · 2 invalid input · 3 internal/not implemented.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use scaena_core::lint::Delta;
use scaena_core::validate::BundleFiles;
use scaena_core::{Finding, Severity};
use scaena_engine::data::DataFiles;
use scaena_engine::fonts::BundleFonts;
use scaena_engine::images::BundleImages;
use scaena_engine::theme::Theme;
use scaena_engine::{Engine, EngineError, FrameRequest};
use scaena_paint::cpu::CpuPainter;
use scaena_paint::{Assets, PaintError, Painter};
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
        /// Through the theme cascade: each node with the deck's overrides merged in, each
        /// text node's look (role, family, size, color), and what its overrides set.
        #[arg(long)]
        resolved: bool,
        /// Each state's cue: where it falls on the deck's timeline, its transition, and
        /// each motion as placed on its clock. Reads the bundle's fonts, as `render` does.
        #[arg(long)]
        timeline: bool,
        /// The rows each chart and table reads, after its `dataTransform`.
        #[arg(long)]
        data: bool,
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
        /// The states to export, comma-separated. Default: every state.
        #[arg(long, value_delimiter = ',')]
        states: Option<Vec<String>>,
        #[arg(long, default_value_t = 60)]
        fps: u32,
    },
    /// Apply a patch: JSON Patch (RFC 6902) and semantic ops, all or none, and say what
    /// changes in what `validate` and `lint` find. A patch that would make the deck invalid
    /// is refused.
    Patch {
        bundle: PathBuf,
        /// The ops: a JSON array (`docs/schema/patch.schema.json`), or `-` for stdin.
        #[arg(long)]
        ops: PathBuf,
        /// Say what would change, and write nothing.
        #[arg(long)]
        dry_run: bool,
    },
    /// Re-theme: point the deck at another theme, copied into the bundle, and say what
    /// changes in what `validate` and `lint` find.
    Theme {
        bundle: PathBuf,
        /// The theme file to apply. A theme outside the bundle is copied to `themes/`.
        #[arg(long)]
        apply: PathBuf,
        /// Say what would change, and write nothing.
        #[arg(long)]
        dry_run: bool,
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
    /// Lay the deck out in one of its `formats` (`9:16`) on that format's canvas.
    /// Omitted: its own canvas.
    #[arg(long)]
    format: Option<String>,
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
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        // A usage error under `--json` is still one JSON value; help and version are not errors.
        Err(e) if e.use_stderr() && std::env::args_os().any(|a| a == "--json") => {
            let text = e.to_string();
            let message = text.lines().next().unwrap_or_default().trim_start_matches("error: ");
            return fail(true, 2, message, None);
        }
        Err(e) => e.exit(),
    };
    let json = cli.json;
    match run(cli) {
        Ok(code) => code,
        Err(e) => {
            let message = format!("{e:#}");
            if e.chain().any(not_built) {
                fail(json, 3, &message, plan_task(&message).as_deref())
            } else {
                fail(json, 2, &message, None)
            }
        }
    }
}

/// A command that stops with exit `code` (2, invalid input; 3, not built yet) and says why,
/// on stderr; under `--json`, also as `{ "error": { "exit", "message", "plan"? } }` on
/// stdout, so stdout always holds one JSON value.
fn fail(json: bool, code: u8, message: &str, plan: Option<&str>) -> ExitCode {
    let mut error = serde_json::json!({ "exit": code, "message": message });
    if let Some(plan) = plan {
        error["plan"] = plan.into();
    }
    stop(json, error)
}

/// [`fail`] with an error that says more (`compile`'s line and column).
fn stop(json: bool, error: serde_json::Value) -> ExitCode {
    eprintln!("error: {}", error["message"].as_str().unwrap_or_default());
    if json {
        let v = serde_json::json!({ "error": error });
        println!("{}", serde_json::to_string_pretty(&v).expect("an error is JSON"));
    }
    ExitCode::from(error["exit"].as_u64().and_then(|c| u8::try_from(c).ok()).unwrap_or(2))
}

/// An error from a path a later PLAN task builds, which exits 3 rather than 2.
fn not_built(e: &(dyn std::error::Error + 'static)) -> bool {
    matches!(e.downcast_ref(), Some(EngineError::NotImplemented(_)))
        || matches!(e.downcast_ref(), Some(PaintError::NotImplemented(_)))
}

/// The PLAN task a message names (`… — PLAN 1.20`), if it names one.
fn plan_task(message: &str) -> Option<String> {
    let (_, rest) = message.split_once("PLAN ")?;
    let task: String = rest.chars().take_while(|c| c.is_ascii_digit() || matches!(c, '.' | 'x')).collect();
    let task = task.trim_end_matches('.');
    (!task.is_empty()).then(|| task.to_string())
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
            let b = open(&bundle)?;
            if fix {
                return lint_fix(&bundle, &b, cli.json);
            }
            let min: Severity = severity.into();
            let (found, laid) = lint_bundle(&b, &b.deck, &b.files, b.theme_json.as_deref())?;
            let findings: Vec<Finding> = found
                .into_iter()
                .filter(|f| f.severity >= min)
                .filter(|f| state.as_ref().is_none_or(|s| f.state.as_deref() == Some(s)))
                .collect();
            report(&findings, cli.json);
            if !laid && !cli.json {
                eprintln!("(the layout rules run once the errors above are fixed)");
            }
            let has_errors = findings.iter().any(|f| f.severity == Severity::Error);
            Ok(if has_errors { ExitCode::from(1) } else { ExitCode::SUCCESS })
        }
        Cmd::Inspect { bundle, state, resolved, timeline, data } => {
            inspect(&open(&bundle)?, state.as_deref(), Views { resolved, timeline, data }, cli.json)
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
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&changes)?);
            } else if changes.is_empty() {
                println!("no changes from `{from}` to `{to}`");
            } else {
                for (id, change) in &changes {
                    match change.as_object().and_then(|c| c.iter().next()) {
                        Some((kind, _)) if kind == "enter" => println!("+ {id}"),
                        Some((kind, _)) if kind == "exit" => println!("- {id}"),
                        Some((_, delta)) => {
                            let keys: Vec<&str> =
                                delta.as_object().into_iter().flatten().map(|(k, _)| k.as_str()).collect();
                            println!("~ {id}: {}", keys.join(", "));
                        }
                        None => {}
                    }
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Export { bundle, format, out, states, .. } => {
            let fmt: scaena_export::Format = format.parse().map_err(anyhow::Error::msg)?;
            match fmt {
                scaena_export::Format::Spine => {
                    if states.is_some() {
                        anyhow::bail!(
                            "--states picks the frames of png, pdf, svg, mp4, webm, and html; spine is the whole spine"
                        );
                    }
                    let b = open(&bundle)?;
                    let v = scaena_export::spine_json(&b.deck);
                    let s = serde_json::to_string_pretty(&v)?;
                    if let Some(p) = &out {
                        std::fs::write(p, &s).with_context(|| format!("writing {}", p.display()))?;
                    }
                    if cli.json {
                        let mut summary = serde_json::json!({ "format": "spine", "out": out });
                        if out.is_none() {
                            summary["spine"] = v;
                        }
                        println!("{}", serde_json::to_string_pretty(&summary)?);
                    } else if out.is_none() {
                        println!("{s}");
                    }
                    Ok(ExitCode::SUCCESS)
                }
                scaena_export::Format::Pdf => Ok(not_yet(cli.json, "export --format pdf", "1.20")),
                scaena_export::Format::Html => Ok(not_yet(cli.json, "export --format html", "2.5")),
                _ => Ok(not_yet(cli.json, &format!("export --format {format}"), "1.21")),
            }
        }
        Cmd::Compile { input, out } => compile(&input, out.as_deref(), cli.json),
        Cmd::Decompile { input, out } => {
            let scn = scaena_core::dsl::decompile(&open(&input)?.deck);
            if let Some(p) = &out {
                std::fs::write(p, &scn).with_context(|| format!("writing {}", p.display()))?;
            }
            if cli.json {
                let mut summary = serde_json::json!({ "out": out });
                if out.is_none() {
                    summary["scn"] = scn.into();
                }
                println!("{}", serde_json::to_string_pretty(&summary)?);
            } else if out.is_none() {
                print!("{scn}");
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Render(args) => render(args, cli.json),
        Cmd::Patch { bundle, ops, dry_run } => patch(&bundle, &ops, dry_run, cli.json),
        Cmd::Theme { bundle, apply, dry_run } => theme_apply(&bundle, &apply, dry_run, cli.json),
        Cmd::Serve { .. } => Ok(not_yet(cli.json, "serve", "2.x")),
        Cmd::Mcp => Ok(not_yet(cli.json, "mcp", "1.17")),
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
                let mut error = serde_json::json!({ "exit": 2, "message": e.message, "line": e.line, "col": e.col });
                if let Some(pointer) = &e.pointer {
                    error["path"] = serde_json::json!(pointer);
                }
                return Ok(stop(json, error));
            }
            let label = e.pointer.clone();
            eprint!("{}", diagnostic(&name, &source, None, &e.message, Some((e.offset, e.len)), label, None));
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
            let summary = serde_json::json!({ "out": null, "findings": located });
            println!("{}", serde_json::to_string_pretty(&summary)?);
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
    if let Some(p) = out {
        std::fs::write(p, &canonical).with_context(|| format!("writing {}", p.display()))?;
    }
    if json {
        let mut summary = serde_json::json!({ "out": out, "findings": [] });
        if out.is_none() {
            summary["deck"] = serde_json::from_str(&canonical)?;
        }
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else if out.is_none() {
        print!("{canonical}");
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

/// `scaena theme --apply` (PLAN 1.6): point the deck at another theme, and report the
/// delta in what `validate` and `lint` find: what the new theme breaks, and what it fixes.
/// A theme change is a pure re-render (SPEC §2.5), so the deck itself is not touched beyond
/// its `theme`. Findings after it, if any are errors, exit 1.
fn theme_apply(bundle: &Path, theme: &Path, dry_run: bool, json: bool) -> Result<ExitCode> {
    use std::collections::BTreeMap;
    let b = open(bundle)?;
    let mut text = std::fs::read_to_string(theme).with_context(|| format!("reading {}", theme.display()))?;
    let mut parsed: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("{} is not JSON", theme.display()))?;
    // A family whose file the bundle does not hold is set in the bundle's font of that
    // family, if it has one: a saved bundle names its fonts by their content.
    let mut mapped = Vec::new();
    for (key, family) in parsed.pointer_mut("/type/families").and_then(|f| f.as_object_mut()).into_iter().flatten() {
        let (Some(file), Some(name)) = (family["file"].as_str(), family["family"].as_str()) else { continue };
        if b.files.exists(file) {
            continue;
        }
        if let Some(font) = b.deck.fonts.iter().find(|f| f.family == name && b.files.exists(&f.file)) {
            mapped.push(format!("family `{key}`: {file} → {}", font.file));
            family["file"] = serde_json::Value::String(font.file.clone());
        }
    }
    if !mapped.is_empty() {
        text = serde_json::to_string_pretty(&parsed)? + "\n";
    }
    // Where it goes: where it already is inside a bundle directory, else `themes/`.
    let inside = match &b.files {
        scaena_store::Files::Dir(root) => match (root.canonicalize(), theme.canonicalize()) {
            (Ok(root), Ok(theme)) => theme.strip_prefix(&root).ok().map(|p| p.to_string_lossy().replace('\\', "/")),
            _ => None,
        },
        scaena_store::Files::Zip(_) => None,
    };
    let name = theme.file_name().and_then(|n| n.to_str()).context("the theme has no file name")?;
    let rel = inside.clone().unwrap_or_else(|| format!("themes/{name}"));
    let was = match &b.deck.theme {
        Some(serde_json::Value::String(path)) => Some(path.clone()),
        Some(_) => Some("(inline)".to_string()),
        None => None,
    };

    let before = lint_bundle(&b, &b.deck, &b.files, b.theme_json.as_deref())?.0;
    let mut deck = b.deck.clone();
    deck.theme = Some(serde_json::Value::String(rel.clone()));
    let after = lint_bundle(&b, &deck, &Overlay { base: &b.files, path: &rel, text: &text }, Some(&text))?.0;

    let states: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
    let Delta { added, removed } = scaena_core::lint::delta(&before, &states, &after, &states, &[]);
    let errors = after.iter().filter(|f| f.severity == Severity::Error).count();

    if !dry_run {
        let mut files = BTreeMap::new();
        files.insert(b.deck_file.clone(), (deck.to_json()? + "\n").into_bytes());
        if inside.is_none() || !mapped.is_empty() {
            files.insert(rel.clone(), text.into_bytes());
        }
        b.write(&files).with_context(|| format!("writing {}", bundle.display()))?;
    }
    if json {
        let v = serde_json::json!({
            "theme": rel, "was": was, "applied": !dry_run, "mapped": mapped,
            "added": added, "removed": removed, "errors": errors,
        });
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        let verb = if dry_run { "would apply" } else { "applied" };
        println!("{verb} {rel} (was {})", was.as_deref().unwrap_or("no theme"));
        for m in &mapped {
            println!("  {m}");
        }
        print_delta(&added, &removed);
    }
    Ok(if errors > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// A lint delta, a line a finding: `+` added, `-` removed.
fn print_delta(added: &[&Finding], removed: &[&Finding]) {
    if added.is_empty() && removed.is_empty() {
        println!("lint delta: none");
    }
    for (sign, list) in [("+", added), ("-", removed)] {
        for f in list {
            let loc = match &f.file {
                Some(file) => format!("{file} {}", f.path.as_deref().unwrap_or("")),
                None => f.path.clone().unwrap_or_default(),
            };
            let format = f.format.as_deref().map(|f| format!(" [{f}]")).unwrap_or_default();
            println!("{sign} {}{format} {loc}: {}", f.code, f.message);
        }
    }
}

/// `scaena patch` (PLAN 1.16, SPEC §7.3): compile the ops, JSON Patch and semantic, against
/// the deck, each against the deck as the ops before it leave it; check the deck they make
/// as `validate` checks a bundle; and write it, canonically, unless that adds a validation
/// error. Reports the patch as RFC 6902 and the delta in what `validate` and `lint` find.
/// An op that does not apply exits 2; a patch that would make the deck invalid exits 1,
/// refused. Either way, and under `--dry-run`, nothing is written. Errors in the deck it
/// makes exit 1, as `lint` does.
fn patch(bundle: &Path, ops: &Path, dry_run: bool, json: bool) -> Result<ExitCode> {
    let b = open(bundle)?;
    let source = if ops == Path::new("-") {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text).context("reading the ops from stdin")?;
        text
    } else {
        std::fs::read_to_string(ops).with_context(|| format!("reading {}", ops.display()))?
    };
    let parsed: serde_json::Value =
        serde_json::from_str(&source).with_context(|| format!("{} is not JSON", ops.display()))?;
    let Some(list) = parsed.as_array() else {
        anyhow::bail!("a patch is a JSON array of ops (SPEC §7.3, docs/schema/patch.schema.json)");
    };
    let doc = serde_json::to_value(&b.deck)?;
    let compiled = match scaena_core::patch::compile(&doc, list, &b.files) {
        Ok(compiled) => compiled,
        Err(e) => return Ok(stop(json, serde_json::json!({ "exit": 2, "message": e.to_string(), "op": e.index }))),
    };
    let was: Vec<&str> = b.deck.states.iter().map(|s| s.id.as_str()).collect();
    let is: Vec<&str> =
        compiled.doc["states"].as_array().into_iter().flatten().map(|s| s["id"].as_str().unwrap_or_default()).collect();
    // The deck it makes, checked as `validate` checks a bundle: a validation error it adds
    // refuses the whole patch.
    let text = serde_json::to_string_pretty(&compiled.doc)?;
    let invalid = scaena_core::validate::validate_bundle(&b.deck.to_json()?, &b.files)?;
    let invalid_after = scaena_core::validate::validate_bundle(&text, &b.files)?;
    let broken = scaena_core::lint::delta(&invalid, &was, &invalid_after, &is, &compiled.renamed).added;
    let (before, after, refused) = if broken.is_empty() {
        let next = b.with_deck(scaena_core::Deck::from_json(&text).context("the patched deck")?)?;
        let (before, _) = lint_bundle(&b, &b.deck, &b.files, b.theme_json.as_deref())?;
        let (after, _) = lint_bundle(&next, &next.deck, &next.files, next.theme_json.as_deref())?;
        if !dry_run && compiled.doc != doc {
            let mut files = std::collections::BTreeMap::new();
            files.insert(b.deck_file.clone(), (next.deck.to_json()? + "\n").into_bytes());
            b.write(&files).with_context(|| format!("writing {}", bundle.display()))?;
        }
        (before, after, false)
    } else {
        (invalid, invalid_after, true)
    };
    let Delta { added, removed } = scaena_core::lint::delta(&before, &was, &after, &is, &compiled.renamed);
    let errors = after.iter().filter(|f| f.severity == Severity::Error).count();
    let applied = !refused && !dry_run;
    if json {
        let v = serde_json::json!({
            "applied": applied, "patch": compiled.patch, "added": added, "removed": removed, "errors": errors,
        });
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        let (n, m) = (list.len(), compiled.patch.len());
        match (refused, dry_run) {
            (true, _) => println!("refused: the patch would make the deck invalid; nothing was written"),
            (false, true) => println!("would apply {n} ops, {m} as JSON Patch"),
            (false, false) => println!("applied {n} ops, {m} as JSON Patch"),
        }
        for op in &compiled.patch {
            println!("  {}", serde_json::to_string(op)?);
        }
        print_delta(&added, &removed);
    }
    Ok(if errors > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// What `lint` finds in a bundle (SPEC §7.5): its validation, the document rules, and,
/// once it has no errors, the layout rules in its own format and each of its `formats`,
/// contrast painted by the CPU painter. `deck`, `files`, and `theme` may be the bundle
/// as it would be (`theme --apply`); its fonts, images, and data are the bundle's. The
/// flag: whether the layout rules ran.
fn lint_bundle(
    b: &Bundle,
    deck: &scaena_core::Deck,
    files: &dyn scaena_core::validate::BundleFiles,
    theme: Option<&str>,
) -> Result<(Vec<Finding>, bool)> {
    let mut found = scaena_core::validate::validate_bundle(&deck.to_json()?, files)?;
    let model: Option<scaena_core::model::theme::Theme> = theme.and_then(|t| serde_json::from_str(t).ok());
    found.extend(scaena_core::lint::check(deck, model.as_ref()));
    let laid = theme.is_some() && !found.iter().any(|f| f.severity == Severity::Error);
    if let (true, Some(text)) = (laid, theme) {
        let theme = Theme::from_json(text)?;
        let mut assets = Assets::new();
        let mut engine = engine_with(b, &theme, Some(&mut assets))?;
        let data = data_files(b)?;
        let mut backdrop = scaena_paint::Backdrop { painter: CpuPainter::default(), assets: &assets };
        found.extend(scaena_engine::lint::lint(&mut engine, deck, &theme, &data, Some(&mut backdrop))?);
    }
    scaena_core::lint::sort(&mut found);
    Ok((found, laid))
}

/// `scaena lint --fix`: apply every fix lint offers (each checked by laying its state out
/// with it, never a change of content), write the deck, and lint again. Under `--json`:
/// `{ "fixed": [findings], "findings": [what remains] }`.
fn lint_fix(path: &Path, b: &Bundle, json: bool) -> Result<ExitCode> {
    let (found, _) = lint_bundle(b, &b.deck, &b.files, b.theme_json.as_deref())?;
    let mut doc = serde_json::to_value(&b.deck)?;
    let mut fixed: Vec<&Finding> = Vec::new();
    for f in found.iter().filter(|f| f.fix.is_some()) {
        // Two findings can carry one fix (the same text in two states); it applies once.
        let patch = f.fix.as_deref().unwrap_or_default();
        let mut next = doc.clone();
        if scaena_core::patch::apply(&mut next, patch).is_ok() {
            doc = next;
            fixed.push(f);
        }
    }
    let deck = scaena_core::Deck::from_json(&doc.to_string()).context("the fixed deck")?;
    if !fixed.is_empty() {
        let mut files = std::collections::BTreeMap::new();
        files.insert(b.deck_file.clone(), (deck.to_json()? + "\n").into_bytes());
        b.write(&files).with_context(|| format!("writing {}", path.display()))?;
    }
    let (after, _) = lint_bundle(b, &deck, &b.files, b.theme_json.as_deref())?;
    if json {
        let v = serde_json::json!({ "fixed": fixed, "findings": after });
        println!("{}", serde_json::to_string_pretty(&v)?);
    } else {
        for f in &fixed {
            let ops: Vec<String> = f.fix.iter().flatten().map(|op| op.to_string()).collect();
            println!("fixed {} {}: {}", f.code, f.node.as_deref().unwrap_or_default(), ops.join(" "));
        }
        if fixed.is_empty() {
            println!("nothing to fix");
        }
        report(&after, false);
    }
    let errors = after.iter().any(|f| f.severity == Severity::Error);
    Ok(if errors { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// A bundle's files with one more, or one replaced: the bundle as it would be.
struct Overlay<'a> {
    base: &'a scaena_store::Files,
    path: &'a str,
    text: &'a str,
}

impl scaena_core::validate::BundleFiles for Overlay<'_> {
    fn exists(&self, path: &str) -> bool {
        path == self.path || self.base.exists(path)
    }

    fn read_text(&self, path: &str) -> Option<String> {
        if path == self.path { Some(self.text.to_string()) } else { self.base.read_text(path) }
    }
}

/// What `inspect` shows of each state besides its snapshot.
#[derive(Clone, Copy)]
struct Views {
    resolved: bool,
    timeline: bool,
    data: bool,
}

/// `scaena inspect`: each state's snapshot, tracking applied (SPEC §2.2). `--resolved`
/// takes it through the theme cascade (PLAN 1.6); `--timeline` adds its cue and `--data`
/// the rows its charts and tables read (PLAN 1.14).
fn inspect(b: &Bundle, state: Option<&str>, views: Views, json: bool) -> Result<ExitCode> {
    use scaena_engine::cascade;
    use scaena_engine::layout::Grid;
    use serde_json::{Map, Value};
    let snaps = scaena_core::resolve_states(&b.deck).context("tracking")?;
    let selected: Vec<&scaena_core::Snapshot> =
        snaps.iter().filter(|s| state.is_none_or(|id| s.state_id == id)).collect();
    if selected.is_empty() {
        anyhow::bail!("unknown state `{}`", state.unwrap_or_default());
    }
    let theme = match views.resolved || views.timeline {
        true => Some(Theme::from_json(b.theme_json.as_deref().context("the deck names no theme")?)?),
        false => None,
    };
    let files = if views.timeline || views.data { data_files(b)? } else { DataFiles::new() };
    // A cue on lines, words, or a chart's marks counts them after layout, so the
    // timeline needs the engine, with the bundle's fonts and images, as `render` does.
    let mut cues = match (&theme, views.timeline) {
        (Some(theme), true) => {
            let mut engine = engine_with(b, theme, None)?;
            let timeline = engine.timeline(&b.deck, theme, &files)?;
            Some((engine, timeline))
        }
        _ => None,
    };
    let mut out = Vec::new();
    for s in selected {
        let snap = if views.resolved { cascade::with_overrides(&b.deck, s) } else { s.clone() };
        let mut v = serde_json::to_value(&snap)?;
        if let (true, Some(theme)) = (views.resolved, &theme) {
            let mut looks = Map::new();
            let mut overrides = Map::new();
            for (id, props) in &snap.nodes {
                if b.deck.nodes[id].node_type == scaena_core::document::NodeType::Text {
                    let slot = Grid::slot_role(theme, snap.layout.as_deref(), props.get("at"));
                    let look = cascade::look(theme, props, slot.as_deref()).with_context(|| format!("node `{id}`"))?;
                    looks.insert(id.clone(), serde_json::to_value(look)?);
                }
                let over = b.deck.overridden(id);
                if !over.is_empty() {
                    overrides.insert(id.clone(), serde_json::to_value(over)?);
                }
            }
            v["looks"] = looks.into();
            v["overrides"] = overrides.into();
        }
        if let (Some((engine, timeline)), Some(theme)) = (&mut cues, &theme) {
            let slot = timeline.slot(&s.state_id).context("a state missing from the timeline")?;
            let cue = engine.transition(&b.deck, theme, &files, &s.state_id)?;
            v["timeline"] = cue_json(slot, &cue);
        }
        if views.data {
            v["data"] = rows_json(b, &files, &cascade::with_overrides(&b.deck, s))?;
        }
        out.push((snap, v));
    }
    if json {
        let all: Vec<&Value> = out.iter().map(|(_, v)| v).collect();
        println!("{}", serde_json::to_string_pretty(&all)?);
        return Ok(ExitCode::SUCCESS);
    }
    for (s, v) in &out {
        println!("state {}  slide={}  layout={}", s.state_id, s.slide_id, s.layout.as_deref().unwrap_or("-"));
        if let Some(cue) = v.get("timeline") {
            print_cue(cue);
        }
        for (id, props) in &s.nodes {
            let marker = if s.entered.contains(id) { "+" } else { " " };
            println!("  {marker} {id:<12} {}", summarize(props));
            if let Some(l) = v["looks"].get(id) {
                println!(
                    "      {}: {} {} (leading {}), weight {}, tracking {}, {} {}",
                    l["role"].as_str().unwrap_or_default(),
                    l["family"].as_str().unwrap_or_default(),
                    l["size"],
                    l["leading"],
                    l["weight"],
                    l["tracking"],
                    l["color"].as_str().unwrap_or_default(),
                    l["hex"].as_str().unwrap_or_default(),
                );
            }
            if let Some(over) = v["overrides"].get(id).and_then(|o| o.as_array()) {
                let names: Vec<&str> = over.iter().filter_map(|p| p.as_str()).map(|p| &p[1..]).collect();
                println!("      {} override(s), not theme-safe: {}", names.len(), names.join(", "));
            }
            if let Some(rows) = v["data"].get(id) {
                print_rows(rows);
            }
        }
        if !s.exited.is_empty() {
            println!("  - exited: {}", s.exited.join(", "));
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// A state's cue (SPEC §2.4, §3.9), ms: where it falls on the deck's timeline (its
/// `start`, its `span` of transition and motions, and its `hold` at rest), its transition,
/// and each motion as placed on the state's clock, which starts with the transition.
fn cue_json(slot: &scaena_core::timeline::Slot, cue: &scaena_engine::sample::Transition) -> serde_json::Value {
    let timing = cue.timing();
    let motions: Vec<serde_json::Value> = cue.schedule().cues.iter().map(motion_json).collect();
    serde_json::json!({
        "start": ms(slot.start),
        "span": ms(slot.span),
        "hold": ms(slot.hold),
        "transition": {
            "duration": ms(timing.duration_ms),
            "curve": curve_json(&timing.curve),
            "match": if timing.matched { "id" } else { "none" },
        },
        "motions": motions,
    })
}

/// One motion on one node: what it does (`enter` from a look, `exit` to one, `emphasis`
/// out to a peak and back, or `anim` tracks), over how many units, and when, ms.
fn motion_json(p: &scaena_core::timeline::Placed) -> serde_json::Value {
    use scaena_core::timeline::Motion;
    let (kind, key, look) = match &p.motion {
        Motion::Enter(l) => ("enter", "from", look_json(l)),
        Motion::Exit(l) => ("exit", "to", look_json(l)),
        Motion::Emphasis(l) => ("emphasis", "peak", look_json(l)),
        Motion::Keys(k) => ("anim", "tracks", keys_json(k)),
    };
    let mut v = serde_json::json!({
        "node": p.node,
        "motion": kind,
        "split": p.split,
        "units": p.units,
        "start": ms(p.start),
        "stagger": ms(p.stagger),
        "duration": ms(p.duration),
        "end": ms(p.end()),
        "curve": curve_json(&p.curve),
    });
    v[key] = look;
    v
}

/// A look as what it changes from rest.
fn look_json(l: &scaena_core::timeline::Look) -> serde_json::Value {
    use scaena_core::timeline::Look;
    let rest = Look::REST;
    let mut m = serde_json::Map::new();
    let mut put = |k: &str, v: serde_json::Value| m.insert(k.to_string(), v);
    if l.opacity != rest.opacity {
        put("opacity", l.opacity.into());
    }
    if l.translate != rest.translate {
        put("translate", serde_json::json!(l.translate));
    }
    if l.scale != rest.scale {
        put("scale", serde_json::json!(l.scale));
    }
    if l.rotate != rest.rotate {
        put("rotate", l.rotate.into());
    }
    if l.anchor != rest.anchor {
        put("anchor", serde_json::json!(l.anchor));
    }
    if let Some((color, amount)) = l.tint {
        put("tint", serde_json::json!({ "color": color.to_hex(), "amount": amount }));
    }
    if l.progress != rest.progress {
        put("progress", l.progress.into());
    }
    m.into()
}

/// An `anim`'s tracks, each its keys: when (ms after the track starts), the value, and
/// the curve from the key before.
fn keys_json(k: &scaena_core::timeline::Keys) -> serde_json::Value {
    use scaena_core::timeline::Key;
    fn track<T: Copy>(keys: &[Key<T>], v: impl Fn(T) -> serde_json::Value) -> serde_json::Value {
        keys.iter().map(|k| serde_json::json!({ "t": ms(k.t), "v": v(k.v), "curve": curve_json(&k.curve) })).collect()
    }
    let (one, two) = (|v: f64| serde_json::json!(v), |v: [f64; 2]| serde_json::json!(v));
    let mut m = serde_json::Map::new();
    for (name, keys, empty) in [
        ("opacity", track(&k.opacity, one), k.opacity.is_empty()),
        ("translate", track(&k.translate, two), k.translate.is_empty()),
        ("scale", track(&k.scale, two), k.scale.is_empty()),
        ("rotate", track(&k.rotate, one), k.rotate.is_empty()),
        ("progress", track(&k.progress, one), k.progress.is_empty()),
    ] {
        if !empty {
            m.insert(name.to_string(), keys);
        }
    }
    if k.anchor != scaena_core::timeline::Look::REST.anchor {
        m.insert("anchor".to_string(), serde_json::json!(k.anchor));
    }
    m.into()
}

/// An easing as its cubic Bézier, or a spring as its constants.
fn curve_json(c: &scaena_core::timeline::Curve) -> serde_json::Value {
    use scaena_core::timeline::{CubicBezier, Curve};
    match c {
        Curve::Ease(CubicBezier(x1, y1, x2, y2)) => serde_json::json!({ "ease": [x1, y1, x2, y2] }),
        Curve::Spring(s, _) => {
            serde_json::json!({ "spring": { "stiffness": s.stiffness, "damping": s.damping, "mass": s.mass } })
        }
    }
}

/// The rows each chart and table in a state reads (SPEC §3.10): its source through its
/// `dataTransform`, as the engine reads them, cells typed by the source's schema.
fn rows_json(b: &Bundle, files: &DataFiles, snap: &scaena_core::Snapshot) -> Result<serde_json::Value> {
    use scaena_core::document::NodeType;
    use scaena_engine::data::{self, Datum};
    let cell = |d: &Datum| match d {
        Datum::Number(n) => serde_json::json!(n),
        Datum::Text(s) => serde_json::json!(s),
        Datum::Bool(b) => serde_json::json!(b),
        Datum::Date(_) => serde_json::json!(d.label()),
        Datum::Null => serde_json::Value::Null,
    };
    let mut out = serde_json::Map::new();
    for (id, props) in &snap.nodes {
        if !matches!(b.deck.nodes[id].node_type, NodeType::Chart | NodeType::Table) {
            continue;
        }
        let Some(source) = props.get("data").and_then(|d| d.as_str()).and_then(|d| d.strip_prefix('@')) else {
            continue;
        };
        let table = data::load(&b.deck, files, source)
            .and_then(|t| data::transform(t, props.get("dataTransform")))
            .with_context(|| format!("node `{id}` in state `{}`", snap.state_id))?;
        let rows: Vec<Vec<serde_json::Value>> = table.rows.iter().map(|r| r.iter().map(cell).collect()).collect();
        let types: Vec<&str> = table.types.iter().map(|t| t.name()).collect();
        out.insert(
            id.clone(),
            serde_json::json!({ "source": source, "columns": table.columns, "types": types, "rows": rows }),
        );
    }
    Ok(out.into())
}

/// `inspect --timeline`, for a person: the state's place on the timeline, its
/// transition, and a line per motion.
fn print_cue(cue: &serde_json::Value) {
    let ms = |v: &serde_json::Value| num(v.as_f64().unwrap_or_default());
    let t = &cue["transition"];
    let transition = match t["duration"].as_f64() {
        Some(d) if d > 0.0 => {
            let unmatched = if t["match"] == "none" { ", match none" } else { "" };
            format!("transition {} ms, {}{unmatched}", num(d), curve_words(&t["curve"]))
        }
        _ => "cut".to_string(),
    };
    let start = cue["start"].as_f64().unwrap_or_default();
    let (span, hold) = (cue["span"].as_f64().unwrap_or_default(), cue["hold"].as_f64().unwrap_or_default());
    println!(
        "  at {}–{} ms on the timeline: span {}, hold {} · {transition}",
        num(start),
        num(start + span + hold),
        num(span),
        num(hold)
    );
    for m in cue["motions"].as_array().into_iter().flatten() {
        let units = match m["split"].as_str() {
            Some(unit) => format!(" by {unit} ×{}", m["units"]),
            None => String::new(),
        };
        let stagger = match m["stagger"].as_f64() {
            Some(s) if s > 0.0 => format!(", {} ms apart", num(s)),
            _ => String::new(),
        };
        let look = ["from", "to", "peak", "tracks"]
            .into_iter()
            .find_map(|k| m.get(k).map(|v| format!(" · {k} {}", words(v))))
            .unwrap_or_default();
        println!(
            "  ▸ {:<12} {}{units} {}–{} ms, {} ms each{stagger}, {}{look}",
            m["node"].as_str().unwrap_or_default(),
            m["motion"].as_str().unwrap_or_default(),
            ms(&m["start"]),
            ms(&m["end"]),
            ms(&m["duration"]),
            curve_words(&m["curve"]),
        );
    }
}

/// `inspect --data`, for a person: the source, its size, and the first rows.
fn print_rows(rows: &serde_json::Value) {
    const SHOWN: usize = 12;
    let columns: Vec<String> =
        rows["columns"].as_array().into_iter().flatten().map(|c| c.as_str().unwrap_or_default().to_string()).collect();
    let all = rows["rows"].as_array().map(Vec::as_slice).unwrap_or_default();
    println!(
        "      data @{}: {} rows × {} columns",
        rows["source"].as_str().unwrap_or_default(),
        all.len(),
        columns.len()
    );
    let cell = |v: &serde_json::Value| match v {
        serde_json::Value::String(s) => truncate(s, 24),
        serde_json::Value::Number(n) => num(n.as_f64().unwrap_or_default()),
        serde_json::Value::Null => "–".to_string(),
        other => other.to_string(),
    };
    let table: Vec<Vec<String>> = std::iter::once(columns.clone())
        .chain(all.iter().take(SHOWN).map(|r| r.as_array().into_iter().flatten().map(cell).collect()))
        .collect();
    let widths: Vec<usize> = (0..columns.len())
        .map(|i| table.iter().map(|r| r.get(i).map_or(0, |c| c.chars().count())).max().unwrap_or(0))
        .collect();
    for row in &table {
        let cells: Vec<String> = row.iter().zip(&widths).map(|(c, w)| format!("{c:<w$}")).collect();
        println!("        {}", cells.join("  ").trim_end());
    }
    if all.len() > SHOWN {
        println!("        … {} more", all.len() - SHOWN);
    }
}

/// A curve in a few words: `ease(0.2, 0, 0, 1)` or `spring(420, 34, 1)`.
fn curve_words(c: &serde_json::Value) -> String {
    let list = |v: &serde_json::Value| -> String {
        v.as_array().into_iter().flatten().map(|x| num(x.as_f64().unwrap_or_default())).collect::<Vec<_>>().join(", ")
    };
    if let Some(e) = c.get("ease") {
        return format!("ease({})", list(e));
    }
    let s = &c["spring"];
    let at = |k: &str| num(s[k].as_f64().unwrap_or_default());
    format!("spring({}, {}, {})", at("stiffness"), at("damping"), at("mass"))
}

/// A look or an `anim`'s tracks in a few words: `opacity 0, translate 0 24`, or
/// `opacity 0@0 1@400` (each key a value at a time).
fn words(look: &serde_json::Value) -> String {
    fn value(v: &serde_json::Value) -> String {
        match v {
            serde_json::Value::Number(n) => num(n.as_f64().unwrap_or_default()),
            serde_json::Value::String(s) => s.clone(),
            serde_json::Value::Array(a) => a.iter().map(value).collect::<Vec<_>>().join(" "),
            serde_json::Value::Object(m) if m.contains_key("t") => format!("{}@{}", value(&m["v"]), value(&m["t"])),
            serde_json::Value::Object(m) => m.values().map(value).collect::<Vec<_>>().join(" "),
            other => other.to_string(),
        }
    }
    let props = look.as_object().into_iter().flatten();
    props.map(|(k, v)| format!("{k} {}", value(v))).collect::<Vec<_>>().join(", ")
}

/// A time in ms to the microsecond: a spring's settle time is not exact, and reads no
/// better for its last digits.
fn ms(x: f64) -> f64 {
    let rounded = (x * 1000.0).round() / 1000.0;
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// A number as a person reads it: no trailing zeros, at most three decimals.
fn num(x: f64) -> String {
    format!("{}", ms(x))
}

/// The bundle's data files, as the engine reads them.
fn data_files(b: &Bundle) -> Result<DataFiles> {
    let mut data = DataFiles::new();
    for (path, bytes) in b.read_data()? {
        data.insert(path, bytes);
    }
    Ok(data)
}

/// An engine with the bundle's fonts and images registered, for what only layout knows;
/// and, given a store, the same fonts and images for a painter.
fn engine_with(b: &Bundle, theme: &Theme, mut store: Option<&mut Assets>) -> Result<Engine> {
    let mut fonts = BundleFonts::new();
    for (id, bytes) in b.read_fonts()? {
        if let Some(store) = store.as_deref_mut() {
            store.insert_font(&id, bytes.clone());
        }
        fonts.register(&id, bytes)?;
    }
    fonts.check_theme(theme)?;
    let mut images = BundleImages::new();
    for (path, bytes) in b.read_images()? {
        let info = images.register(&path, &bytes)?;
        if let Some(store) = store.as_deref_mut() {
            store.insert_image(&info.id, &bytes)?;
        }
    }
    Ok(Engine::new(fonts).with_images(images))
}

/// `scaena render` (PLAN 0.6, 0.7): bundle fonts → `Engine::frame` → painter → PNG.
/// Timings are wall clock in this client; the render path itself never reads a clock.
fn render(args: RenderArgs, json: bool) -> Result<ExitCode> {
    let RenderArgs { bundle, state, t, format, size, out, display_list, painter } = args;
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
    let data = data_files(&b)?;
    let load = lap();

    let mut fonts = BundleFonts::new();
    let mut store = Assets::new();
    for (id, bytes) in files {
        store.insert_font(&id, bytes.clone());
        fonts.register(&id, bytes)?;
    }
    fonts.check_theme(&theme)?;
    let mut images = BundleImages::new();
    for (path, bytes) in b.read_images()? {
        let info = images.register(&path, &bytes)?;
        store.insert_image(&info.id, &bytes)?;
    }
    let register = lap();

    let req = FrameRequest {
        deck: &b.deck,
        theme: &theme,
        data: &data,
        state,
        t_ms: t.unwrap_or(f64::INFINITY),
        format: format.as_deref(),
    };
    let frame = Engine::new(fonts).with_images(images).frame(&req)?;
    let (dl, span) = (frame.display_list, frame.duration_ms);
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
            let message = "`--painter gpu` needs the CLI built with `--features gpu` (PLAN 0.7)";
            return Ok(fail(json, 3, message, Some("0.7")));
        }
    };
    let init = lap();
    let raster = painter.paint(&dl, &store, scale)?;
    let paint = lap();
    let out = out.unwrap_or_else(|| PathBuf::from(format!("{state}.png")));
    std::fs::write(&out, raster.to_png_fast()?).with_context(|| format!("writing {}", out.display()))?;
    let encode = lap();
    let total = (Instant::now() - start).as_secs_f64() * 1e3;

    if json {
        let us = |ms: f64| (ms * 1e3).round() / 1e3;
        let summary = serde_json::json!({
            "state": state,
            "format": format,
            "t_ms": t,
            "span_ms": span,
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

fn not_yet(json: bool, what: &str, plan: &str) -> ExitCode {
    fail(json, 3, &format!("`{what}` is not implemented yet — see docs/PLAN.md task {plan}"), Some(plan))
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
        let format = f.format.as_deref().map(|f| format!(" [{f}]")).unwrap_or_default();
        println!("{sev} {}{format} {loc}: {}", f.code, f.message);
        if let Some(h) = &f.hint {
            println!("        hint: {h}");
        }
        if let Some(fix) = &f.fix {
            let ops: Vec<String> = fix.iter().map(|op| op.to_string()).collect();
            println!("        fix: {} (`lint --fix` applies it)", ops.join(" "));
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
