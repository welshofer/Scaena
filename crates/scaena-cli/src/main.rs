//! `scaena` — the command-line tool (SPEC §7.1). The first client (ADR-0003).
//!
//! Exit codes: 0 ok · 1 lint errors · 2 invalid input · 3 internal/not implemented.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use scaena_core::{Finding, Severity};
use scaena_engine::EngineError;
use scaena_ops::OpsError;
use scaena_ops::inspect::Views;
use scaena_ops::lint::Linted;
use scaena_ops::render::Painter;
use scaena_paint::PaintError;
use scaena_store::{Bundle, SaveOptions};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

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
    /// MCP server on stdio (PLAN 1.17, SPEC §7.2): the operations above as tools, and the
    /// format's schemas, lint catalog, and examples as resources.
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
            let ops = e.chain().find_map(|c| c.downcast_ref::<OpsError>());
            if let Some(plan) = ops.and_then(|o| o.plan.as_deref()) {
                fail(json, 3, &message, Some(plan))
            } else if e.chain().any(not_built) {
                fail(json, 3, &message, scaena_ops::plan_task(&message).as_deref())
            } else if let Some(op) = ops.and_then(|o| o.op) {
                // A patch's op that does not apply: which one.
                stop(json, serde_json::json!({ "exit": 2, "message": message, "op": op }))
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

fn run(cli: Cli) -> Result<ExitCode> {
    match cli.cmd {
        Cmd::Validate { bundle } => {
            let findings = scaena_ops::lint::validate(&bundle)?;
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
                return lint_fix(&b, cli.json);
            }
            let min: Severity = severity.into();
            let Linted { findings: found, laid } = scaena_ops::lint::lint(&b)?;
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
            use scaena_ops::inspect::Change;
            let changes = scaena_ops::inspect::diff(&open(&bundle)?, &from, &to)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&changes)?);
            } else if changes.is_empty() {
                println!("no changes from `{from}` to `{to}`");
            } else {
                for (id, change) in &changes {
                    match change {
                        Change::Enter(_) => println!("+ {id}"),
                        Change::Exit(_) => println!("- {id}"),
                        Change::Change(delta) => {
                            println!("~ {id}: {}", delta.keys().map(String::as_str).collect::<Vec<_>>().join(", "))
                        }
                    }
                }
            }
            Ok(ExitCode::SUCCESS)
        }
        Cmd::Export { bundle, format, out, states, .. } => {
            let v = scaena_ops::export::export(&open(&bundle)?, &format, states.as_deref())?;
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
        Cmd::Mcp => {
            scaena_mcp::stdio().context("serving MCP on stdio")?;
            Ok(ExitCode::SUCCESS)
        }
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
    let t = scaena_ops::theme::theme_apply(&open(bundle)?, theme, dry_run)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&t)?);
    } else {
        let verb = if dry_run { "would apply" } else { "applied" };
        println!("{verb} {} (was {})", t.theme, t.was.as_deref().unwrap_or("no theme"));
        for m in &t.mapped {
            println!("  {m}");
        }
        print_delta(&t.added, &t.removed);
    }
    Ok(if t.errors > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// A lint delta, a line a finding: `+` added, `-` removed.
fn print_delta(added: &[Finding], removed: &[Finding]) {
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
/// the deck; check the deck they make as `validate` checks a bundle; and write it,
/// canonically, unless that adds a validation error. Reports the patch as RFC 6902 and the
/// delta in what `validate` and `lint` find. An op that does not apply exits 2; a patch that
/// would make the deck invalid exits 1, refused. Either way, and under `--dry-run`, nothing
/// is written. Errors in the deck it makes exit 1, as `lint` does.
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
    let p = scaena_ops::patch::patch(&b, &parsed, dry_run)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&p)?);
    } else {
        let (n, m) = (parsed.as_array().map_or(0, Vec::len), p.patch.len());
        match (p.refused, dry_run) {
            (true, _) => println!("refused: the patch would make the deck invalid; nothing was written"),
            (false, true) => println!("would apply {n} ops, {m} as JSON Patch"),
            (false, false) => println!("applied {n} ops, {m} as JSON Patch"),
        }
        for op in &p.patch {
            println!("  {}", serde_json::to_string(op)?);
        }
        print_delta(&p.added, &p.removed);
    }
    Ok(if p.errors > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// `scaena lint --fix`: apply every fix lint offers (each checked by laying its state out
/// with it, never a change of content), write the deck, and lint again. Under `--json`:
/// `{ "fixed": [findings], "findings": [what remains] }`.
fn lint_fix(b: &Bundle, json: bool) -> Result<ExitCode> {
    let fixed = scaena_ops::lint::lint_fix(b)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&fixed)?);
    } else {
        for f in &fixed.fixed {
            let ops: Vec<String> = f.fix.iter().flatten().map(|op| op.to_string()).collect();
            println!("fixed {} {}: {}", f.code, f.node.as_deref().unwrap_or_default(), ops.join(" "));
        }
        if fixed.fixed.is_empty() {
            println!("nothing to fix");
        }
        report(&fixed.findings, false);
    }
    let errors = fixed.findings.iter().any(|f| f.severity == Severity::Error);
    Ok(if errors { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// `scaena inspect`: each state's snapshot, tracking applied (SPEC §2.2). `--resolved`
/// takes it through the theme cascade (PLAN 1.6); `--timeline` adds its cue and `--data`
/// the rows its charts and tables read (PLAN 1.14).
fn inspect(b: &Bundle, state: Option<&str>, views: Views, json: bool) -> Result<ExitCode> {
    let out = scaena_ops::inspect::inspect(b, state, views)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(ExitCode::SUCCESS);
    }
    for i in &out {
        let (s, v) = (&i.snapshot, serde_json::to_value(i)?);
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

/// A number as a person reads it: no trailing zeros, at most three decimals.
fn num(x: f64) -> String {
    format!("{}", scaena_ops::inspect::ms(x))
}

/// `scaena render` (PLAN 0.6, 0.7): bundle fonts → `Engine::frame` → painter → PNG, with
/// the display list if asked for.
fn render(args: RenderArgs, json: bool) -> Result<ExitCode> {
    let RenderArgs { bundle, state, t, format, size, out, display_list, painter } = args;
    let painter = match painter {
        PainterArg::Cpu => Painter::Cpu,
        PainterArg::Gpu => Painter::Gpu,
    };
    let req = scaena_ops::render::Request { state: state.clone(), t, format: format.clone(), size, painter };
    let r = scaena_ops::render::render(&bundle, &req)?;
    if let Some(path) = &display_list {
        std::fs::write(path, r.display_list.to_golden_json()?)
            .with_context(|| format!("writing {}", path.display()))?;
    }
    let out = out.unwrap_or_else(|| PathBuf::from(format!("{state}.png")));
    std::fs::write(&out, &r.png).with_context(|| format!("writing {}", out.display()))?;
    if json {
        let summary = serde_json::json!({
            "state": state,
            "format": format,
            "t_ms": t,
            "span_ms": r.span_ms,
            "painter": r.painter,
            "adapter": r.adapter,
            "size": r.size,
            "out": out,
            "display_list": display_list,
            "ms": r.ms,
        });
        println!("{}", serde_json::to_string_pretty(&summary)?);
    } else {
        let m = r.ms;
        println!(
            "{} ({}×{}, {}) in {:.0} ms: load {:.1} · fonts {:.1} · frame {:.1} · init {:.1} · paint {:.1} · png {:.1}",
            out.display(),
            r.size[0],
            r.size[1],
            r.adapter.as_deref().unwrap_or(r.painter),
            m.total,
            m.load,
            m.fonts,
            m.frame,
            m.init,
            m.paint,
            m.png,
        );
    }
    Ok(ExitCode::SUCCESS)
}

fn open(path: &Path) -> Result<Bundle> {
    Ok(scaena_ops::open(path)?)
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
