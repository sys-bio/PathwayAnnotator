//! Reaction context on the kpack-build fixtures (upper glycolysis: HEX1,
//! PGI, PFK), with a model whose intermediates have meaningless names.

use std::path::Path;

use annot_engine::{Compartment, Model, Participant, Reaction, Role, Species, annotate};
use kpack::{Pack, Target};
use kpack_build::PackBuilder;
use kpack_build::sources::{bigg, chebi};

fn pack() -> Pack {
    let dir = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../kpack-build/tests/fixtures"));
    let mut b = PackBuilder::new();
    bigg::load(&mut b, &dir.join("bigg"), "test").unwrap();
    chebi::load(&mut b, &dir.join("chebi")).unwrap();
    b.finish("test".into())
}

fn entity(p: &Pack, bigg_id: &str) -> Target {
    p.find_xref("bigg.metabolite", bigg_id)[0]
}

fn reaction(p: &Pack, bigg_id: &str) -> Target {
    p.find_xref("bigg.reaction", bigg_id)[0]
}

fn species(id: &str, name: Option<&str>) -> Species {
    Species {
        id: id.into(),
        name: name.map(Into::into),
        compartment: Some("cell".into()),
    }
}

fn part(species: usize, role: Role) -> Participant {
    Participant {
        species,
        role,
        stoichiometry: 1.0,
    }
}

/// 0 glucose, 1 ATP, 2 ADP, 3 S3, 4 S4.
/// J1: glucose + ATP -> S3 + ADP     (hexokinase: S3 is G6P)
/// J2: S3 -> S4                      (isomerase: S4 is F6P)
fn model() -> Model {
    use Role::*;
    Model {
        compartments: vec![Compartment {
            id: "cell".into(),
            name: None,
        }],
        species: vec![
            species("Glc", Some("glucose")),
            species("ATP", None),
            species("ADP", None),
            species("S3", None),
            species("S4", None),
        ],
        reactions: vec![
            Reaction {
                id: "J1".into(),
                name: None,
                participants: vec![part(0, Reactant), part(1, Reactant), part(3, Product), part(2, Product)],
            },
            Reaction {
                id: "J2".into(),
                name: None,
                participants: vec![part(3, Reactant), part(4, Product)],
            },
        ],
    }
}

#[test]
fn a_leftover_identifies_a_species() {
    let p = pack();
    let s = annotate(&p, &model());
    let first = &s.species[3][0];
    assert_eq!(first.target, entity(&p, "g6p"), "{:?}", s.species[3]);
    assert!(first.evidence.iter().any(|e| e.provider == "context" && e.note.contains("remaining product")));
}

#[test]
fn identifications_spread_along_the_pathway() {
    let p = pack();
    let s = annotate(&p, &model());
    // S4 is found from S3, which was itself found from context.
    assert_eq!(s.species[4].first().map(|c| c.target), Some(entity(&p, "f6p")), "{:?}", s.species[4]);
}

#[test]
fn reactions_are_matched_from_their_species() {
    let p = pack();
    let s = annotate(&p, &model());
    assert_eq!(s.reactions[0].first().map(|c| c.target), Some(reaction(&p, "HEX1")), "{:?}", s.reactions[0]);
    assert_eq!(s.reactions[1].first().map(|c| c.target), Some(reaction(&p, "PGI")), "{:?}", s.reactions[1]);
}

#[test]
fn named_species_keep_their_names() {
    let p = pack();
    let s = annotate(&p, &model());
    assert_eq!(s.species[0][0].target, entity(&p, "glc__D"));
    assert_eq!(s.species[1][0].target, entity(&p, "atp"));
}
