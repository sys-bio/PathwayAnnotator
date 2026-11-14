//! A workspace on real pack data (the kpack-build fixtures): loading,
//! reading existing annotations, accepting suggestions, undo, and writing
//! SBML that reads back with nothing lost.

use std::path::Path;

use annot_model::{EditError, Element, Format, Resolution, load, load_antimony, uris_for};
use kpack::{Pack, Target};
use kpack_build::PackBuilder;
use kpack_build::sources::{bigg, chebi};

fn fixtures() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../kpack-build/tests/fixtures"))
}

fn pack() -> Pack {
    let mut b = PackBuilder::new();
    bigg::load(&mut b, &fixtures().join("bigg"), "test").unwrap();
    chebi::load(&mut b, &fixtures().join("chebi")).unwrap();
    b.finish("test".into())
}

fn entity(p: &Pack, bigg_id: &str) -> Target {
    p.find_xref("bigg.metabolite", bigg_id)[0]
}

/// A model with one annotated species (G6P, plus another tool's own
/// annotation XML), one plain species, and a reaction.
const MODEL: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level2/version4" level="2" version="4">
  <model id="m">
    <listOfCompartments><compartment id="cell" size="1"/></listOfCompartments>
    <listOfSpecies>
      <species metaid="m1" id="G6P" name="glucose 6-phosphate" compartment="cell" initialConcentration="1">
        <annotation>
          <tool:data xmlns:tool="http://example.org/tool" tool:x="42"/>
          <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:bqbiol="http://biomodels.net/biology-qualifiers/">
            <rdf:Description rdf:about="#m1">
              <bqbiol:is><rdf:Bag><rdf:li rdf:resource="urn:miriam:obo.chebi:CHEBI%3A4170"/></rdf:Bag></bqbiol:is>
            </rdf:Description>
          </rdf:RDF>
        </annotation>
      </species>
      <species id="S2" compartment="cell" initialConcentration="1"/>
    </listOfSpecies>
    <listOfReactions>
      <reaction id="J1">
        <listOfReactants><speciesReference species="G6P"/></listOfReactants>
        <listOfProducts><speciesReference species="S2"/></listOfProducts>
      </reaction>
    </listOfReactions>
  </model>
</sbml>"##;

#[test]
fn loads_sbml_and_gives_the_engine_its_view() {
    let w = load(MODEL).unwrap();
    assert_eq!(w.format(), Format::Sbml);
    let m = w.engine_model();
    assert_eq!(m.species.len(), 2);
    assert_eq!(m.species[0].name.as_deref(), Some("glucose 6-phosphate"));
    assert_eq!(m.reactions[0].participants.len(), 2);
    assert_eq!(m.compartments[0].id, "cell");
}

#[test]
fn existing_annotations_resolve_in_the_pack() {
    let p = pack();
    let w = load(MODEL).unwrap();
    assert_eq!(
        w.resolution(&p, Element::Species(0)),
        Resolution::Known {
            targets: vec![entity(&p, "g6p")],
            tie: false,
            exact: true
        }
    );
    assert_eq!(w.resolution(&p, Element::Species(1)), Resolution::Unannotated);
}

#[test]
fn accepting_writes_terms_and_sbo_and_reads_back() {
    let p = pack();
    let mut w = load(MODEL).unwrap();
    let f6p = entity(&p, "f6p");
    let added = w.accept(&p, Element::Species(1), f6p).unwrap();
    assert_eq!(added, uris_for(&p, f6p));
    assert!(added.iter().any(|u| u.contains("CHEBI:")), "{added:?}");

    // The species now has a metaid, the terms and an SBO term.
    let text = w.to_sbml();
    let again = load(&text).unwrap();
    assert_eq!(again.identity_uris(Element::Species(1)), added);
    assert_eq!(
        again.resolution(&p, Element::Species(1)),
        Resolution::Known {
            targets: vec![f6p],
            tie: false,
            exact: true
        }
    );
    let s2 = &again.document().model.as_ref().unwrap().species[1];
    assert_eq!(s2.meta.metaid.as_deref(), Some("meta_S2"));
    assert_eq!(s2.meta.sbo_term.as_deref(), Some("SBO:0000247"));
}

#[test]
fn editing_keeps_other_annotation_content() {
    let p = pack();
    let mut w = load(MODEL).unwrap();
    // Add a second identity to the species that has another tool's XML.
    w.accept(&p, Element::Species(0), entity(&p, "g6p")).unwrap();
    let text = w.to_sbml();
    assert!(text.contains("tool:data") && text.contains("tool:x=\"42\""), "{text}");
    // The old URN is still there, beside the new ids.
    let again = load(&text).unwrap();
    let uris = again.identity_uris(Element::Species(0));
    assert!(uris.contains(&"urn:miriam:obo.chebi:CHEBI%3A4170".to_string()), "{uris:?}");
    assert!(uris.len() > 1);
}

