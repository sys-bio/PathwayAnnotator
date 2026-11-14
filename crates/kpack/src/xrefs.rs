//! The reverse cross-reference index: (namespace, id) -> what it refers to.
//! Used to read an existing annotation ("CHEBI:15422 is ATP") and to check
//! one.

use serde::{Deserialize, Serialize};

use crate::data::{EntityId, NsId, ReactionId, XrefScope};
use crate::strings::{StrId, StringTable};

/// What a database id refers to in the pack.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Target {
    Entity(EntityId),
    Reaction(ReactionId),
}

/// One row of the index.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct XrefKey {
    pub ns: NsId,
    pub id: StrId,
    pub target: Target,
    pub scope: XrefScope,
}

/// Sorted by namespace, then by id text, so lookup is a binary search.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct XrefIndex {
    rows: Vec<XrefKey>,
}

impl XrefIndex {
    /// Builds the index. Duplicate rows are removed; where the same id
    /// links the same target in two scopes, the closer one (`Exact` before
    /// `Broader` before `Narrower`) is kept.
    pub fn build(mut rows: Vec<XrefKey>, strings: &StringTable) -> XrefIndex {
        rows.sort_by(|a, b| {
            a.ns.cmp(&b.ns)
                .then_with(|| strings.get(a.id).cmp(strings.get(b.id)))
                .then_with(|| target_order(a.target).cmp(&target_order(b.target)))
                .then_with(|| a.scope.cmp(&b.scope))
        });
        rows.dedup_by(|later, kept| {
            kept.ns == later.ns && kept.id == later.id && kept.target == later.target
        });
        XrefIndex { rows }
    }

    /// Everything `id` in namespace `ns` refers to, with how.
    pub fn lookup(&self, strings: &StringTable, ns: NsId, id: &str) -> Vec<(Target, XrefScope)> {
        let key = |row: &XrefKey| (row.ns, strings.get(row.id));
        let start = self.rows.partition_point(|r| key(r) < (ns, id));
        self.rows[start..]
            .iter()
            .take_while(|r| key(r) == (ns, id))
            .map(|r| (r.target, r.scope))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.rows.len()
    }

    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

fn target_order(t: Target) -> (u8, u32) {
    match t {
        Target::Entity(e) => (0, e),
        Target::Reaction(r) => (1, r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(ns: NsId, id: StrId, e: EntityId, scope: XrefScope) -> XrefKey {
        XrefKey {
            ns,
            id,
            target: Target::Entity(e),
            scope,
        }
    }

    #[test]
    fn finds_every_target_of_an_id() {
        let strings = StringTable::from_vec(vec!["C00002".into(), "CHEBI:15422".into()]);
        let index = XrefIndex::build(
            vec![
                row(1, 0, 7, XrefScope::Exact),
                row(0, 1, 7, XrefScope::Exact),
                row(0, 1, 7, XrefScope::Exact),
                row(0, 1, 9, XrefScope::Broader),
            ],
            &strings,
        );
        assert_eq!(index.len(), 3);
        assert_eq!(
            index.lookup(&strings, 0, "CHEBI:15422"),
            vec![
                (Target::Entity(7), XrefScope::Exact),
                (Target::Entity(9), XrefScope::Broader)
            ]
        );
        assert_eq!(
            index.lookup(&strings, 1, "C00002"),
            vec![(Target::Entity(7), XrefScope::Exact)]
        );
        assert!(index.lookup(&strings, 1, "CHEBI:15422").is_empty());
    }

    #[test]
    fn the_closest_scope_wins() {
        let strings = StringTable::from_vec(vec!["CHEBI:1".into()]);
        let index = XrefIndex::build(
            vec![row(0, 0, 3, XrefScope::Narrower), row(0, 0, 3, XrefScope::Exact)],
            &strings,
        );
        assert_eq!(
            index.lookup(&strings, 0, "CHEBI:1"),
            vec![(Target::Entity(3), XrefScope::Exact)]
        );
    }
}
