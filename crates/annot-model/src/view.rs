//! The engine's view of the model.

use std::collections::HashMap;

use annot_engine::{Compartment, Model, Participant, Reaction, Role, Species};

use crate::Workspace;

impl Workspace {
    /// What the engine may look at: names, ids, compartments and reaction
    /// participants, indexed like the SBML model's lists. No annotations.
    pub fn engine_model(&self) -> Model {
        let m = self.model();
        let mut out = Model::default();
        for c in &m.compartments {
            out.compartments.push(Compartment {
                id: c.id.clone(),
                name: c.name.clone(),
            });
        }
        let mut index: HashMap<&str, usize> = HashMap::new();
        for (i, s) in m.species.iter().enumerate() {
            index.insert(s.id.as_str(), i);
            out.species.push(Species {
                id: s.id.clone(),
                name: s.name.clone(),
                compartment: Some(s.compartment.clone()),
            });
        }
        for r in &m.reactions {
            let mut participants = Vec::new();
            for (refs, role) in [(&r.reactants, Role::Reactant), (&r.products, Role::Product)] {
                for sr in refs {
                    if let Some(&i) = index.get(sr.species.as_str()) {
                        participants.push(Participant {
                            species: i,
                            role,
                            stoichiometry: sr.stoichiometry.unwrap_or(1.0).abs(),
                        });
                    }
                }
            }
            for mr in &r.modifiers {
                if let Some(&i) = index.get(mr.species.as_str()) {
                    participants.push(Participant {
                        species: i,
                        role: Role::Modifier,
                        stoichiometry: 1.0,
                    });
                }
            }
            out.reactions.push(Reaction {
                id: r.id.clone(),
                name: r.name.clone(),
                participants,
            });
        }
        out
    }
}
