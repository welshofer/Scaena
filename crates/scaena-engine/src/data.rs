//! Data sources (SPEC §3.10) for the engine. The engine never reads files: the caller
//! hands it the bundle's data files as bytes ([`DataFiles`]), the way it hands over
//! fonts, and `scaena_core::data` reads and types them, as validation does.

use crate::EngineError;
use scaena_core::Deck;
use scaena_core::data::{DataError, SourceFiles};
use std::borrow::Cow;
use std::collections::BTreeMap;

pub use scaena_core::data::{ColumnType, Datum, Table};

/// A bundle's data files by bundle path (`data/q3.csv`), as the deck's sources name them.
#[derive(Debug, Clone, Default)]
pub struct DataFiles(BTreeMap<String, Vec<u8>>);

impl DataFiles {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, path: impl Into<String>, bytes: Vec<u8>) {
        self.0.insert(path.into(), bytes);
    }

    pub fn get(&self, path: &str) -> Option<&[u8]> {
        self.0.get(path).map(Vec::as_slice)
    }
}

impl SourceFiles for DataFiles {
    fn bytes(&self, path: &str) -> Option<Cow<'_, [u8]>> {
        self.get(path).map(Cow::Borrowed)
    }
}

/// Load the deck's data source `name` (a chart's `"@name"` without the `@`).
pub fn load(deck: &Deck, files: &DataFiles, name: &str) -> Result<Table, EngineError> {
    scaena_core::data::load(deck, files, name).map_err(|e| match e {
        DataError::Missing { name, path } => {
            EngineError::Data(format!("`@{name}`: `{path}` was not handed to the engine (DataFiles)"))
        }
        e => EngineError::Data(e.to_string()),
    })
}
