//! Candidates, their evidence, and the ranked suggestions for a model.

use kpack::Target;

use crate::model::Model;

/// One piece of evidence for (or, with a negative weight, against) a
/// candidate.
#[derive(Clone, Debug, PartialEq)]
pub struct Evidence {
    /// The provider that found it.
    pub provider: &'static str,
    /// Its contribution to the score.
    pub weight: f32,
    /// For people: `name "G6P" is used for it in 17 curated models`.
    pub note: String,
}

/// A possible meaning of a species or reaction.
#[derive(Clone, Debug, PartialEq)]
pub struct Candidate {
    pub target: Target,
    /// The sum of the evidence weights.
    pub score: f32,
    pub evidence: Vec<Evidence>,
}

/// Candidates for every species and reaction of a model, indexed like
/// [`Model::species`] and [`Model::reactions`]. Best first after ranking.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Suggestions {
    pub species: Vec<Vec<Candidate>>,
    pub reactions: Vec<Vec<Candidate>>,
}

impl Suggestions {
    /// No candidates yet, sized for `model`.
    pub fn for_model(model: &Model) -> Suggestions {
        Suggestions {
            species: vec![Vec::new(); model.species.len()],
            reactions: vec![Vec::new(); model.reactions.len()],
        }
    }

    /// Adds evidence for `target` to a list, making it a candidate if new.
    pub fn add(list: &mut Vec<Candidate>, target: Target, evidence: Evidence) {
        let c = match list.iter_mut().position(|c| c.target == target) {
            Some(i) => &mut list[i],
            None => {
                list.push(Candidate {
                    target,
                    score: 0.0,
                    evidence: Vec::new(),
                });
                list.last_mut().expect("just pushed")
            }
        };
        c.score += evidence.weight;
        c.evidence.push(evidence);
    }

    /// Sorts every list best first. Ties are broken by target, so the order
    /// never depends on the order evidence arrived in.
    pub fn rank(&mut self) {
        for list in self.species.iter_mut().chain(self.reactions.iter_mut()) {
            list.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.target.cmp(&b.target)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(weight: f32) -> Evidence {
        Evidence {
            provider: "test",
            weight,
            note: String::new(),
        }
    }

    #[test]
    fn evidence_adds_up_and_ranks() {
        let mut list = Vec::new();
        Suggestions::add(&mut list, Target::Entity(2), ev(1.0));
        Suggestions::add(&mut list, Target::Entity(1), ev(1.5));
        Suggestions::add(&mut list, Target::Entity(2), ev(1.0));
        Suggestions::add(&mut list, Target::Entity(3), ev(1.5));
        let mut s = Suggestions {
            species: vec![list],
            reactions: Vec::new(),
        };
        s.rank();
        let order: Vec<_> = s.species[0].iter().map(|c| (c.target, c.score)).collect();
        assert_eq!(
            order,
            [
                (Target::Entity(2), 2.0),
                (Target::Entity(1), 1.5),
                (Target::Entity(3), 1.5)
            ]
        );
        assert_eq!(s.species[0][0].evidence.len(), 2);
    }
}
