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
    /// The full EC numbers each reaction's annotation gives (four levels,
    /// no "-"): the curator's enzyme, for judging equivalent answers.
    pub reaction_ecs: Vec<Vec<String>>,
    /// Whether each reaction's answer key rests only on EC numbers: none of
    /// its other ids (KEGG reaction, Rhea, ...) is in the pack. An EC number
    /// names an enzyme class, not one reaction, so such a key cannot tell
    /// apart reactions the class covers (glucokinase on alpha-glucose from
    /// hexokinase on glucose).
    pub reaction_key_coarse: Vec<bool>,
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
    let reaction_ecs = (0..model.reactions.len())
        .map(|i| {
            w.identity_uris(Element::Reaction(i))
                .iter()
                .filter_map(|u| pack.resolve_resource(u))
                .filter(|(ns, id)| pack.namespace_prefix(*ns) == "ec-code" && id.split('.').count() == 4 && !id.contains('-'))
                .map(|(_, id)| id)
                .collect()
        })
        .collect();
    let reaction_key_coarse = (0..model.reactions.len())
        .map(|i| {
            let uris = w.identity_uris(Element::Reaction(i));
            let specific = uris.iter().any(|u| {
                pack.resolve_resource(u)
                    .is_some_and(|(ns, _)| pack.namespace_prefix(ns) != "ec-code")
                    && pack.find_resource(u).iter().any(|(t, _)| matches!(t, Target::Reaction(_)))
            });
            !specific
        })
        .collect();
    Ok(Graded {
        id: id.to_string(),
        model,
        species,
        reactions,
        reaction_ecs,
        reaction_key_coarse,
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
