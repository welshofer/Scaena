//! `scaena` — the command-line tool (SPEC §7.1). The first client (ADR-0003).
//!
//! Exit codes: 0 ok · 1 lint errors · 2 invalid input · 3 internal/not implemented.

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand, ValueEnum};
use scaena_core::{Finding, Severity};
use scaena_engine::EngineError;
use scaena_ops::OpsError;
use scaena_ops::arrange::{Align, Order, Spread};
use scaena_ops::inspect::{SnapMode, Views};
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
    /// A new bundle in `dir`, a directory not there yet or empty, as `deck_create` makes one
    /// (PLAN 2.13): a theme, its fonts, and one state with nothing on it, titled `--title`.
    New {
        dir: PathBuf,
        /// A theme that ships (`dusk`, `daybreak`, `ember`), which comes with its fonts; or a
        /// theme file, whose fonts are beside it or above it.
        #[arg(long, default_value = "dusk")]
        theme: String,
        #[arg(long, default_value = "Untitled")]
        title: String,
    },
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
        /// Each visible node's box at rest, canvas units: what a pointer selects and moves
        /// (ADR-0013). Reads the bundle's fonts, as `render` does.
        #[arg(long)]
        boxes: bool,
        /// The nodes that draw at `X,Y` (canvas units) at rest, topmost first, each with the
        /// containers it sits in.
        #[arg(long, value_name = "X,Y", value_parser = point)]
        at: Option<[f32; 2]>,
        /// One of the deck's formats (`9:16`) to inspect it in, laid out with its template set.
        #[arg(long)]
        format: Option<String>,
        /// Where NODE may go in the state (ADR-0013): what holds it, its cell, and the
        /// tracks, slots, or order a drag snaps it to. Needs `--state`.
        #[arg(long, value_name = "NODE", requires = "state")]
        targets: Option<String>,
        /// How the box `--to` snaps on NODE's targets (move, resize, slot, free, order), and
        /// the patch that puts NODE there.
        #[arg(long, value_name = "HOW", requires_all = ["targets", "to"])]
        snap: Option<SnapMode>,
        /// The box a drag left, `X,Y,W,H` in canvas units: NODE's cell, moved or resized.
        #[arg(long, value_name = "X,Y,W,H", value_parser = rect, requires = "snap")]
        to: Option<[f32; 4]>,
        /// Keep the snapped or arranged patch to the state: what it changes goes into the
        /// state's own props, wherever it lives now (`place`'s and `choose`'s `fork`).
        #[arg(long)]
        fork: bool,
        /// Several nodes in the state, children of one container, arranged at once (PLAN
        /// 2.42): with `--align`, `--spread`, `--order`, or `--by`, where each lands and the
        /// patch that puts them there. Needs `--state`.
        #[arg(long, value_name = "NODES", value_delimiter = ',', requires = "state")]
        arrange: Option<Vec<String>>,
        /// The edge, or the middle, the nodes `--arrange` names all take: left, center,
        /// right, top, middle, or bottom; on a grid, snapped to its tracks.
        #[arg(long, value_name = "EDGE", requires = "arrange")]
        align: Option<Align>,
        /// Spread the nodes `--arrange` names so the gaps between them are equal, the first
        /// and the last staying: across or down.
        #[arg(long, value_name = "WAY", requires = "arrange")]
        spread: Option<Spread>,
        /// Order the nodes `--arrange` names among their container's children, by `z`:
        /// forward or backward past the next each overlaps, or to the front or the back.
        #[arg(long, value_name = "HOW", requires = "arrange")]
        order: Option<Order>,
        /// The node `--arrange` names, listed just before NODE, as `--layers` lists them (PLAN
        /// 2.50): painted just over it, or, in a stack, laid out just before it. Held by
        /// another container, or by none, the node goes there with it, placed as that one
        /// places what it holds.
        #[arg(long, value_name = "NODE", requires = "arrange")]
        before: Option<String>,
        /// The node `--arrange` names, listed just after NODE: painted just under it, or, in a
        /// stack, laid out just after it; into what holds it, as with `--before`.
        #[arg(long, value_name = "NODE", requires = "arrange")]
        after: Option<String>,
        /// The node `--arrange` names, into the container NODE, listed first among what it
        /// holds, placed as it places what it holds (PLAN 2.50).
        #[arg(long, value_name = "NODE", requires = "arrange")]
        into: Option<String>,
        /// Move the nodes `--arrange` names together `DX,DY` canvas units: the first snapped
        /// as a drag of it snaps, the rest as far as it went.
        #[arg(long, value_name = "DX,DY", value_parser = point, requires = "arrange", allow_hyphen_values = true)]
        by: Option<[f32; 2]>,
        /// With `--by`: off the grid, each to whole canvas units (a `rect`), as Shift drags.
        #[arg(long, requires = "by")]
        free: bool,
        /// What an inspector offers for NODE in the state (ADR-0013): each property it
        /// edits, the value shown and where it lives, and the theme's names for it. Needs
        /// `--state`.
        #[arg(long, value_name = "NODE", requires = "state")]
        choices: Option<String>,
        /// What an inspector offers for the state itself (PLAN 2.36): its layout, from the
        /// theme's layouts with a slot for each node placed in one; each key of its
        /// transition; its hold; and its notes, each with its value and where it lives, which
        /// is where `set_state` writes. Needs `--state`.
        #[arg(long, requires = "state")]
        state_choices: bool,
        /// What may be inserted in the state (PLAN 2.34): a text in each of the theme's
        /// roles, each kind of shape, each image in the bundle, a chart and a table of each
        /// data source, and each shader preset, as `add_node` adds each, with the box it
        /// takes at first. Needs `--state`.
        #[arg(long, requires = "state")]
        inserts: bool,
        /// The state's layers (PLAN 2.50): its nodes nested as their containers and groups hold
        /// them, topmost first, with those that leave in it and those another state of its
        /// slide shows, hidden. Needs `--state`.
        #[arg(long, requires = "state")]
        layers: bool,
        /// NODE's look in the state (PLAN 2.58): each property of its type's look and the
        /// value the state shows, as the editor's ⌥⌘C picks it up. Needs `--state`.
        #[arg(long, value_name = "NODE", requires = "state")]
        look: Option<String>,
        /// Put `--look`'s look on these nodes: a `choose` for each property a node shows
        /// otherwise, written where that node's own value lives (`scaena patch` takes them),
        /// and the nodes that look so already or take none of it, with why.
        #[arg(long, value_name = "NODES", value_delimiter = ',', requires = "look")]
        onto: Option<Vec<String>>,
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
        /// Start keeping history in `history/deck.loro` (SPEC §8): every change from here on
        /// is recorded, with who made it. A bundle that keeps it keeps it either way.
        #[arg(long)]
        history: bool,
    },
    /// Export a projection: pdf|png|svg|mp4|webm|prores|html|spine.
    Export {
        bundle: PathBuf,
        #[arg(long)]
        format: String,
        /// Where to write it: a file (pdf, video, html, spine), or a directory that gets
        /// an image per state (png, svg). The spine prints without it.
        #[arg(long)]
        out: Option<PathBuf>,
        /// The states to export, comma-separated, in that order (for html, the states it
        /// plays). Default: every state; for pdf, each slide once, at its last state, in
        /// spine order; for video, the whole timeline.
        #[arg(long, value_delimiter = ',')]
        states: Option<Vec<String>>,
        /// `WxH` pixels for png, svg, and video, in the canvas's aspect ratio. Default:
        /// the canvas's size.
        #[arg(long)]
        size: Option<String>,
        /// A video's frames a second. Default: 60.
        #[arg(long)]
        fps: Option<u32>,
        /// A video's sound track (any file ffmpeg reads), from the first frame: cut where
        /// the video ends, or carried on in silence until it does.
        #[arg(long)]
        audio: Option<PathBuf>,
        /// What paints a video's frames: `gpu` is vello on the GPU (needs a CLI built with
        /// `--features gpu`). Every other export paints with the CPU painter.
        #[arg(long, value_enum, default_value_t = PainterArg::Cpu)]
        painter: PainterArg,
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
    /// A data source's rows (PLAN 2.55, SPEC §3.10): without `--edits`, the source as a table,
    /// each cell as written; with them, cells set and rows added and taken away, all or none, in
    /// one write of its file that keeps every other byte, and what that changes in what
    /// `validate` and `lint` find. A value its column refuses stops them; edits that would make
    /// the deck invalid are refused.
    Data {
        bundle: PathBuf,
        /// The source's id: what a chart or a table names as `@id`.
        source: String,
        /// The edits: a JSON array of `{"op": "set", "row", "column", "value"}`, `{"op": "add",
        /// "row"?, "values"}`, and `{"op": "remove", "row"}`, rows from 0, or `-` for stdin
        /// (`docs/schema/mcp/data_edit.json`).
        #[arg(long)]
        edits: Option<PathBuf>,
        /// Say what would change, and write nothing.
        #[arg(long, requires = "edits")]
        dry_run: bool,
    },
    /// A bundle's versions (PLAN 2.60, SPEC §8): each change its history keeps, by author and
    /// time, oldest first. `--at` prints the deck as it was just after one; `--diff` says what
    /// changed from one to another, or to the deck as it is now; `--restore` makes one the deck
    /// again, with its data files as they were, one change, refused as a patch is where the deck
    /// would not validate in the bundle as it is.
    History {
        bundle: PathBuf,
        /// The deck as it was in this version: its number as listed, or its id.
        #[arg(long, value_name = "VERSION", conflicts_with_all = ["diff", "restore"])]
        at: Option<String>,
        /// With `--at`, the deck as `.scn`.
        #[arg(long, requires = "at")]
        scn: bool,
        /// What changed from one version to another, or, with one, to the deck as it is now.
        #[arg(long, value_name = "FROM[,TO]", value_delimiter = ',', conflicts_with = "restore")]
        diff: Option<Vec<String>>,
        /// Make this version the deck again: its number as listed, or its id.
        #[arg(long, value_name = "VERSION")]
        restore: Option<String>,
        /// Say what restoring would change, and write nothing.
        #[arg(long, requires = "restore")]
        dry_run: bool,
    },
    /// The bundle's images, fonts, and data (PLAN 2.59): each with what in the deck or its theme
    /// names it, and the nodes drawn from it in the states that show them so; a file nothing
    /// names says so. With `--remove`, those files taken out of the bundle, all or none: each
    /// must be one of its images, fonts, or data that nothing names.
    Files {
        bundle: PathBuf,
        /// Files to take out, by their paths in the bundle, comma-separated.
        #[arg(long, value_name = "PATHS", value_delimiter = ',')]
        remove: Option<Vec<String>>,
        /// Say what would be taken out, and write nothing.
        #[arg(long, requires = "remove")]
        dry_run: bool,
    },
    /// Find text across the deck's texts, in every state (PLAN 2.47): each text that holds it,
    /// once for each place the text is written, and the states that show it. With `--replace`,
    /// every match is replaced in one patch, a `replace_text` where each text lives.
    Find {
        bundle: PathBuf,
        /// The characters sought.
        text: String,
        /// Upper and lower case apart.
        #[arg(long)]
        case: bool,
        /// Whole words only.
        #[arg(long)]
        words: bool,
        /// Replace every match with this, as `patch` applies a patch.
        #[arg(long, value_name = "TEXT")]
        replace: Option<String>,
        /// With `--replace`: say what would change, and write nothing.
        #[arg(long, requires = "replace")]
        dry_run: bool,
    },
    /// Re-theme: point the deck at another theme, copied into the bundle, and say what
    /// changes in what `validate` and `lint` find. A theme that would leave the deck invalid
    /// is refused.
    Theme {
        bundle: PathBuf,
        /// The theme file to apply. A theme outside the bundle is copied to `themes/`.
        #[arg(long)]
        apply: PathBuf,
        /// Say what would change, and write nothing.
        #[arg(long)]
        dry_run: bool,
        /// Apply a theme that leaves the deck invalid. Without it, the deck keeps its theme,
        /// and the new one is copied in for a patch with the `retheme` op and the fixes.
        #[arg(long)]
        force: bool,
    },
    /// The web player and editor on a bundle's folder, on this machine only (PLAN 2.11): a
    /// `deck.scn` saved there compiles into `deck.json`, and the pages show each change.
    Serve {
        /// A bundle's folder: a directory with `deck.json` in it.
        bundle: PathBuf,
        /// The port on 127.0.0.1; 0 for any free one.
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
    /// Milliseconds into the state's cue: its transition, then its motions. Omitted: the state at rest.
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

impl From<PainterArg> for Painter {
    fn from(painter: PainterArg) -> Self {
        match painter {
            PainterArg::Cpu => Painter::Cpu,
            PainterArg::Gpu => Painter::Gpu,
        }
    }
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
        Cmd::New { dir, theme, title } => new(&dir, &theme, &title, cli.json),
        Cmd::Validate { bundle } => {
            let findings = scaena_ops::lint::validate(&bundle)?;
            report(&findings, cli.json);
            Ok(if findings.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) })
        }
        Cmd::Save { bundle, to, keep_fonts, history } => {
            let b = open(&bundle)?;
            let to = to.unwrap_or(bundle);
            let opts = SaveOptions { subset_fonts: !keep_fonts, now: now_rfc3339(), history };
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
        Cmd::Inspect {
            bundle,
            state,
            resolved,
            timeline,
            data,
            boxes,
            at,
            format,
            targets,
            snap,
            to,
            fork,
            arrange,
            align,
            spread,
            order,
            before,
            after,
            into,
            by,
            free,
            choices,
            state_choices,
            inserts,
            layers,
            look,
            onto,
        } => {
            let views = Views {
                resolved,
                timeline,
                data,
                boxes,
                at,
                format,
                targets,
                snap,
                to,
                fork,
                arrange,
                align,
                spread,
                order,
                before,
                after,
                into,
                by,
                free,
                choices,
                state_choices,
                inserts,
                layers,
                look,
                onto,
            };
            inspect(&open(&bundle)?, state.as_deref(), views, cli.json)
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
        Cmd::Export { bundle, format, out, states, size, fps, audio, painter } => {
            let painter = painter.into();
            let req = scaena_ops::export::Request { format, states, out, size, fps, audio, painter };
            let exported = scaena_ops::export::export(&open(&bundle)?, &req)?;
            if cli.json {
                println!("{}", serde_json::to_string_pretty(&exported)?);
            } else if let Some(spine) = &exported.spine {
                println!("{}", serde_json::to_string_pretty(spine)?);
            } else {
                let out = exported.out.as_deref().unwrap_or_default();
                match (&exported.pages, &exported.files, exported.frames) {
                    (_, Some(files), _) if exported.format == "spine" => {
                        println!("wrote {out} and {} renders beside it", files.len())
                    }
                    (Some(states), None, _) if exported.format == "html" => {
                        let kb = exported.bytes.unwrap_or_default().div_ceil(1024);
                        println!("wrote {out} ({kb} KB, playing {} states: {})", states.len(), states.join(", "))
                    }
                    (Some(pages), None, _) => println!("wrote {out} ({} pages: {})", pages.len(), pages.join(", ")),
                    (_, Some(files), _) => println!("wrote {} {} images into {out}", files.len(), exported.format),
                    (_, _, Some(frames)) => println!(
                        "wrote {out} ({frames} frames at {} fps, {:.1} s{})",
                        exported.fps.unwrap_or_default(),
                        exported.duration_ms.unwrap_or_default() / 1000.0,
                        exported.adapter.as_deref().map(|a| format!(", painted on {a}")).unwrap_or_default()
                    ),
                    _ => println!("wrote {out}"),
                }
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
        Cmd::Data { bundle, source, edits, dry_run } => data(&bundle, &source, edits.as_deref(), dry_run, cli.json),
        Cmd::Files { bundle, remove, dry_run } => files(&bundle, remove.as_deref(), dry_run, cli.json),
        Cmd::History { bundle, at, scn, diff, restore, dry_run } => {
            let ask = scaena_ops::history::Ask { at, scn, compare: diff.unwrap_or_default(), restore, dry_run };
            history(&bundle, &ask, cli.json)
        }
        Cmd::Find { bundle, text, case, words, replace, dry_run } => {
            let query = scaena_core::patch::Query { find: text, case, words };
            find(&bundle, &query, replace.as_deref(), dry_run, cli.json)
        }
        Cmd::Theme { bundle, apply, dry_run, force } => theme_apply(&bundle, &apply, dry_run, force, cli.json),
        Cmd::Serve { bundle, port } => serve(&bundle, port, cli.json),
        Cmd::Mcp => {
            scaena_mcp::stdio().context("serving MCP on stdio")?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

/// `scaena serve` (PLAN 2.11, ADR-0012): the web player and the editor on a bundle's folder, on
/// this machine only, until it is stopped. What happens is said on stderr: each change, and a
/// `deck.scn` that does not compile, shown as `compile` shows it. Under `--json`, stdout holds
/// where it serves, once.
fn serve(bundle: &Path, port: u16, json: bool) -> Result<ExitCode> {
    use scaena_serve::{Note, ServeError};
    if !scaena_serve::pages_built() {
        let message = "this scaena was built without the web pages `serve` carries: build them with `just web`, then \
                       build scaena again";
        return Ok(fail(json, 3, message, Some("2.11")));
    }
    let shown = bundle.display().to_string();
    let started = |addr: std::net::SocketAddr| {
        let url = format!("http://localhost:{}/", addr.port());
        if json {
            let v = serde_json::json!({ "bundle": shown, "player": url, "editor": format!("{url}edit") });
            println!("{}", serde_json::to_string_pretty(&v).expect("JSON"));
        } else {
            eprintln!(
                "Serving {shown} on this machine only.\n  The player: {url}\n  The editor: {url}edit\nCtrl-C stops it."
            );
        }
    };
    let note = |note: Note| match note {
        Note::Compiled { ms, written: true } => {
            eprintln!("{} compiled into {} ({ms} ms)", scaena_serve::SOURCE, scaena_serve::DECK)
        }
        Note::Compiled { written: false, .. } => eprintln!("{} compiled: the deck is as it was", scaena_serve::SOURCE),
        Note::Failed(failed) => {
            for p in &failed.problems {
                let message = match &p.file {
                    Some(file) => format!("{file} {}: {}", p.path.as_deref().unwrap_or(""), p.message),
                    None => p.message.clone(),
                };
                let label = p.span.and(p.path.clone());
                let shown = diagnostic(
                    scaena_serve::SOURCE,
                    &failed.source,
                    p.code.as_deref(),
                    &message,
                    p.span,
                    label,
                    p.hint.as_deref(),
                );
                eprint!("{shown}");
            }
        }
        Note::Changed { paths, by: Some(_) } => eprintln!("saved from a page: {}", paths.join(", ")),
        Note::Changed { paths, by: None } => eprintln!("changed: {}", paths.join(", ")),
    };
    match scaena_serve::run(bundle, port, started, note) {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(e @ (ServeError::NotABundle(_) | ServeError::Bind { .. })) => Ok(fail(json, 2, &e.to_string(), None)),
        Err(e) => Err(e).context("serving"),
    }
}

/// `scaena compile` (PLAN 1.5): `.scn` → `deck.json`. A source that does not parse exits 2;
/// a deck that does not validate exits 1, each finding shown at the source it came from.
/// Either way nothing is written: `deck.json` is the truth, and only a valid deck replaces it.
fn compile(input: &Path, out: Option<&Path>, json: bool) -> Result<ExitCode> {
    let source = std::fs::read_to_string(input).with_context(|| format!("reading {}", input.display()))?;
    let name = input.display().to_string();
    // The bundle the deck is checked in: the one it is written to, or the source's.
    let root = out.unwrap_or(input).parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
    let compiled = match scaena_ops::compile::compile(&source, &scaena_store::Files::Dir(root.to_path_buf())) {
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
    let findings = &compiled.findings;
    if !findings.is_empty() {
        // A finding about the deck is about the source that wrote that part of it; one
        // about another file (the theme) is about that file.
        let span = |f: &Finding| compiled.span(f);
        if json {
            let located: Vec<serde_json::Value> = findings
                .iter()
                .map(|f| {
                    let mut v = serde_json::to_value(f).expect("a finding is JSON");
                    if let Some((offset, _)) = span(f) {
                        let (line, col) = scaena_ops::compile::line_col(&source, offset);
                        v["line"] = serde_json::json!(line);
                        v["col"] = serde_json::json!(col);
                    }
                    v
                })
                .collect();
            let summary = serde_json::json!({ "out": null, "findings": located });
            println!("{}", serde_json::to_string_pretty(&summary)?);
        } else {
            for f in findings {
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
    let deck = scaena_core::document::Deck::from_json(&compiled.json.to_string()).context("the compiled deck")?;
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

/// `scaena theme --apply` (PLAN 1.6): point the deck at another theme, and report the
/// delta in what `validate` and `lint` find: what the new theme breaks, and what it fixes.
/// A theme change is a pure re-render (SPEC §2.5), so the deck itself is not touched beyond
/// its `theme`. A theme that would leave the deck invalid is refused unless `force`, and
/// exits 1. Findings after it, if any are errors, exit 1.
/// `scaena new` (PLAN 2.13): a bundle from a theme and its fonts, as `deck_create` makes one.
/// Findings that are errors exit 1, and the bundle is not made.
fn new(dir: &Path, theme: &str, title: &str, json: bool) -> Result<ExitCode> {
    let req = scaena_ops::create::Create { theme: theme.into(), title: Some(title.into()), ..Default::default() };
    let made = scaena_ops::create::create(dir, &req)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&made)?);
    } else if made.created {
        println!("made {}: {}", dir.display(), made.files.join(", "));
        let d = dir.display();
        println!("next: scaena decompile {d} -o {d}/deck.scn, then scaena serve {d}");
    } else {
        println!("not made: {} would not validate", dir.display());
        report(&made.findings, false);
    }
    Ok(if made.errors > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

fn theme_apply(bundle: &Path, theme: &Path, dry_run: bool, force: bool, json: bool) -> Result<ExitCode> {
    let t = scaena_ops::theme::theme_apply(&open(bundle)?, theme, dry_run, force)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&t)?);
    } else if t.refused {
        let copied = if dry_run { "" } else { ", and the theme is copied in for `patch`'s `retheme` op" };
        println!("refused: {} would leave the deck invalid; the deck keeps its theme{copied}", t.theme);
    } else {
        let verb = if dry_run { "would apply" } else { "applied" };
        println!("{verb} {} (was {})", t.theme, t.was.as_deref().unwrap_or("no theme"));
        for m in &t.mapped {
            println!("  {m}");
        }
        for l in &t.listed {
            println!("  fonts lists {l}");
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

/// `scaena data` (PLAN 2.55, ADR-0014): a data source as a table, each cell as written and the
/// cells its column does not read; or edited in place, reported as `patch` reports a patch. An
/// edit that does not apply exits 2 with its index, as a patch's op does; edits that would make
/// the deck invalid exit 1, refused. Either way, and under `--dry-run`, nothing is written.
fn data(bundle: &Path, source: &str, edits: Option<&Path>, dry_run: bool, json: bool) -> Result<ExitCode> {
    let b = open(bundle)?;
    let edits = match edits {
        None => Vec::new(),
        Some(path) => {
            let text = if path == Path::new("-") {
                let mut text = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
                    .context("reading the edits from stdin")?;
                text
            } else {
                std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?
            };
            serde_json::from_str(&text).with_context(|| {
                format!("{} is not a JSON array of edits (docs/schema/mcp/data_edit.json)", path.display())
            })?
        }
    };
    let req = scaena_ops::data::DataEdit { source: source.to_string(), edits };
    let d = scaena_ops::data::data_edit(&b, &req, dry_run)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&d)?);
        return Ok(if d.refused || d.errors > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS });
    }
    let what = d.file.as_deref().map_or_else(|| "the deck's inline rows".to_string(), String::from);
    let n = req.edits.len();
    let edits = if n == 1 { "1 edit".to_string() } else { format!("{n} edits") };
    match (n, d.refused, dry_run) {
        (0, ..) => {
            let rows = d.sheet.rows.len();
            println!("@{source}  {what}  {}", if rows == 1 { "1 row".to_string() } else { format!("{rows} rows") });
            print_sheet(&d.sheet);
            return Ok(ExitCode::SUCCESS);
        }
        (_, true, _) => println!("refused: the edits would make the deck invalid; nothing was written"),
        (_, false, true) => println!("would make {edits} in {what}"),
        (_, false, false) => println!("made {edits} in {what}"),
    }
    print_delta(&d.added, &d.removed);
    Ok(if d.refused || d.errors > 0 { ExitCode::from(1) } else { ExitCode::SUCCESS })
}

/// `scaena files` (PLAN 2.59): the bundle's images, fonts, and data, and what uses each; or,
/// with `remove`, those taken out, all or none, which exits 1 where one cannot be.
fn files(bundle: &Path, remove: Option<&[String]>, dry_run: bool, json: bool) -> Result<ExitCode> {
    use scaena_core::files::Kind;
    let b = open(bundle)?;
    if let Some(paths) = remove {
        let r = scaena_ops::files::remove(&b, paths, dry_run)?;
        let code = if r.refused.is_empty() { ExitCode::SUCCESS } else { ExitCode::from(1) };
        if json {
            println!("{}", serde_json::to_string_pretty(&r)?);
            return Ok(code);
        }
        for refusal in &r.refused {
            println!("refused: {}: {}", refusal.path, refusal.why);
        }
        match (r.refused.is_empty(), dry_run) {
            (false, _) => println!("nothing was taken out"),
            (true, true) => println!("would take out {}", r.removed.join(", ")),
            (true, false) => println!("took out {}", r.removed.join(", ")),
        }
        return Ok(code);
    }
    let listed = scaena_ops::files::Listed { files: scaena_ops::files::files(&b)? };
    if json {
        println!("{}", serde_json::to_string_pretty(&listed)?);
        return Ok(ExitCode::SUCCESS);
    }
    for (kind, heading) in [(Kind::Image, "images"), (Kind::Font, "fonts"), (Kind::Data, "data")] {
        let of: Vec<_> = listed.files.iter().filter(|f| f.kind == kind).collect();
        if of.is_empty() {
            continue;
        }
        println!("{heading}");
        for f in of {
            println!("  {}  {}", f.path, size(f.bytes));
            if f.named.is_empty() {
                println!("    nothing names it: --remove {} takes it out", f.path);
                continue;
            }
            let names: Vec<String> = f.named.iter().map(scaena_ops::files::said).collect();
            println!("    named by {}", names.join(", "));
            for used in &f.used {
                println!("    {} in {}", used.node, used.states.join(", "));
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn history(bundle: &Path, ask: &scaena_ops::history::Ask, json: bool) -> Result<ExitCode> {
    use scaena_ops::history::StateChange;
    use scaena_ops::inspect::Change;
    let h = scaena_ops::history::history(&open(bundle)?, ask)?;
    let refused = h.restored.as_ref().is_some_and(|r| r.refused || r.errors > 0);
    let code = if refused { ExitCode::from(1) } else { ExitCode::SUCCESS };
    if json {
        println!("{}", serde_json::to_string_pretty(&h)?);
        return Ok(code);
    }
    let shown = |v: &serde_json::Value| {
        let text = v.to_string();
        if text.chars().count() > 60 { format!("{}…", text.chars().take(59).collect::<String>()) } else { text }
    };
    for v in h.versions.iter().flatten() {
        let (at, by) = (v.at.as_deref().unwrap_or("-"), v.author.as_deref().unwrap_or("-"));
        println!("{:>4}  {at}  {by:<14}  {}  ({})", v.n, v.message.as_deref().unwrap_or(""), v.id);
    }
    if let Some(seen) = &h.seen {
        match (&seen.scn, &seen.deck) {
            (Some(scn), _) => print!("{scn}"),
            (_, Some(deck)) => println!("{}", serde_json::to_string_pretty(deck)?),
            _ => {}
        }
    }
    if let Some(c) = &h.compared {
        let later = c.to.as_ref().map_or_else(|| "the deck as it is now".to_string(), |v| format!("version {}", v.n));
        println!("from version {} to {later}", c.from.n);
        if c.states.is_empty() && c.deck.is_empty() && c.files.is_empty() {
            println!("  nothing changed");
        }
        for (id, change) in &c.states {
            match change {
                StateChange::Added(_) => println!("  state {id}: added"),
                StateChange::Removed(_) => println!("  state {id}: removed"),
                StateChange::Changed(changed) => {
                    println!("  state {id}:");
                    for (field, value) in &changed.fields {
                        println!("    {field}: {}", shown(value));
                    }
                    for (node, change) in &changed.nodes {
                        match change {
                            Change::Enter(_) => println!("    {node} enters"),
                            Change::Exit(_) => println!("    {node} exits"),
                            Change::Change(props) => {
                                let props: Vec<String> =
                                    props.iter().map(|(k, v)| format!("{k} {}", shown(v))).collect();
                                println!("    {node}: {}", props.join(", "));
                            }
                        }
                    }
                }
            }
        }
        for (field, value) in &c.deck {
            println!("  deck {field}: {}", shown(value));
        }
        for file in &c.files {
            println!("  {file} changed");
        }
    }
    if let Some(r) = &h.restored {
        let n = r.version.n;
        match (r.refused, ask.dry_run) {
            (true, _) => {
                println!("refused: version {n} would make the deck invalid in the bundle as it is; nothing was written")
            }
            (false, true) => println!("would make version {n} the deck again"),
            (false, false) => println!("made version {n} the deck again"),
        }
        for file in &r.files {
            println!("  {file} as it was then");
        }
        print_delta(&r.added, &r.removed);
    }
    Ok(code)
}

/// A size in bytes, as a person reads it.
fn size(bytes: u64) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.0} KB", b as f64 / 1024.0),
        b => format!("{:.1} MB", b as f64 / (1024.0 * 1024.0)),
    }
}

/// A data source's sheet as a table: each column's name and type over its cells, each row by
/// its index, then the cells a column does not read.
fn print_sheet(sheet: &scaena_ops::data::Sheet) {
    let index = |i: usize| i.to_string();
    let mut widths: Vec<usize> =
        sheet.columns.iter().map(|c| c.name.chars().count().max(c.kind.name().len())).collect();
    for row in &sheet.rows {
        for (w, cell) in widths.iter_mut().zip(row) {
            *w = (*w).max(cell.chars().count());
        }
    }
    let first = index(sheet.rows.len().saturating_sub(1)).len().max(3);
    let line = |lead: &str, cells: Vec<&str>| {
        let cells: Vec<String> = cells.iter().zip(&widths).map(|(c, w)| format!("{c:<w$}")).collect();
        println!("{lead:<first$}  {}", cells.join("  ").trim_end());
    };
    line("row", sheet.columns.iter().map(|c| c.name.as_str()).collect());
    line("", sheet.columns.iter().map(|c| c.kind.name()).collect());
    for (i, row) in sheet.rows.iter().enumerate() {
        line(&index(i), row.iter().map(String::as_str).collect());
    }
    for p in &sheet.problems {
        println!("! row {}, {}: {}", p.row, p.column, p.why);
    }
}

/// `scaena find` (PLAN 2.47): each text the query matches, once for each place it is written,
/// with the states that show it and its matches; with `--replace`, every match replaced in
/// one patch, reported and written as `patch` reports and writes one, and exiting as it does.
fn find(
    bundle: &Path,
    query: &scaena_core::patch::Query,
    replace: Option<&str>,
    dry_run: bool,
    json: bool,
) -> Result<ExitCode> {
    let b = open(bundle)?;
    let searched = scaena_ops::find::search(&b, query, replace, dry_run)?;
    if json {
        println!("{}", serde_json::to_string_pretty(&searched)?);
    } else {
        let texts = searched.found.len();
        println!(
            "{} {} in {texts} {}",
            searched.matches,
            if searched.matches == 1 { "match" } else { "matches" },
            if texts == 1 { "text" } else { "texts" }
        );
        for f in &searched.found {
            let quoted: Vec<String> = f.matches.iter().map(|&[from, to]| format!("{from}..{to}")).collect();
            println!("  {} in {} ({}): {:?} at {}", f.node, f.states.join(", "), f.lives, f.text, quoted.join(", "));
        }
        if let Some(p) = &searched.replaced {
            match (p.refused, dry_run) {
                (true, _) => println!("refused: replacing them would make the deck invalid; nothing was written"),
                (false, true) => println!("would replace them, {} ops as JSON Patch", p.patch.len()),
                (false, false) => println!("replaced them, {} ops as JSON Patch", p.patch.len()),
            }
            print_delta(&p.added, &p.removed);
        }
    }
    Ok(match &searched.replaced {
        Some(p) if p.errors > 0 => ExitCode::from(1),
        _ => ExitCode::SUCCESS,
    })
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

/// A point on the canvas, `X,Y` in canvas units.
fn rect(s: &str) -> Result<[f32; 4], String> {
    let parts: Vec<Option<f32>> = s.split(',').map(|v| v.trim().parse::<f32>().ok()).collect();
    match parts[..] {
        [Some(x), Some(y), Some(w), Some(h)] if [x, y, w, h].iter().all(|v| v.is_finite()) => Ok([x, y, w, h]),
        _ => Err(format!("`{s}`: expected X,Y,W,H in canvas units, as `96,96,600,200`")),
    }
}

fn point(s: &str) -> Result<[f32; 2], String> {
    let parsed =
        s.split_once(',').and_then(|(x, y)| Some([x.trim().parse::<f32>().ok()?, y.trim().parse::<f32>().ok()?]));
    match parsed {
        Some(p) if p.iter().all(|v| v.is_finite()) => Ok(p),
        _ => Err(format!("`{s}`: expected X,Y in canvas units, as `960,540`")),
    }
}

/// `scaena inspect`: each state's snapshot, tracking applied (SPEC §2.2). `--resolved`
/// takes it through the theme cascade (PLAN 1.6); `--timeline` adds its cue and `--data`
/// the rows its charts and tables read (PLAN 1.14); `--boxes` each node's box at rest, and
/// `--at` what draws at a point (ADR-0013).
fn inspect(b: &Bundle, state: Option<&str>, views: Views, json: bool) -> Result<ExitCode> {
    let at = views.at;
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
        if let Some(boxes) = &i.boxes {
            println!("  boxes at rest, canvas units:");
            for (id, b) in boxes {
                let [x, y, w, h] = b.rect.map(|v| num(f64::from(v)));
                let parent = b.parent.as_ref().map(|p| format!(" in {p}")).unwrap_or_default();
                let holds = if b.draws { "" } else { ", holds others" };
                println!("    {id:<16} x {x}, y {y}, {w} × {h}{parent}{holds}");
            }
        }
        if let (Some(hits), Some([x, y])) = (&i.hits, at) {
            let [x, y] = [x, y].map(|v| num(f64::from(v)));
            match hits.is_empty() {
                true => println!("  at {x},{y}: nothing draws there"),
                false => {
                    println!("  at {x},{y}, topmost first:");
                    for h in hits {
                        let within = match h.containers.is_empty() {
                            true => String::new(),
                            false => format!(" (in {})", h.containers.join(" in ")),
                        };
                        let caret = h.offset.map(|o| format!(", a caret after {o} characters")).unwrap_or_default();
                        println!("    {}{within}{caret}", h.node);
                    }
                }
            }
        }
        if let Some(t) = &i.targets {
            print_targets(t);
        }
        if let Some(snapped) = &i.snapped {
            let [x, y, w, h] = snapped.cell.map(|v| num(f64::from(v)));
            println!("  lands at x {x}, y {y}, {w} × {h}");
            match snapped.patch.is_empty() {
                true => println!("    where it is: nothing to patch"),
                false => println!("    patch: {}", serde_json::to_string(&snapped.patch)?),
            }
        }
        if let Some(arranged) = &i.arranged {
            println!("  arranged:");
            for landed in &arranged.landed {
                let [x, y, w, h] = landed.cell.map(|v| num(f64::from(v)));
                println!("    {} lands at x {x}, y {y}, {w} × {h}", landed.node);
            }
            match arranged.patch.is_empty() {
                true => println!("    where they are: nothing to patch"),
                false => println!("    patch: {}", serde_json::to_string(&arranged.patch)?),
            }
        }
        if let Some(c) = &i.choices {
            print_choices(c);
        }
        if let Some(c) = &i.state_choices {
            print_state_choices(c);
        }
        if let Some(offered) = &i.inserts {
            print_inserts(offered);
        }
        if let Some(layers) = &i.layers {
            println!("  layers, topmost first:");
            print_layers(layers, 2);
        }
        if let Some(look) = &i.look {
            println!("  the look of {}:", look.node);
            for part in &look.props {
                let value = part.value.as_ref().map_or_else(|| "the theme's".to_string(), |v| v.to_string());
                println!("    {}: {value}", part.prop);
            }
        }
        if let Some(put) = &i.put {
            match put.patch.is_empty() {
                true => println!("  put down: nothing to patch"),
                false => println!("  put on {}: {}", put.took.join(", "), serde_json::to_string(&put.patch)?),
            }
            if !put.same.is_empty() {
                println!("    {} look so already", put.same.join(", "));
            }
            for refused in &put.refused {
                println!("    {}: {}", refused.node, refused.why);
            }
        }
    }
    Ok(ExitCode::SUCCESS)
}

/// `inspect --layers`, for a person: each node as a layers panel lists it, under what holds it,
/// and those the state does not show marked hidden.
fn print_layers(layers: &[scaena_core::layers::Layer], depth: usize) {
    for l in layers {
        let kind = serde_json::to_value(l.kind).ok().and_then(|v| v.as_str().map(str::to_string)).unwrap_or_default();
        let hidden = if l.shown { "" } else { ", hidden" };
        println!("{:width$}{} ({kind}{hidden})", "", l.node, width = depth * 2);
        print_layers(&l.children, depth + 1);
    }
}

/// `inspect --inserts`, for a person: what may be inserted, each with the node it adds and
/// the box it takes at first.
fn print_inserts(offered: &[scaena_core::inserts::Insert]) {
    use scaena_core::inserts::Start;
    println!("  inserts:");
    for insert in offered {
        let start = match &insert.start {
            Start::Box { w, h } => format!("{:.0}% × {:.0}% of the canvas, at the pointer", w * 100.0, h * 100.0),
            Start::Slot(slot) => format!("the {slot} slot, under the rest"),
        };
        println!("    {} as {}…: {}; {start}", insert.label, insert.id, insert.node);
    }
}

/// `inspect --choices`, for a person: each property an inspector edits, the value the state
/// shows and where it lives, and what it takes.
fn print_choices(c: &scaena_core::choices::Choices) {
    use scaena_core::choices::Where;
    let kind = serde_json::to_value(c.node_type).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default();
    println!("  choices for {} ({kind}):", c.node);
    for f in &c.fields {
        let shown = match (&f.value, &f.lives) {
            (Some(v), Some(lives)) => {
                let v = v.as_str().map_or_else(|| v.to_string(), String::from);
                let at = match lives {
                    Where::Overrides => "the deck's overrides, an override".to_string(),
                    Where::State(state) => format!("{state}'s delta"),
                    Where::Node => "the node".to_string(),
                };
                let written = if f.literal && *lives != Where::Overrides { ", written out (W300)" } else { "" };
                format!("{v}, in {at}{written}")
            }
            _ => "the theme's".to_string(),
        };
        println!("    {:<14} {shown} · {}", f.prop, takes(&f.takes));
    }
}

/// What a property takes, for a person.
fn takes(takes: &scaena_core::choices::Takes) -> String {
    use scaena_core::choices::Takes;
    match takes {
        Takes::Name { names, overrides, .. } => {
            const SHOWN: usize = 8;
            let more = names.len().saturating_sub(SHOWN);
            let mut said = names.iter().take(SHOWN).cloned().collect::<Vec<_>>().join(", ");
            if more > 0 {
                said += &format!(", … ({} in all)", names.len());
            }
            if *overrides {
                said += ", or a value written out, an override";
            }
            said
        }
        Takes::Word { words } => words.join(", "),
        Takes::Number { min, above, max, whole, overrides } => {
            let what = if *whole { "a whole number" } else { "a number" };
            let from = match (min, above) {
                (Some(min), _) => format!(" from {}", num(*min)),
                (_, Some(above)) => format!(" above {}", num(*above)),
                _ => String::new(),
            };
            let to = max.map(|m| format!(" to {}", num(m))).unwrap_or_default();
            let over = if *overrides { ", an override" } else { "" };
            format!("{what}{from}{to}{over}")
        }
        Takes::Flag => "yes or no".to_string(),
        Takes::Text => "words".to_string(),
        Takes::Fractions { names } => format!("fractions of the image, {}", names.join(", ")),
    }
}

/// `inspect --state-choices`, for a person: the state's layout, transition, hold, and notes,
/// each with its value and where it lives, and what it takes.
fn print_state_choices(c: &scaena_core::choices::StateChoices) {
    use scaena_core::choices::Where;
    println!("  choices for state {}:", c.state);
    for f in &c.fields {
        let shown = match (&f.value, &f.lives) {
            (Some(v), Some(Where::State(state))) => {
                let v = v.as_str().map_or_else(|| v.to_string(), String::from);
                let v =
                    if v.chars().count() > 40 { format!("{}…", v.chars().take(40).collect::<String>()) } else { v };
                format!("{v}, set in {state}")
            }
            _ if f.prop == "layout" => "none".to_string(),
            _ => "not set".to_string(),
        };
        println!("    {:<20} {shown} · {}", f.prop, takes(&f.takes));
    }
}

/// `inspect --targets`, for a person: what holds the node, its cell, and what a drag snaps
/// it to.
fn print_targets(t: &scaena_ops::inspect::Targets) {
    let rect = |r: [f32; 4]| {
        let [x, y, w, h] = r.map(|v| num(f64::from(v)));
        format!("x {x}, y {y}, {w} × {h}")
    };
    let held = match (t.by.as_str(), &t.parent) {
        ("stack", Some(p)) => format!("in stack {p}, by order"),
        ("cells", Some(p)) => format!("in grid {p}, by its cells or areas"),
        ("frame", Some(p)) => format!("in frame {p}, by a rect from its padding edge"),
        _ => "on the theme's grid, by cells, a slot, or a rect".to_string(),
    };
    println!("  targets: {held}");
    println!("    cell: {}", rect(t.cell));
    if !t.columns.is_empty() {
        let first = |tracks: &[[f32; 2]]| tracks.first().map(|r| num(f64::from(r[1] - r[0]))).unwrap_or_default();
        println!(
            "    {} columns ({} wide first), {} rows ({} tall first)",
            t.columns.len(),
            first(&t.columns),
            t.rows.len(),
            first(&t.rows)
        );
    }
    for (name, r) in &t.slots {
        println!("    {name:<14} {}", rect(*r));
    }
    if !t.flow.is_empty() {
        println!("    order: {}", t.flow.join(", "));
    }
    let ways: Vec<&str> = t.snaps.iter().map(|m| m.name()).collect();
    println!("    snaps by: {}", ways.join(", "));
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
    let req =
        scaena_ops::render::Request { state: state.clone(), t, format: format.clone(), size, painter: painter.into() };
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

/// The bundle at `path`, edited by `$SCAENA_AUTHOR` (`user` without it): its history
/// records what this command changes as theirs (SPEC §8.2).
fn open(path: &Path) -> Result<Bundle> {
    let mut b = scaena_ops::open(path)?;
    if let Some(author) = std::env::var("SCAENA_AUTHOR").ok().filter(|a| !a.trim().is_empty()) {
        b.author = author;
    }
    Ok(b)
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