#[test]
fn undo_and_redo() {
    let p = pack();
    let mut w = load(MODEL).unwrap();
    let original = w.to_sbml();
    assert!(!w.can_undo());
    w.accept(&p, Element::Species(1), entity(&p, "f6p")).unwrap();
    let edited = w.to_sbml();
    assert_ne!(edited, original);
    assert!(w.undo());
    assert_eq!(w.to_sbml(), original);
    assert!(w.redo());
    assert_eq!(w.to_sbml(), edited);
    // A new edit clears the redo list.
    w.undo();
    w.set_sbo(Element::Species(1), Some(247)).unwrap();
    assert!(!w.can_redo());
}

#[test]
fn a_batch_is_one_undo_step() {
    let p = pack();
    let mut w = load(MODEL).unwrap();
    w.set_sbo(Element::Species(0), Some(247)).unwrap();
    let before = w.to_sbml();
    w.batch(|w| {
        w.accept(&p, Element::Species(1), entity(&p, "f6p")).unwrap();
        // Changes nothing, so it is not recorded.
        w.set_sbo(Element::Species(0), Some(247)).unwrap();
        w.rename(Element::Reaction(0), "PGI").unwrap();
        w.set_sbo(Element::Reaction(0), Some(176)).unwrap();
    });
    let after = w.to_sbml();
    assert!(after.contains("id=\"PGI\""));
    // One undo reverts the whole batch, and only the batch.
    assert!(w.undo());
    assert_eq!(w.to_sbml(), before);
    assert!(w.can_undo());
    assert!(w.redo());
    assert_eq!(w.to_sbml(), after);
    // An empty batch adds no step.
    w.batch(|_| ());
    w.undo();
    assert_eq!(w.to_sbml(), before);
}

#[test]
fn accepting_twice_adds_but_replacing_swaps() {
    let p = pack();
    let f6p = entity(&p, "f6p");
    let g6p = entity(&p, "g6p");

    // Two accepts leave both identities: what the app must not do.
    let mut w = load(MODEL).unwrap();
    w.accept(&p, Element::Species(1), f6p).unwrap();
    w.accept(&p, Element::Species(1), g6p).unwrap();
    let Resolution::Known { targets, .. } = w.resolution(&p, Element::Species(1)) else { panic!() };
    assert_eq!(targets.len(), 2);

    // Replacing swaps, in one undoable step.
    let mut w = load(MODEL).unwrap();
    w.accept(&p, Element::Species(1), f6p).unwrap();
    let after_accept = w.to_sbml();
    w.replace_identity(&p, Element::Species(1), g6p).unwrap();
    assert_eq!(
        w.resolution(&p, Element::Species(1)),
        Resolution::Known { targets: vec![g6p], tie: false, exact: true }
    );
    w.undo();
    assert_eq!(w.to_sbml(), after_accept);
}

#[test]
fn replacing_keeps_what_the_pack_does_not_know() {
    let p = pack();
    let mut w = load(MODEL).unwrap();
    let protein = "https://identifiers.org/uniprot:P12345".to_string();
    w.add_terms(Element::Species(0), "is", std::slice::from_ref(&protein)).unwrap();
    w.replace_identity(&p, Element::Species(0), entity(&p, "f6p")).unwrap();
    let uris = w.identity_uris(Element::Species(0));
    assert!(uris.contains(&protein), "{uris:?}");
    // The old ChEBI URN for G6P is gone.
    assert!(!uris.iter().any(|u| u.contains("4170")), "{uris:?}");
    // Other tools' XML stays.
    assert!(w.to_sbml().contains("tool:data"));
}

#[test]
fn changing_a_qualifier_and_reading_sbo() {
    let mut w = load(MODEL).unwrap();
    let el = Element::Species(0);
    let uri = w.identity_uris(el)[0].clone();
    let before = w.to_sbml();
    w.change_qualifier(el, &uri, "is", "isVersionOf").unwrap();
    let terms = w.terms(el);
    assert_eq!(terms.len(), 1);
    assert_eq!((terms[0].qualifier.as_str(), terms[0].uri.as_str()), ("isVersionOf", uri.as_str()));
    // Other annotation content is kept.
    assert!(w.to_sbml().contains("tool:data"));
    // Not there: nothing changes, nothing to undo.
    w.change_qualifier(el, "https://identifiers.org/CHEBI:1", "is", "hasPart").unwrap();
    assert!(w.change_qualifier(el, &uri, "isVersionOf", "isSortOf").is_err());
    assert!(w.undo());
    assert_eq!(w.to_sbml(), before);
    assert!(!w.can_undo());

    assert_eq!(w.sbo(el), None);
    w.set_sbo(el, Some(247)).unwrap();
    assert_eq!(w.sbo(el), Some(247));
    w.set_sbo(el, None).unwrap();
    assert_eq!(w.sbo(el), None);
}

#[test]
fn removing_a_term() {
    let mut w = load(MODEL).unwrap();
    let urn = "urn:miriam:obo.chebi:CHEBI%3A4170";
    w.remove_term(Element::Species(0), "is", urn).unwrap();
    assert!(w.identity_uris(Element::Species(0)).is_empty());
    // The other tool's XML stays.
    assert!(w.to_sbml().contains("tool:data"));
    w.undo();
    assert_eq!(w.identity_uris(Element::Species(0)), [urn]);
}

