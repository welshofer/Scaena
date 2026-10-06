//! The bundle's files (PLAN 2.59): its images, fonts, and data, each with what in the deck
//! names it and the nodes drawn from it in the states that show them so
//! (`scaena_core::files`); and those nothing names, taken out on request, as `scaena files
//! --remove` takes them.

use crate::{Bundle, OpsError};
use scaena_core::files::{BundleFile, Named};
use schemars::JsonSchema;
use serde::Serialize;

/// The bundle's images, fonts, and data, and what uses each: what `scaena files` lists.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Listed {
    pub files: Vec<BundleFile>,
}

/// The bundle's images, fonts, and data, in that order and each by its path, with what in the
/// deck and its theme names each, and the nodes drawn from it.
pub fn files(b: &Bundle) -> Result<Vec<BundleFile>, OpsError> {
    let theme = crate::theme(b)?;
    let mut held = Vec::new();
    for path in b.files.list().map_err(|e| OpsError::new(e.to_string()))? {
        let size = b.files.size(&path).map_err(|e| OpsError::new(e.to_string()))?;
        held.push((path, size));
    }
    scaena_core::files::files(&b.deck, &theme, &held).map_err(OpsError::new)
}

/// What taking files out of a bundle did, or would do.
#[derive(Debug, Clone, Serialize, JsonSchema)]
pub struct Removed {
    /// The files taken out: all those asked, or none.
    pub removed: Vec<String>,
    /// Those that cannot be, each with why: nothing is taken out while one is.
    pub refused: Vec<Refusal>,
    /// Whether the bundle was written: not on a dry run, and not when one is refused.
    pub applied: bool,
}

/// A file a bundle keeps, and why.
#[derive(Debug, Clone, PartialEq, Serialize, JsonSchema)]
pub struct Refusal {
    pub path: String,
    pub why: String,
}

/// Why each of `paths` cannot be taken out of a bundle whose images, fonts, and data are
/// `listed`, of all it holds, `held`: one it does not hold, one that is none of them (the deck,
/// its theme, its manifest, a license), and one something names, with what.
pub fn refusals(listed: &[BundleFile], held: &[String], paths: &[String]) -> Vec<Refusal> {
    let mut out = Vec::new();
    for path in paths {
        let why = match listed.iter().find(|f| f.path == *path) {
            _ if !held.contains(path) => format!("the bundle holds no {path}"),
            None => format!("{path} is not one of the bundle's images, fonts, or data"),
            Some(f) if !f.named.is_empty() => {
                let names: Vec<String> = f.named.iter().map(said).collect();
                format!("{} names it: take that out of the deck first", names.join(", "))
            }
            Some(_) => continue,
        };
        out.push(Refusal { path: path.clone(), why });
    }
    out
}

/// What names a file, as the CLI and the editor say it.
pub fn said(named: &Named) -> String {
    match named {
        Named::Node { node } => format!("image {node}"),
        Named::Evidence { beat } => format!("beat {beat}'s evidence"),
        Named::Font { family, style: Some(style) } => format!("the deck's fonts ({family} {style})"),
        Named::Font { family, style: None } => format!("the deck's fonts ({family})"),
        Named::Theme { family } => format!("the theme's {family} family"),
        Named::Source { source } => format!("data source {source}"),
    }
}

/// `paths` taken out of the bundle, all or none: each must be one of its images, fonts, or
/// data that nothing names (PLAN 2.59). The deck draws and reads the same after. With
/// `dry_run`, nothing is written.
pub fn remove(b: &Bundle, paths: &[String], dry_run: bool) -> Result<Removed, OpsError> {
    let listed = files(b)?;
    let held = b.files.list().map_err(|e| OpsError::new(e.to_string()))?;
    let refused = refusals(&listed, &held, paths);
    let applied = refused.is_empty() && !dry_run;
    if applied {
        b.remove(paths).map_err(|e| OpsError::new(e.to_string()).context(format!("writing {}", b.root.display())))?;
    }
    let removed = if refused.is_empty() { paths.to_vec() } else { Vec::new() };
    Ok(Removed { removed, refused, applied })
}
