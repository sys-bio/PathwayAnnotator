//! Where a fact came from.

use serde::{Deserialize, Serialize};
use std::ops::{BitOr, BitOrAssign};

/// A set of sources, as bits. A fact found in two sources carries both.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Sources(pub u16);

impl Sources {
    pub const NONE: Sources = Sources(0);
    pub const BIGG: Sources = Sources(1 << 0);
    pub const BIOMODELS: Sources = Sources(1 << 1);
    pub const CHEBI: Sources = Sources(1 << 2);
    pub const RHEA: Sources = Sources(1 << 3);
    pub const METANETX: Sources = Sources(1 << 4);
    pub const REACTOME: Sources = Sources(1 << 5);
    pub const UNIPROT: Sources = Sources(1 << 6);
    pub const GO: Sources = Sources(1 << 7);
    /// Hand-maintained tables in this repository.
    pub const CURATED: Sources = Sources(1 << 15);

    const NAMES: [(Sources, &'static str); 9] = [
        (Sources::BIGG, "BiGG"),
        (Sources::BIOMODELS, "BioModels"),
        (Sources::CHEBI, "ChEBI"),
        (Sources::RHEA, "Rhea"),
        (Sources::METANETX, "MetaNetX"),
        (Sources::REACTOME, "Reactome"),
        (Sources::UNIPROT, "UniProt"),
        (Sources::GO, "GO"),
        (Sources::CURATED, "curated"),
    ];

    pub fn contains(self, other: Sources) -> bool {
        self.0 & other.0 == other.0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The sources' display names, for explanations.
    pub fn names(self) -> Vec<&'static str> {
        Sources::NAMES
            .iter()
            .filter(|(s, _)| self.contains(*s))
            .map(|(_, n)| *n)
            .collect()
    }
}

impl BitOr for Sources {
    type Output = Sources;
    fn bitor(self, rhs: Sources) -> Sources {
        Sources(self.0 | rhs.0)
    }
}

impl BitOrAssign for Sources {
    fn bitor_assign(&mut self, rhs: Sources) {
        self.0 |= rhs.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_combine_and_name_themselves() {
        let s = Sources::BIGG | Sources::RHEA;
        assert!(s.contains(Sources::BIGG));
        assert!(!s.contains(Sources::CHEBI));
        assert_eq!(s.names(), vec!["BiGG", "Rhea"]);
        assert!(Sources::NONE.is_empty());
    }
}