#[test]
fn refusals_leave_the_model_alone() {
    let mut w = load(MODEL).unwrap();
    let before = w.to_sbml();
    assert_eq!(
        w.add_terms(Element::Species(0), "isTheSameAs", &["x".into()]),
        Err(EditError::UnknownQualifier("isTheSameAs".into()))
    );
    assert_eq!(w.add_terms(Element::Species(9), "is", &["x".into()]), Err(EditError::NoSuchElement));
    assert_eq!(w.to_sbml(), before);
    assert!(!w.can_undo());
}

#[test]
fn rdf_the_reader_does_not_know_survives_an_edit() {
    // A description holding something besides MIRIAM terms is kept whole,
    // as raw XML, and written back beside the new terms.
    let odd = MODEL.replace(
        "<rdf:Description rdf:about=\"#m1\">",
        "<rdf:Description rdf:about=\"#m1\"><rdf:value>not a term</rdf:value>",
    );
    let mut w = load(&odd).unwrap();
    w.add_terms(Element::Species(0), "is", &["https://identifiers.org/CHEBI:1".into()]).unwrap();
    let text = w.to_sbml();
    assert!(text.contains("not a term"), "{text}");
    assert!(text.contains("CHEBI%3A4170") || text.contains("CHEBI:4170"), "{text}");
    let again = load(&text).unwrap();
    assert!(again.identity_uris(Element::Species(0)).contains(&"https://identifiers.org/CHEBI:1".to_string()));
}

#[test]
fn an_rdf_block_the_reader_cannot_read_is_not_rewritten() {
    // rdf:RDF holding something other than descriptions.
    let odd = MODEL.replace(
        "<rdf:Description rdf:about=\"#m1\">",
        "<rdf:Bag/><rdf:Description rdf:about=\"#m1\">",
    );
    let mut w = load(&odd).unwrap();
    let before = w.to_sbml();
    assert_eq!(
        w.add_terms(Element::Species(0), "is", &["https://identifiers.org/CHEBI:1".into()]),
        Err(EditError::UnreadableRdf)
    );
    assert_eq!(w.to_sbml(), before);
}

#[test]
fn antimony_models_load_and_annotate() {
    let p = pack();
    let mut w = load_antimony(
        "model m\n  compartment cell = 1;\n  species G6P in cell = 1, F6P in cell = 0;\n  J1: G6P -> F6P; k*G6P;\n  k = 0.1;\nend",
    )
    .unwrap();
    assert_eq!(w.format(), Format::Antimony);
    assert_eq!(w.engine_model().species.len(), 2);
    w.accept(&p, Element::Species(1), entity(&p, "f6p")).unwrap();
    let again = load(&w.to_sbml()).unwrap();
    assert!(!again.identity_uris(Element::Species(1)).is_empty());
}

#[test]
fn accepting_an_entry_from_another_database() {
    let mut w = load(MODEL).unwrap();
    let before = w.to_sbml();
    let uri = "https://identifiers.org/uniprot:P08237";
    w.accept_uri(Element::Species(1), uri, Some(252)).unwrap();
    assert_eq!(w.identity_uris(Element::Species(1)), [uri]);
    assert!(w.to_sbml().contains("SBO:0000252"));
    // An SBO term already there is kept.
    w.accept_uri(Element::Species(0), "https://identifiers.org/CHEBI:88003", Some(247)).unwrap();
    w.set_sbo(Element::Reaction(0), Some(176)).unwrap();
    w.accept_uri(Element::Reaction(0), "https://identifiers.org/rhea:11816", Some(252)).unwrap();
    assert!(w.to_sbml().contains("SBO:0000176") && !w.to_sbml().contains("sboTerm=\"SBO:0000252\" id=\"J1\""));
    // One undo step each.
    for _ in 0..4 {
        assert!(w.undo());
    }
    assert_eq!(w.to_sbml(), before);
}

#[test]
fn the_model_says_which_organism() {
    let with = |uri: &str| {
        format!(
            r##"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version2/core" level="3" version="2">
  <model metaid="m" id="m">
    <annotation>
      <rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:bqbiol="http://biomodels.net/biology-qualifiers/">
        <rdf:Description rdf:about="#m">
          <bqbiol:hasTaxon><rdf:Bag><rdf:li rdf:resource="{uri}"/></rdf:Bag></bqbiol:hasTaxon>
        </rdf:Description>
      </rdf:RDF>
    </annotation>
  </model>
</sbml>"##
        )
    };
    for uri in [
        "https://identifiers.org/taxonomy:9606",
        "http://identifiers.org/taxonomy/9606",
        "urn:miriam:taxonomy:9606",
        "http://purl.obolibrary.org/obo/NCBITaxon_9606",
    ] {
        assert_eq!(load(&with(uri)).unwrap().model_taxon(), Some(9606), "{uri}");
    }
    assert_eq!(load(MODEL).unwrap().model_taxon(), None);
}
