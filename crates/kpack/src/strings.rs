//! The pack's one string table. Everything else refers to text by [`StrId`].

use serde::{Deserialize, Serialize};

/// An index into the [`StringTable`].
pub type StrId = u32;

/// Every string in the pack, each stored once.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct StringTable {
    strings: Vec<String>,
}

impl StringTable {
    /// A table holding `strings`, where string `i` has id `i`. The builder
    /// is responsible for not storing a string twice.
    pub fn from_vec(strings: Vec<String>) -> StringTable {
        StringTable { strings }
    }

    /// The string with this id.
    ///
    /// # Panics
    /// If `id` is not in the table, which means the pack is corrupt.
    pub fn get(&self, id: StrId) -> &str {
        &self.strings[id as usize]
    }

    pub fn len(&self) -> usize {
        self.strings.len()
    }

    pub fn is_empty(&self) -> bool {
        self.strings.is_empty()
    }
}
