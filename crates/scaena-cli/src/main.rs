//! `scaena` — the command-line tool (SPEC §7.1). The first client (ADR-0003).
//!
//! Exit codes: 0 ok · 1 lint errors · 2 invalid input · 3 internal/not implemented.

use anyhow::{Context, Result};
use clap::{Parser, Subcommand, ValueEnum};
use scaena_core::{Finding, Severity};
use scaena_store::Bundle;
use std::path::PathBuf;
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
    },
    /// What changes between two states (resolved).
    Diff {
        bundle: PathBuf,
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
    },
    /// DSL → JSON (PLAN 1.5).
    Compile {
        input: PathBuf,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// JSON → canonical DSL (PLAN 1.5).
    Decompile {
        input: PathBuf,
        #[arg(short, long)]
        out: Option<PathBuf>,
    },
    /// Render one frame (PLAN 0.6+).
    Render {
        bundle: PathBuf,
        #[arg(long)]
        state: String,
        #[arg(long, default_value_t = 0.0)]
        t: f64,
        #[arg(long, default_value = "1920x1080")]
        size: String,
        #[arg(long)]
        out: Option<PathBuf>,
        #[arg(long)]
        display_list: Option<PathBuf>,
        #[arg(long, default_value = "cpu")]
        painter: String,
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
            let b = open(&bundle)?;
            let findings = scaena_core::validate::validate(&b.deck);
            report(&findings, cli.json);
            Ok(if findings.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
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
        Cmd::Compile { .. } | Cmd::Decompile { .. } => not_yet("compile/decompile (DSL)", "1.5"),
        Cmd::Render { .. } => not_yet("render", "0.3–0.10"),
        Cmd::Patch { .. } => not_yet("patch", "1.16"),
        Cmd::Theme { .. } => not_yet("theme --apply", "1.6"),
        Cmd::Serve { .. } => not_yet("serve", "2.x"),
        Cmd::Mcp => not_yet("mcp", "1.17"),
    }
}

fn open(path: &std::path::Path) -> Result<Bundle> {
    Bundle::open(path).with_context(|| format!("opening {}", path.display()))
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
        let loc = f.path.as_deref().unwrap_or("");
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
            parts.push(format!("{key}={}", if s.len() > 32 { format!("{}…", &s[..32]) } else { s }));
        }
    }
    parts.join("  ")
}
