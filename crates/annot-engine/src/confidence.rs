//! How far a first suggestion can be trusted, in three levels the app can
//! show: accept with a glance, check, or look closely.
//!
//! The rules and their thresholds are set from the benchmark (DESIGN.md
//! section 6, "Confidence"), which also reports how often each level is
//! right.

use crate::Candidate;

/// How far the first suggestion for an element can be trusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Confidence {
    High,
    Likely,
    Uncertain,
}

impl Confidence {
    pub fn label(self) -> &'static str {
        match self {
            Confidence::High => "high",
            Confidence::Likely => "likely",
            Confidence::Uncertain => "uncertain",
        }
    }
}

/// What a first suggestion rests on, strongest first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Basis {
    /// The name or id as written is in the dictionary.
    Name,
    /// A reading of the name, or a database id inside the SBML id.
    Decoded,
    /// Only the reaction context.
    Context,
}

/// What the first candidate rests on.
pub fn basis(c: &Candidate) -> Basis {
    let has = |p: &str| c.evidence.iter().any(|e| e.provider == p);
    if has("names") {
        Basis::Name
    } else if has("decoder") {
        Basis::Decoded
    } else {
        Basis::Context
    }
}

/// A direct name match with no competitor at this score or more is High.
pub const HIGH_NAME: f32 = 2.0;
/// A decoded match at this score or more, with a clear lead, is High.
pub const HIGH_DECODED: f32 = 2.0;
/// A clear lead: the runner-up scores at most this fraction of the best.
pub const CLEAR_LEAD: f32 = 0.75;
/// Below these, a suggestion is Uncertain.
pub const LIKELY_NAME: f32 = 1.5;
pub const LIKELY_DECODED: f32 = 1.0;
pub const LIKELY_CONTEXT: f32 = 1.5;

/// The confidence of the first of a ranked candidate list, or `None` if it
/// is empty.
///
/// On the benchmark's held-out models (2026-11-14), High suggestions were
/// right about 99% of the time, Likely about 90%, Uncertain about half.
pub fn confidence(list: &[Candidate]) -> Option<Confidence> {
    let first = list.first()?;
    let score = first.score;
    let second = list.get(1).map_or(0.0, |c| c.score);
    let alone = list.len() == 1;
    let clear = second <= CLEAR_LEAD * score;
    // A reaction whose every molecule is identified and matches one
    // database reaction exactly is High, whatever else the evidence is.
    let complete = first.evidence.iter().any(|e| e.complete);
    Some(match basis(first) {
        _ if complete && clear => Confidence::High,
        Basis::Name if alone && score >= HIGH_NAME => Confidence::High,
        Basis::Name if score >= LIKELY_NAME => Confidence::Likely,
        Basis::Decoded if clear && score >= HIGH_DECODED => Confidence::High,
        Basis::Decoded if score >= LIKELY_DECODED => Confidence::Likely,
        Basis::Context if score >= LIKELY_CONTEXT => Confidence::Likely,
        _ => Confidence::Uncertain,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Evidence;
    use kpack::Target;

    fn cand(t: u32, provider: &'static str, score: f32) -> Candidate {
        Candidate {
            target: Target::Entity(t),
            score,
            evidence: vec![Evidence {
                provider,
                weight: score,
                note: String::new(),
                complete: false,
            }],
        }
    }

    #[test]
    fn levels() {
        assert_eq!(confidence(&[]), None);
        // A lone strong name match.
        assert_eq!(confidence(&[cand(1, "names", 2.5)]), Some(Confidence::High));
        // The same with a competitor is only likely.
        assert_eq!(
            confidence(&[cand(1, "names", 2.5), cand(2, "names", 1.0)]),
            Some(Confidence::Likely)
        );
        // A weak name match.
        assert_eq!(confidence(&[cand(1, "names", 1.2)]), Some(Confidence::Uncertain));
        // A decoded match with a clear lead.
        assert_eq!(
            confidence(&[cand(1, "decoder", 2.0), cand(2, "decoder", 1.0)]),
            Some(Confidence::High)
        );
        // Context alone is never high.
        assert_eq!(confidence(&[cand(1, "context", 5.0)]), Some(Confidence::Likely));
        assert_eq!(confidence(&[cand(1, "context", 1.0)]), Some(Confidence::Uncertain));
    }
}
