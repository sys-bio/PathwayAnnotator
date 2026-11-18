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
        known: None,
        rejected: Vec::new(),
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
                known: None,
                rejected: Vec::new(),
            },
            Reaction {
                id: "J2".into(),
                name: None,
                participants: vec![part(3, Reactant), part(4, Product)],
                known: None,
                rejected: Vec::new(),
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

#[test]
fn confirmed_molecules_make_an_exact_reaction_match_high() {
    let p = pack();
    let mut m = model();
    // Not confirmed: the reactions are at most likely.
    let before = annotate(&p, &m);
    assert_ne!(annot_engine::confidence(&before.reactions[0]), Some(annot_engine::Confidence::High));

    // The user confirms all four molecules of J1 (glucose + ATP -> G6P + ADP).
    for (i, bigg) in [(0, "glc__D"), (1, "atp"), (2, "adp"), (3, "g6p")] {
        let Target::Entity(e) = entity(&p, bigg) else { unreachable!() };
        m.species[i].known = Some(e);
    }
    let after = annotate(&p, &m);
    let first = &after.reactions[0][0];
    assert_eq!(first.target, reaction(&p, "HEX1"));
    assert!(first.evidence.iter().any(|e| e.complete), "{:?}", first.evidence);
    assert_eq!(annot_engine::confidence(&after.reactions[0]), Some(annot_engine::Confidence::High));
    // J2 (G6P -> S4) is not complete: S4 is not confirmed.
    assert!(!after.reactions[1].iter().flat_map(|c| &c.evidence).any(|e| e.complete));
}

fn entity_id(p: &Pack, bigg: &str) -> kpack::EntityId {
    match entity(p, bigg) {
        Target::Entity(e) => e,
        t => panic!("{t:?}"),
    }
}

#[test]
fn a_confirmed_reaction_pins_down_its_remaining_molecule() {
    let p = pack();
    let mut m = model();
    // Glucose, ATP and ADP confirmed; J1 confirmed as hexokinase; S3 not.
    for (i, bigg) in [(0, "glc__D"), (1, "atp"), (2, "adp")] {
        m.species[i].known = Some(entity_id(&p, bigg));
    }
    let Target::Reaction(hex1) = reaction(&p, "HEX1") else { unreachable!() };
    m.reactions[0].known = Some(hex1);
    let s = annotate(&p, &m);
    let first = &s.species[3][0];
    assert_eq!(first.target, entity(&p, "g6p"));
    assert!(
        first.evidence.iter().any(|e| e.complete && e.note.contains("which is identified as")),
        "{:?}",
        first.evidence
    );
    assert_eq!(annot_engine::confidence(&s.species[3]), Some(annot_engine::Confidence::High));
}

#[test]
fn a_pinned_molecule_is_not_complete_when_the_line_up_rests_on_guesses() {
    let p = pack();
    let mut m = model();
    // J1 confirmed, but its other molecules only guessed from their names.
    let Target::Reaction(hex1) = reaction(&p, "HEX1") else { unreachable!() };
    m.reactions[0].known = Some(hex1);
    let s = annotate(&p, &m);
    assert_eq!(s.species[3][0].target, entity(&p, "g6p"));
    assert!(!s.species[3][0].evidence.iter().any(|e| e.complete));
}

#[test]
fn a_rejected_suggestion_does_not_come_back() {
    let p = pack();
    let mut m = model();
    let g6p = entity_id(&p, "g6p");
    m.species[3].rejected = vec![g6p];
    let s = annotate(&p, &m);
    assert!(
        !s.species[3].iter().any(|c| c.target == Target::Entity(g6p)),
        "{:?}",
        s.species[3]
    );
    // Nor is it reasoned from: with S3 not G6P, nothing makes S4 F6P via
    // the isomerase.
    assert!(!s.species[4].iter().any(|c| c.target == entity(&p, "f6p") && c.score > 1.0));

    // Rejected reactions likewise.
    let Target::Reaction(hex1) = reaction(&p, "HEX1") else { unreachable!() };
    m.reactions[0].rejected = vec![hex1];
    let s = annotate(&p, &m);
    assert!(!s.reactions[0].iter().any(|c| c.target == Target::Reaction(hex1)));
}

/// Kinetic models leave out H+: in "glucose + ATP -> G6P + X" read as
/// hexokinase, ADP and H+ are left over for X, and X is ADP.
#[test]
fn a_cofactor_is_found_when_h_plus_is_left_out() {
    let mut m = model();
    m.species[2] = species("X", None);
    m.species[3] = species("G6P", Some("D-glucose 6-phosphate"));
    let p = pack();
    let s = annotate(&p, &m);
    let first = &s.species[2][0];
    let Target::Entity(e) = first.target else { panic!("{:?}", first.target) };
    assert_eq!(p.str(p.entity(e).label), "ADP", "{:?}", s.species[2]);
    assert!(first.evidence.iter().any(|e| e.provider == "context" && e.note.contains("remaining product")));
}
