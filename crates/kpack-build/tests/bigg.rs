//! The BiGG source on a real excerpt (tests/fixtures/bigg): parse, build a
//! pack, encode and decode it, and check what the annotator will rely on.

use std::path::Path;

use kpack::{EntityFlags, NameKind, Pack, ReactionFlags, Sources, Target};
use kpack_build::PackBuilder;
use kpack_build::sources::bigg;

fn fixture_dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/bigg"))
}

fn read(name: &str) -> String {
    std::fs::read_to_string(fixture_dir().join(name)).expect("fixture")
}

/// The fixture as a pack, after an encode/decode round trip.
fn pack() -> Pack {
    let mut b = PackBuilder::new();
    let stats = bigg::load(&mut b, fixture_dir(), "test").expect("loads");
    assert_eq!(stats.metabolites, 11);
    assert_eq!(stats.reactions, 4);
    assert_eq!(stats.skipped_reactions, 0);
    assert!(stats.unknown_namespaces.is_empty(), "{:?}", stats.unknown_namespaces);
    let pack = b.finish("2026-11-13".into());
    kpack::decode(&kpack::encode(&pack)).expect("round trip")
}

fn entity_by_bigg(p: &Pack, id: &str) -> u32 {
    match p.find_xref("bigg.metabolite", id).as_slice() {
        [Target::Entity(e)] => *e,
        other => panic!("{id}: {other:?}"),
    }
}

fn reaction_by_bigg(p: &Pack, id: &str) -> u32 {
    match p.find_xref("bigg.reaction", id).as_slice() {
        [Target::Reaction(r)] => *r,
        other => panic!("{id}: {other:?}"),
    }
}

#[test]
fn charge_forms_become_one_metabolite() {
    let mets = bigg::parse_metabolites(&read(bigg::METABOLITES_FILE)).unwrap();
    let atp = mets.iter().find(|m| m.id == "atp").unwrap();
    assert!(atp.forms.len() >= 2, "ATP has several charge forms");
    assert_eq!(atp.default_form.as_deref(), Some("atp:-4"));
    assert!(atp.compartments.iter().any(|c| c == "c"));
}

#[test]
fn reaction_copies_are_merged() {
    let rxns = bigg::parse_reactions(&read(bigg::REACTIONS_FILE)).unwrap();
    assert_eq!(rxns.iter().filter(|r| r.id == "ACOAD8f").count(), 1);
    let hex1 = rxns.iter().find(|r| r.id == "HEX1").unwrap();
    let mets: Vec<_> = hex1.participants.iter().map(|p| p.metabolite.as_str()).collect();
    assert_eq!(mets, ["glc__D", "atp", "adp", "h", "g6p"]);
    assert!(hex1.participants.iter().all(|p| p.compartment == "c"));
    assert_eq!(hex1.participants[0].coef, -1.0);
}

#[test]
fn atp_has_its_ids_and_label() {
    let p = pack();
    let atp = entity_by_bigg(&p, "atp");
    let e = p.entity(atp);
    assert_eq!(p.str(e.label), "ATP");
    assert_eq!(e.charge, Some(-4));
    assert_eq!(e.formula.map(|f| p.str(f)), Some("C10H12N5O13P3"));
    assert!(e.flags.contains(EntityFlags::CURRENCY));
    assert_eq!(e.sources, Sources::BIGG);

    // Both the neutral and the 4- ChEBI entries, each with its charge.
    let chebi: Vec<_> = p
        .entity_xrefs(atp, "chebi")
        .map(|x| (p.str(x.id), x.form_charge))
        .collect();
    assert!(chebi.contains(&("CHEBI:15422", Some(0))), "{chebi:?}");
    assert!(chebi.contains(&("CHEBI:30616", Some(-4))), "{chebi:?}");
    // KEGG has one entry for every form, so it names no particular charge.
    let kegg: Vec<_> = p
        .entity_xrefs(atp, "kegg.compound")
        .map(|x| (p.str(x.id), x.form_charge))
        .collect();
    assert_eq!(kegg, [("C00002", None)]);

    // An existing annotation leads back to the entity.
    assert_eq!(p.find_xref("chebi", "CHEBI:15422"), [Target::Entity(atp)]);
    assert_eq!(p.find_xref("kegg.compound", "C00002"), [Target::Entity(atp)]);
}

#[test]
fn names_find_entities() {
    let p = pack();
    let atp = entity_by_bigg(&p, "atp");
    let g6p = entity_by_bigg(&p, "g6p");
    let hits = p.entities_named("ATP");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].target, atp);
    assert_eq!(hits[0].kind, NameKind::Preferred);
    // Charge suffixes and case do not matter.
    assert_eq!(p.entities_named("atp(4-)")[0].target, atp);
    // BiGG ids are names too.
    let hits = p.entities_named("G6P");
    assert_eq!(hits[0].target, g6p);
    assert_eq!(hits[0].kind, NameKind::BiggId);
    assert!(p.entities_named("no such molecule").is_empty());
}

#[test]
fn reactions_have_participants_xrefs_and_names() {
    let p = pack();
    let hex1 = reaction_by_bigg(&p, "HEX1");
    let r = p.reaction(hex1);
    assert_eq!(p.str(r.label), "hexokinase (D-glucose:ATP)");
    assert!(!r.flags.contains(ReactionFlags::TRANSPORT));

    let glc = entity_by_bigg(&p, "glc__D");
    let g6p = entity_by_bigg(&p, "g6p");
    let coef = |e| r.participants.iter().find(|x| x.entity == e).map(|x| x.coef);
    assert_eq!(coef(glc), Some(-1.0));
    assert_eq!(coef(g6p), Some(1.0));
    let cytosol = r.participants[0].compartment.unwrap();
    assert_eq!(p.str(p.compartments[cytosol as usize].code), "c");
    assert_eq!(
        p.compartments[cytosol as usize].go.map(|g| p.str(g)),
        Some("GO:0005829")
    );

    // Rhea and EC ids, both ways.
    assert_eq!(p.find_xref("rhea", "17825"), [Target::Reaction(hex1)]);
    assert_eq!(p.find_xref("ec-code", "2.7.1.1"), [Target::Reaction(hex1)]);

    // By name: full, short, and BiGG id.
    assert_eq!(p.reactions_named("hexokinase (D-glucose:ATP)")[0].target, hex1);
    assert_eq!(p.reactions_named("hexokinase")[0].target, hex1);
    assert_eq!(p.reactions_named("hex1")[0].target, hex1);
}

#[test]
fn participant_index_links_the_pathway() {
    let p = pack();
    let g6p = entity_by_bigg(&p, "g6p");
    let f6p = entity_by_bigg(&p, "f6p");
    let hex1 = reaction_by_bigg(&p, "HEX1");
    let pgi = reaction_by_bigg(&p, "PGI");
    let pfk = reaction_by_bigg(&p, "PFK");
    // Lists are sorted by reaction id, whatever order the file had.
    let sorted = |mut v: Vec<u32>| {
        v.sort_unstable();
        v
    };
    assert_eq!(p.reactions_with(g6p), sorted(vec![hex1, pgi]));
    assert_eq!(p.reactions_with(f6p), sorted(vec![pgi, pfk]));
}

#[test]
fn the_header_carries_the_source_and_its_notice() {
    let p = pack();
    assert_eq!(p.header.built, "2026-11-13");
    let s = &p.header.sources[0];
    assert_eq!(s.name, "BiGG");
    assert!(s.notice.as_deref().unwrap().contains("Regents of the University of California"));
}
