//! Reading an SBML model for the benchmark: what the engine may see
//! ([`annot_engine::Model`]), and the answer key from its annotations, which
//! the engine never sees. Both come from [`annot_model`], as they will in
//! the app.

use annot_engine::Model;
use annot_model::{Element, Resolution};
use kpack::{Pack, Target};

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

impl From<Resolution> for Key {
    fn from(r: Resolution) -> Key {
        match r {
            Resolution::Unannotated => Key::Unannotated,
            Resolution::OutsidePack => Key::OutsidePack,
            Resolution::Known { targets, tie, .. } => Key::Known { accept: targets, tie },
        }
    }
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
    let w = annot_model::load_sbml(xml).map_err(|e| e.to_string())?;
    let model = w.engine_model();
    let species = (0..model.species.len())
        .map(|i| w.resolution(pack, Element::Species(i)).into())
        .collect();
    let reactions = (0..model.reactions.len())
        .map(|i| w.resolution(pack, Element::Reaction(i)).into())
        .collect();
    Ok(Graded {
        id: id.to_string(),
        model,
        species,
        reactions,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use annot_engine::Role;
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
