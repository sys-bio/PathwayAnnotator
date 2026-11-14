//! BioModels on hand-written models (tests/fixtures/biomodels, see ABOUT.md
//! there), after BiGG and ChEBI as in a real build.

use std::path::Path;

use kpack::{Pack, Sources, Target};
use kpack_build::PackBuilder;
use kpack_build::sources::{bigg, biomodels, chebi};

fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

fn build(exclude_test: bool) -> (Pack, biomodels::LoadStats) {
    let mut b = PackBuilder::new();
    bigg::load(&mut b, &fixtures().join("bigg"), "test").unwrap();
    chebi::load(&mut b, &fixtures().join("chebi")).unwrap();
    let stats = biomodels::load(&mut b, &fixtures().join("biomodels"), "test", exclude_test).unwrap();
    let pack = b.finish("2026-11-13".into());
    (kpack::decode(&kpack::encode(&pack)).unwrap(), stats)
}

fn entity(p: &Pack, bigg_id: &str) -> u32 {
    match p.find_xref("bigg.metabolite", bigg_id).as_slice() {
        [Target::Entity(e)] => *e,
        other => panic!("{bigg_id}: {other:?}"),
    }
}

/// (target, count) of the BioModels-backed meanings of a name.
fn from_models(p: &Pack, name: &str) -> Vec<(u32, u16)> {
    p.entities_named(name)
        .iter()
        .filter(|h| h.sources.contains(Sources::BIOMODELS))
        .map(|h| (h.target, h.count))
        .collect()
}

#[test]
fn elements_are_resolved_through_every_uri_form() {
    let (_, s) = build(false);
    assert_eq!(s.listed, 3);
    assert_eq!(s.ingest.models, 3);
    assert!(s.unreadable.is_empty(), "{:?}", s.unreadable);
    let i = &s.ingest;
    assert_eq!(i.elements, 10);
    assert_eq!(i.resolved, 8, "{i:?}");
    assert_eq!(i.ambiguous, 1, "mix is annotated as both ATP and ADP");
    assert_eq!(i.unresolved, 1, "the UniProt protein");
    assert_eq!(i.unknown_namespaces.get("uniprot"), Some(&1));
}

#[test]
fn model_names_are_counted_per_model() {
    let (p, _) = build(false);
    let g6p = entity(&p, "g6p");
    // "G6P" in all three models, written three different ways (URN, compact
    // identifiers.org, ChEBI page link to the 2- form).
    assert_eq!(from_models(&p, "G6P"), [(g6p, 3)]);
    // It is also BiGG's id for it; the hit carries both sources.
    let hit = p.entities_named("G6P")[0];
    assert_eq!(hit.sources, Sources::BIGG | Sources::BIOMODELS);
    assert_eq!(from_models(&p, "glucose-6-phosphate"), [(g6p, 1)]);
}

#[test]
fn a_broader_annotation_still_names_the_entity() {
    let (p, _) = build(false);
    // Glc is annotated with CHEBI:17634 (D-glucose), broader than BiGG's
    // D-glucopyranose; it is the only entity it leads to.
    let glc = entity(&p, "glc__D");
    assert_eq!(from_models(&p, "Glc"), [(glc, 1)]);
}

#[test]
fn reaction_names_come_too() {
    let (p, _) = build(false);
    let hex1 = match p.find_xref("bigg.reaction", "HEX1").as_slice() {
        [Target::Reaction(r)] => *r,
        other => panic!("{other:?}"),
    };
    let hk: Vec<_> = p
        .reactions_named("HK")
        .iter()
        .filter(|h| h.sources.contains(Sources::BIOMODELS))
        .map(|h| (h.target, h.count))
        .collect();
    assert_eq!(hk, [(hex1, 1)]);
}

#[test]
fn a_name_that_means_something_different_everywhere_is_dropped() {
    let (p, s) = build(false);
    // s1 is ATP, fructose 6-phosphate and ADP in three models.
    assert!(from_models(&p, "s1").is_empty());
    assert_eq!(s.ingest.names_dropped, 3);
}

#[test]
fn the_test_set_can_be_held_out() {
    let (p, s) = build(true);
    assert_eq!(s.held_out, 1);
    assert_eq!(s.ingest.models, 2);
    let g6p = entity(&p, "g6p");
    assert_eq!(from_models(&p, "G6P"), [(g6p, 2)]);
    let src = p.header.sources.iter().find(|s| s.name == "BioModels").unwrap();
    assert!(src.version.contains("test set excluded"), "{}", src.version);
    assert_eq!(src.license, "CC0 1.0");
}

#[test]
fn the_pack_reads_annotation_uris_itself() {
    // The same resolution kpack-build does at build time, from the pack
    // alone, as the browser app will.
    let (p, _) = build(false);
    let atp = Target::Entity(entity(&p, "atp"));
    for uri in [
        "http://identifiers.org/chebi/CHEBI:15422",
        "https://identifiers.org/CHEBI:15422",
        "urn:miriam:obo.chebi:CHEBI%3A15422",
        "http://identifiers.org/obo.chebi/CHEBI:15422",
        "http://www.ebi.ac.uk/chebi/searchId.do?chebiId=CHEBI:15422",
        "urn:miriam:kegg.compound:C00002",
    ] {
        let found: Vec<Target> = p.find_resource(uri).into_iter().map(|(t, _)| t).collect();
        assert_eq!(found, [atp], "{uri}");
    }
    assert!(p.find_resource("http://identifiers.org/uniprot/P12345").is_empty());
}
