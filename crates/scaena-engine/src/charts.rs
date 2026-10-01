//! Charts → marks (SPEC §3.7). A chart spec never stores pixels; it compiles to
//! marks with data keys so states can morph mark-by-mark. Phase 0 task 0.10 does
//! bar → line for the spike; Phase 1 task 1.9 does the full kind list.

#[derive(Debug, Clone, PartialEq)]
pub enum MarkShape {
    Rect([f64; 4]),
    Point([f64; 2], f64),
    Path(String),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Mark {
    /// Identity across states (from the chart's `key`).
    pub key: String,
    pub shape: MarkShape,
    pub color: String,
}
