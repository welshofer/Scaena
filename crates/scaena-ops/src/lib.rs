//! # scaena-ops
//!
//! The operations every client exposes (SPEC §7; ADR-0003, ADR-0009). Each works over a
//! bundle: create it and attach data to it; read, validate, lint, patch, re-theme, inspect,
//! diff, render, and export it. Each returns a typed result. Serialized, a result is what
//! `scaena --json` prints and what an MCP tool returns (named, where the CLI prints a list
//! or a map, since a tool's result is an object), and its type generates the tool's output
//! schema (`docs/schema/mcp/`).
//!
//! The CLI and the MCP server stay thin: they parse what they are given, call these, and
//! print or return what comes back. Wall-clock timings are taken here, outside the render
//! path, which never reads a clock (SPEC §13).

pub mod create;
pub mod export;
pub mod inspect;
pub mod lint;
pub mod patch;
pub mod read;
pub mod render;
pub mod theme;

pub use scaena_store::Bundle;
use std::fmt::Display;
use std::path::Path;

/// Why an operation stopped: what it was given is not what it takes, or what it needs is
/// built by a later PLAN task (`plan`). The CLI exits 2 or 3 with it.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
#[error("{message}")]
pub struct OpsError {
    pub message: String,
    /// The PLAN task that builds what it stopped at.
    pub plan: Option<String>,
    /// The op of a patch it stopped at, from 0.
    pub op: Option<usize>,
}

impl OpsError {
    pub fn new(message: impl Into<String>) -> OpsError {
        OpsError { message: message.into(), plan: None, op: None }
    }

    /// Stopped at something PLAN task `plan` builds.
    pub fn not_built(message: impl Into<String>, plan: &str) -> OpsError {
        OpsError { message: message.into(), plan: Some(plan.into()), op: None }
    }

    /// This error, with what it was doing in front: `context: message`.
    pub fn context(mut self, context: impl Display) -> OpsError {
        self.message = format!("{context}: {}", self.message);
        self
    }
}

/// The PLAN task a message names (`… — PLAN 1.20`), if it names one.
pub fn plan_task(message: &str) -> Option<String> {
    let (_, rest) = message.split_once("PLAN ")?;
    let task: String = rest.chars().take_while(|c| c.is_ascii_digit() || matches!(c, '.' | 'x')).collect();
    let task = task.trim_end_matches('.');
    (!task.is_empty()).then(|| task.to_string())
}

impl From<scaena_engine::EngineError> for OpsError {
    fn from(e: scaena_engine::EngineError) -> OpsError {
        let plan = match &e {
            scaena_engine::EngineError::NotImplemented(m) => plan_task(m),
            _ => None,
        };
        OpsError { message: e.to_string(), plan, op: None }
    }
}

impl From<scaena_paint::PaintError> for OpsError {
    fn from(e: scaena_paint::PaintError) -> OpsError {
        let plan = match &e {
            scaena_paint::PaintError::NotImplemented(m) => plan_task(m),
            _ => None,
        };
        OpsError { message: e.to_string(), plan, op: None }
    }
}

macro_rules! from {
    ($($error:ty),*) => {$(
        impl From<$error> for OpsError {
            fn from(e: $error) -> OpsError {
                OpsError::new(e.to_string())
            }
        }
    )*};
}
from!(scaena_store::StoreError, serde_json::Error, std::io::Error, scaena_core::tracking::TrackingError);

/// What an operation was doing when it stopped, in front of why, as `anyhow`'s context
/// reads in the CLI.
pub trait Context<T> {
    fn context(self, context: impl Display) -> Result<T, OpsError>;
    fn with_context<C: Display>(self, context: impl FnOnce() -> C) -> Result<T, OpsError>;
}

impl<T, E: Into<OpsError>> Context<T> for Result<T, E> {
    fn context(self, context: impl Display) -> Result<T, OpsError> {
        self.map_err(|e| e.into().context(context))
    }

    fn with_context<C: Display>(self, context: impl FnOnce() -> C) -> Result<T, OpsError> {
        self.map_err(|e| e.into().context(context()))
    }
}

impl<T> Context<T> for Option<T> {
    fn context(self, context: impl Display) -> Result<T, OpsError> {
        self.ok_or_else(|| OpsError::new(context.to_string()))
    }

    fn with_context<C: Display>(self, context: impl FnOnce() -> C) -> Result<T, OpsError> {
        self.ok_or_else(|| OpsError::new(context().to_string()))
    }
}

/// A bundle directory, a `.scaena` zip, or a bare deck file, opened.
pub fn open(path: &Path) -> Result<Bundle, OpsError> {
    Bundle::open(path).with_context(|| format!("opening {}", path.display()))
}

/// The theme a bundle's deck names, read for the engine.
pub fn theme(b: &Bundle) -> Result<scaena_engine::theme::Theme, OpsError> {
    let text = b.theme_json.as_deref().context("the deck names no theme")?;
    Ok(scaena_engine::theme::Theme::from_json(text)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_names_its_plan_task() {
        assert_eq!(plan_task("PDF export — PLAN 1.20").as_deref(), Some("1.20"));
        assert_eq!(plan_task("see docs/PLAN.md task 2.x.").as_deref(), None);
        assert_eq!(plan_task("the dev server — PLAN 2.x.").as_deref(), Some("2.x"));
        assert_eq!(plan_task("no task here"), None);
    }

    #[test]
    fn context_goes_in_front() {
        let e: Result<(), OpsError> = Err(OpsError::not_built("not yet", "1.20"));
        let e = e.context("exporting").unwrap_err();
        assert_eq!((e.message.as_str(), e.plan.as_deref()), ("exporting: not yet", Some("1.20")));
    }
}
