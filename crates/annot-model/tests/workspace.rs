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
