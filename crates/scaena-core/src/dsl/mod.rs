//! The `.scn` DSL (SPEC §4, PLAN 1.5): an authoring projection of the document.
//!
//! [`compile`] turns source into a [`Deck`]; [`decompile`] writes a deck as canonical
//! source. The round trip is semantic and exact: `compile(decompile(deck))` is `deck`,
//! byte for byte as canonical JSON, for every deck. Source formatting is not kept; the
//! decompiler's form is the canonical one, and decompiling is a fixed point.

mod lex;
mod parse;
mod print;

use crate::document::Deck;
use crate::model::check::Checker;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

pub use print::decompile;

/// A compile error: where, what, and the part of the deck it was compiling.
#[derive(Debug, Clone, PartialEq, thiserror::Error, serde::Serialize)]
#[error("{line}:{col}: {message}")]
pub struct DslError {
    /// 1-based.
    pub line: usize,
    /// 1-based, in characters.
    pub col: usize,
    /// Byte offset into the source, and the length of the source it is about.
    pub offset: usize,
    pub len: usize,
    pub message: String,
    /// A JSON pointer into the compiled deck, when the error is about part of it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pointer: Option<String>,
}

impl DslError {
    fn at(src: &str, offset: usize, message: &str) -> DslError {
        DslError::span(src, offset, 1, message)
    }

    fn span(src: &str, offset: usize, len: usize, message: &str) -> DslError {
        let offset = offset.min(src.len());
        let before = &src[..offset];
        let line = before.matches('\n').count() + 1;
        let col = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        DslError { line, col, offset, len, message: message.into(), pointer: None }
    }

    fn pointer(mut self, pointer: impl Into<String>) -> DslError {
        self.pointer = Some(pointer.into());
        self
    }
}

/// Where each part of a compiled deck came from in its source.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SourceMap(HashMap<String, (usize, usize)>);

impl SourceMap {
    /// The source that the part of the deck at `pointer` (a JSON pointer) came from, as a
    /// byte offset and length: the part's own, or its nearest ancestor's that has one.
    pub fn locate(&self, pointer: &str) -> Option<(usize, usize)> {
        let mut p = pointer;
        loop {
            if let Some(&span) = self.0.get(p) {
                return Some(span);
            }
            p = &p[..p.rfind('/')?];
        }
    }

    /// The source the part of the deck at `pointer` came from, if the source wrote that
    /// part itself: no ancestor's.
    pub fn exact(&self, pointer: &str) -> Option<(usize, usize)> {
        self.0.get(pointer).copied()
    }
}

/// `source` as a deck.
pub fn compile(source: &str) -> Result<Deck, DslError> {
    let (doc, map) = compile_json(source)?;
    Deck::deserialize(&doc).map_err(|e| {
        // The schema says why in the deck's terms, and where; serde says neither.
        match Checker::deck().check(&doc).into_iter().next() {
            Some(v) => {
                let (offset, len) = map.locate(&v.path).unwrap_or((0, 0));
                DslError::span(source, offset, len, &v.message).pointer(v.path)
            }
            None => DslError::at(source, 0, &format!("not a deck: {e}")),
        }
    })
}

/// `source` as the JSON of a deck, and where each part of it came from. The JSON is what
/// the source says, unchecked: `validate` says what is wrong with it, and the map says
/// where in the source.
pub fn compile_json(source: &str) -> Result<(Value, SourceMap), DslError> {
    let lines = lex::lex(source)?;
    parse::parse(source, &lines)
}
