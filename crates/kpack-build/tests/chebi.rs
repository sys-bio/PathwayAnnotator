//! ChEBI on real excerpts (tests/fixtures/chebi, matching tests/fixtures/bigg):
//! it must add names, charge forms and old ids to BiGG's entities, and
//! nothing else.

use std::path::Path;

use kpack::{NameKind, Pack, Sources, Target, XrefScope};
use kpack_build::PackBuilder;
use kpack_build::sources::{bigg, chebi};

fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures"))
}

/// BiGG then ChEBI, as `kpack-build build` does, through an encode/decode.
fn build() -> (Pack, chebi::EnrichStats) {
    let mut b = PackBuilder::new();
    bigg::load(&mut b, &fixtures().join("bigg"), "test").expect("BiGG loads");
    let entities_before = b.entities.len();
    let stats = chebi::load(&mut b, &fixtures().join("chebi")).expect("ChEBI loads");
    assert_eq!(b.entities.len(), entities_before, "ChEBI adds no entities");
    let pack = b.finish("2026-11-13".into());
    (kpack::decode(&kpack::encode(&pack)).unwrap(), stats)
}

fn entity(p: &Pack, bigg_id: &str) -> u32 {
    match p.find_xref("bigg.metabolite", bigg_id).as_slice() {
        [Target::Entity(e)] => *e,
        other => panic!("{bigg_id}: {other:?}"),
    }
}

fn named(p: &Pack, name: &str) -> Vec<u32> {
    p.entities_named(name).iter().map(|h| h.target).collect()
}

#[test]
fn every_bigg_id_is_found() {
    let (_, stats) = build();
    assert_eq!(stats.entities, 11);
    assert_eq!(stats.seeds_unknown, 0);
    assert_eq!(stats.shared_seeds, 0);
    assert!(stats.names_added > 50, "{stats:?}");
}

#[test]
fn chebi_synonyms_find_bigg_entities() {
    let (p, _) = build();
    let glc = entity(&p, "glc__D");
    // BiGG calls it D-glucopyranose; people say these.
    for name in ["D-glucose", "glucose", "dextrose", "Grape sugar"] {
        assert_eq!(named(&p, name), [glc], "{name}");
    }
    let atp = entity(&p, "atp");
    assert_eq!(named(&p, "adenosine 5'-triphosphate"), [atp]);
    let hit = &p.entities_named("dextrose")[0];
    assert_eq!(hit.kind, NameKind::Synonym);
    assert!(hit.sources.contains(Sources::CHEBI));
    // The BiGG label is kept.
    assert_eq!(p.str(p.entity(glc).label), "D-glucopyranose");
}

#[test]
fn brand_and_unreviewed_names_are_left_out() {
    let (p, _) = build();
    // Brand names of ATP and unreviewed submissions for glucose in ChEBI.
    for name in ["Adephos", "Myotriphos", "Glucose injection", "D5W", "Rapilose"] {
        assert!(named(&p, name).is_empty(), "{name} should not be a name");
    }
}

#[test]
fn old_ids_are_recognized_but_marked() {
    let (p, _) = build();
    let atp = entity(&p, "atp");
    // CHEBI:2359 was merged into CHEBI:15422 (ATP).
    assert_eq!(p.find_xref("chebi", "CHEBI:2359"), [Target::Entity(atp)]);
    let ids: Vec<_> = p
        .entity_xrefs(atp, "chebi")
        .map(|x| (p.str(x.id), x.secondary))
        .collect();
    assert!(ids.contains(&("CHEBI:2359", true)), "{ids:?}");
    assert!(ids.contains(&("CHEBI:15422", false)), "{ids:?}");
}

#[test]
fn ids_carry_both_sources_and_charges() {
    let (p, _) = build();
    let atp = entity(&p, "atp");
    let x = p
        .entity_xrefs(atp, "chebi")
        .find(|x| p.str(x.id) == "CHEBI:30616")
        .unwrap();
    assert_eq!(x.form_charge, Some(-4));
    assert_eq!(x.sources, Sources::BIGG | Sources::CHEBI);
    assert!(p.entity(atp).sources.contains(Sources::CHEBI));
}

#[test]
fn broader_ids_lead_to_the_entity_but_not_as_identity() {
    let (p, stats) = build();
    let glc = entity(&p, "glc__D");
    // BiGG's glucose is CHEBI:4167 (D-glucopyranose). BioModels often say
    // CHEBI:17634 (D-glucose) or CHEBI:17234 (glucose): one and two "is a"
    // steps up.
    for id in ["CHEBI:17634", "CHEBI:17234"] {
        assert!(p.find_xref("chebi", id).is_empty(), "{id} is not an identity");
        assert_eq!(
            p.find_xref_related("chebi", id),
            [(Target::Entity(glc), XrefScope::Broader)],
            "{id}"
        );
    }
    assert!(stats.broader_added > 0);
    // Broader ids bring no names: "hexose" must not find glucose.
    assert!(!named(&p, "D-hexose").contains(&glc));
}

#[test]
fn narrower_ids_lead_to_the_entity() {
    let (p, stats) = build();
    let glc = entity(&p, "glc__D");
    // alpha-D-glucose "is a" D-glucopyranose.
    assert_eq!(
        p.find_xref_related("chebi", "CHEBI:17925"),
        [(Target::Entity(glc), XrefScope::Narrower)]
    );
    assert!(stats.narrower_added > 0);
}

#[test]
fn header_attributes_chebi() {
    let (p, _) = build();
    let s = p.header.sources.iter().find(|s| s.name == "ChEBI").unwrap();
    assert_eq!(s.version, "257");
    assert_eq!(s.license, "CC BY 4.0");
    assert!(s.notice.as_deref().unwrap().contains("the version of ChEBI is 257"));
}

#[test]
fn familiar_names_become_synonyms() {
    let (p, _) = build();
    let glc = entity(&p, "glc__D");
    let e = p.entity(glc);
    let synonyms: Vec<&str> = e.synonyms.iter().map(|s| p.str(*s)).collect();
    // BiGG calls it D-glucopyranose; people know it as D-glucose.
    assert!(synonyms.iter().any(|s| s.eq_ignore_ascii_case("D-glucose")), "{synonyms:?}");
    // Not the label again, no brand names, at most a handful.
    assert!(!synonyms.iter().any(|s| kpack::normalize_name(s) == kpack::normalize_name(p.str(e.label))));
    assert!(!synonyms.contains(&"Rapilose"));
    assert!(synonyms.len() <= kpack_build::builder::MAX_SYNONYMS);
    // ATP's charge forms are not synonyms of ATP.
    let atp = p.entity(entity(&p, "atp"));
    let atp_syn: Vec<&str> = atp.synonyms.iter().map(|s| p.str(*s)).collect();
    assert!(!atp_syn.iter().any(|s| kpack::normalize_name(s) == "atp"), "{atp_syn:?}");
}
