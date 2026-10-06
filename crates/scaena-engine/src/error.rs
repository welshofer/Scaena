use thiserror::Error;

#[derive(Debug, Error)]
pub enum EngineError {
    #[error("not implemented yet: {0} (see docs/PLAN.md)")]
    NotImplemented(&'static str),
    #[error("tracking: {0}")]
    Tracking(#[from] scaena_core::tracking::TrackingError),
    #[error("unknown state `{0}`")]
    UnknownState(String),
    #[error("theme: {0}")]
    Theme(String),
    #[error("font: {0}")]
    Font(String),
    #[error("layout: {0}")]
    Layout(String),
    #[error("data: {0}")]
    Data(String),
    /// A language's hyphenation patterns that could not be had (`hyphen`).
    #[error("hyphenation: {0}")]
    Hyphenation(String),
}
