//! What existing annotations mean in the knowledge pack.

use std::collections::{BTreeMap, BTreeSet};

use kpack::{Pack, Target, XrefScope};

/// What an element's annotation resources lead to.
#[derive(Clone, Debug, PartialEq)]
pub enum Resolution {
    /// No resources.
    Unannotated,
    /// Resources, but none the pack knows (a protein, a Reactome complex).
    OutsidePack,
    /// The targets with the most votes (several only on a tie), and whether
    /// they came from exact ids or only from broader or narrower ones.
    Known {
        targets: Vec<Target>,
        tie: bool,
        exact: bool,
    },
}

/// Resolves annotation URIs against the pack. Each URI votes for every
/// target it leads to; exact ids win over broader or narrower ones, and the
/// targets with the most votes win. Only entities count for a species
/// (`reaction` false) and only reactions for a reaction.
///
/// This is the same rule the BioModels source uses when it builds the pack,
/// and the benchmark uses for its answer key.
pub fn resolve(pack: &Pack, uris: &[String], reaction: bool) -> Resolution {
    if uris.is_empty() {
        return Resolution::Unannotated;
    }
    let mut exact: BTreeMap<Target, usize> = BTreeMap::new();
    let mut related: BTreeMap<Target, usize> = BTreeMap::new();
    for uri in uris {
        let mut voted = BTreeSet::new();
        for (target, scope) in pack.find_resource(uri) {
            let fits = matches!(target, Target::Reaction(_)) == reaction;
            if !fits || !voted.insert((target, scope == XrefScope::Exact)) {
                continue;
            }
            let tally = if scope == XrefScope::Exact { &mut exact } else { &mut related };
            *tally.entry(target).or_default() += 1;
        }
    }
    let is_exact = !exact.is_empty();
    let votes = if is_exact { exact } else { related };
    let Some(&best) = votes.values().max() else {
        return Resolution::OutsidePack;
    };
    let targets: Vec<Target> = votes.into_iter().filter(|(_, n)| *n == best).map(|(t, _)| t).collect();
    Resolution::Known {
        tie: targets.len() > 1,
        targets,
        exact: is_exact,
    }
}
