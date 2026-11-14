//! Reading an SBML model for the benchmark: what the engine may see
//! ([`annot_engine::Model`]), and the answer key from its annotations, which
//! the engine never sees.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use annot_engine::{Compartment, Model, Participant, Reaction, Role, Species};
use kpack::{Pack, Target, XrefScope};
use kpack_build::namespaces;

/// What an element's annotations say it is.
#[derive(Clone, Debug, PartialEq)]
pub enum Key {
    /// No `bqbiol:is` / `isVersionOf` annotation: not graded.
    Unannotated,
    /// Annotated, but with nothing the pack knows (a protein, a Reactome
    /// complex, ...): not graded, but counted.
    OutsidePack,
    /// Annotated with something in the pack. Any of `accept` is right: the
    /// targets with the most votes from the element's resources (several
    /// only on a tie).
    Known { accept: Vec<Target>, tie: bool },
}

/// One model, ready to grade.
pub struct Graded {
    pub id: String,
    pub model: Model,
    pub species: Vec<Key>,
    pub reactions: Vec<Key>,
}

/// Reads `xml` (model `id`) and resolves its annotations against `pack`.
pub fn read(id: &str, xml: &str, pack: &Pack) -> Result<Graded, String> {
    let read = sbml_io::read(xml).map_err(|d| {
        d.first()
            .map(|d| format!("{d:?}"))
            .unwrap_or_else(|| "unreadable".into())
    })?;
    let doc = read.document;
    let mut out = Graded {
        id: id.to_string(),
        model: Model::default(),
        species: Vec::new(),
        reactions: Vec::new(),
    };
    let Some(m) = &doc.model else {
        return Ok(out);
    };
    let resources = |meta: &sbml_io::Meta| -> Vec<String> {
        let Some(xml) = &meta.annotation else {
            return Vec::new();
        };
        sbml_io::miriam::read_annotation(xml, &doc.namespaces)
            .terms
            .into_iter()
            .filter(|t| !t.model && (t.qualifier == "is" || t.qualifier == "isVersionOf"))
            .flat_map(|t| t.resources)
            .collect()
    };

    for c in &m.compartments {
        out.model.compartments.push(Compartment {
            id: c.id.clone(),
            name: c.name.clone(),
        });
    }
    let mut index: HashMap<&str, usize> = HashMap::new();
    for (i, s) in m.species.iter().enumerate() {
        index.insert(s.id.as_str(), i);
        out.model.species.push(Species {
            id: s.id.clone(),
            name: s.name.clone(),
            compartment: Some(s.compartment.clone()),
        });
        out.species.push(key(pack, &resources(&s.meta), false));
    }
    for r in &m.reactions {
        let mut participants = Vec::new();
        let sides = [(&r.reactants, Role::Reactant), (&r.products, Role::Product)];
        for (refs, role) in sides {
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
        out.model.reactions.push(Reaction {
            id: r.id.clone(),
            name: r.name.clone(),
            participants,
        });
        out.reactions.push(key(pack, &resources(&r.meta), true));
    }
    Ok(out)
}

/// The answer key for one element from its resources. Each resource votes
/// for every target it leads to; exact ids win over broader or narrower.
fn key(pack: &Pack, resources: &[String], reaction: bool) -> Key {
    if resources.is_empty() {
        return Key::Unannotated;
    }
    let mut exact: BTreeMap<Target, usize> = BTreeMap::new();
    let mut related: BTreeMap<Target, usize> = BTreeMap::new();
    for uri in resources {
        let Some((written, id)) = kpack::split_resource(uri) else {
            continue;
        };
        let Some((def, id)) = namespaces::resolve(&written, &id) else {
            continue;
        };
        let mut voted = BTreeSet::new();
        for (target, scope) in pack.find_xref_related(def.prefix, &id) {
            let fits = matches!(target, Target::Reaction(_)) == reaction;
            if !fits || !voted.insert((target, scope == XrefScope::Exact)) {
                continue;
            }
            let tally = if scope == XrefScope::Exact { &mut exact } else { &mut related };
            *tally.entry(target).or_default() += 1;
        }
    }
    let votes = if exact.is_empty() { related } else { exact };
    let Some(&best) = votes.values().max() else {
        return Key::OutsidePack;
    };
    let accept: Vec<Target> = votes
        .into_iter()
        .filter(|(_, n)| *n == best)
        .map(|(t, _)| t)
        .collect();
    Key::Known {
        tie: accept.len() > 1,
        accept,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kpack_build::PackBuilder;
    use kpack_build::sources::{bigg, chebi};
    use std::path::Path;

    /// The kpack-build test fixtures, as a pack.
    fn pack() -> (Pack, &'static Path) {
        let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../kpack-build/tests/fixtures"));
        let mut b = PackBuilder::new();
        bigg::load(&mut b, &dir.join("bigg"), "test").unwrap();
        chebi::load(&mut b, &dir.join("chebi")).unwrap();
        (b.finish("test".into()), dir)
    }

    fn known(p: &Pack, ns: &str, id: &str) -> Key {
        Key::Known {
            accept: p.find_xref(ns, id),
            tie: false,
        }
    }

    #[test]
    fn reads_the_model_and_its_answer_key() {
        let (p, dir) = pack();
        let xml = std::fs::read_to_string(dir.join("biomodels/BIOMD0000000001.xml")).unwrap();
        let g = read("BIOMD0000000001", &xml, &p).unwrap();

        let ids: Vec<_> = g.model.species.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(ids, ["G6P", "Glc", "s1"]);
        assert_eq!(g.species[0], known(&p, "bigg.metabolite", "g6p"));
        // Annotated with the broader D-glucose id: the key is still glucose.
        assert_eq!(g.species[1], known(&p, "bigg.metabolite", "glc__D"));
        assert_eq!(g.species[2], known(&p, "bigg.metabolite", "atp"));
        assert_eq!(g.reactions[0], known(&p, "bigg.reaction", "HEX1"));

        // The engine's view has the structure but no annotations.
        let hk = &g.model.reactions[0];
        assert_eq!(hk.participants.len(), 2);
        assert_eq!(hk.participants[0].role, Role::Reactant);
        assert_eq!(g.model.species[hk.participants[0].species].id, "Glc");
    }

    #[test]
    fn keys_for_ties_and_outside_the_pack() {
        let (p, dir) = pack();
        let xml = std::fs::read_to_string(dir.join("biomodels/BIOMD0000000005.xml")).unwrap();
        let g = read("BIOMD0000000005", &xml, &p).unwrap();
        // "mix" is annotated as both ATP and ADP: either is accepted.
        let Key::Known { accept, tie } = &g.species[2] else {
            panic!("{:?}", g.species[2]);
        };
        assert!(tie);
        assert_eq!(accept.len(), 2);
        // A UniProt protein is not in a metabolism pack.
        assert_eq!(g.species[3], Key::OutsidePack);
    }
}
