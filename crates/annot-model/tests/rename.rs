//! Renaming ids: every reference follows, or nothing changes.

use annot_model::{Element, RenameError, load, load_antimony, references_left};

/// s9 is used in a reaction, two rate laws, an assignment rule, an initial
/// assignment and an event.
const ANT: &str = "model m
  compartment cell = 1;
  species S1 in cell = 1, s9 in cell = 0;
  J1: S1 -> s9; k1*S1;
  J2: s9 -> ; k2*s9;
  k1 = 1; k2 = 0.5;
  x := 2*s9;
  y = 3*s9;
  E1: at time > 1: s9 = 0;
end";

fn species_index(w: &annot_model::Workspace, id: &str) -> usize {
    w.engine_model().species.iter().position(|s| s.id == id).unwrap()
}

#[test]
fn every_reference_follows() {
    let mut w = load_antimony(ANT).unwrap();
    let before = w.to_sbml();
    let s9 = species_index(&w, "s9");
    let n = w.rename(Element::Species(s9), "PG3").unwrap();
    // The id, J1's product, J2's reactant, J2's rate law (J1's is k1*S1),
    // the rule, the initial assignment, the event assignment.
    assert_eq!(n, 7);
    let text = w.to_sbml();
    assert!(references_left(&text, "s9").is_empty(), "{text}");
    assert!(text.contains("id=\"PG3\""));
    assert!(text.contains("species=\"PG3\""));
    assert!(text.contains("<ci> PG3 </ci>"));
    assert!(text.contains("variable=\"PG3\""), "the event assignment");

    // It reads back, and the engine sees the new id.
    let again = load(&text).unwrap();
    assert!(again.engine_model().species.iter().any(|s| s.id == "PG3"));

    // One undo restores the model exactly; redo renames again.
    assert!(w.undo());
    assert_eq!(w.to_sbml(), before);
    assert!(w.redo());
    assert_eq!(w.to_sbml(), text);
}

#[test]
fn reactions_and_compartments_too() {
    let mut w = load_antimony(ANT).unwrap();
    w.rename(Element::Reaction(0), "HK").unwrap();
    assert_eq!(w.element_id(Element::Reaction(0)), "HK");
    w.rename(Element::Compartment(0), "cytosol").unwrap();
    let text = w.to_sbml();
    assert!(text.contains("compartment=\"cytosol\""));
    assert!(references_left(&text, "cell").is_empty(), "{text}");
}

#[test]
fn bad_ids_are_refused_and_nothing_changes() {
    let mut w = load_antimony(ANT).unwrap();
    let before = w.to_sbml();
    let s9 = Element::Species(species_index(&w, "s9"));
    assert_eq!(w.rename(s9, "3pg"), Err(RenameError::InvalidId("3pg".into())));
    assert_eq!(w.rename(s9, "S1"), Err(RenameError::Taken("S1".into())));
    assert_eq!(w.rename(s9, "k1"), Err(RenameError::Taken("k1".into())), "parameters count");
    assert_eq!(w.rename(s9, "s9"), Err(RenameError::Unchanged));
    assert_eq!(w.rename(Element::Model, "m2"), Err(RenameError::NotRenamable));
    assert_eq!(w.to_sbml(), before);
    assert!(!w.can_undo());
}

/// A reaction whose rate law has a local parameter "k".
const LOCAL: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" level="3" version="1">
  <model id="m">
    <listOfCompartments><compartment id="cell" size="1" constant="true" spatialDimensions="3"/></listOfCompartments>
    <listOfSpecies>
      <species id="A" compartment="cell" initialConcentration="1" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
    </listOfSpecies>
    <listOfReactions>
      <reaction id="J1" reversible="false">
        <listOfReactants><speciesReference species="A" stoichiometry="1" constant="true"/></listOfReactants>
        <kineticLaw>
          <math xmlns="http://www.w3.org/1998/Math/MathML"><apply><times/><ci> k </ci><ci> A </ci></apply></math>
          <listOfLocalParameters><localParameter id="k" value="1"/></listOfLocalParameters>
        </kineticLaw>
      </reaction>
    </listOfReactions>
  </model>
</sbml>"#;

#[test]
fn a_local_parameter_with_the_new_name_is_refused() {
    let mut w = load(LOCAL).unwrap();
    // "A" -> "k" would make the rate law's "k * A" read "k * k".
    let r = w.rename(Element::Species(0), "k");
    assert!(matches!(r, Err(RenameError::Taken(_)) | Err(RenameError::LocalParameter { .. })), "{r:?}");
}

/// The FBC package's flux objective refers to reaction J1 from XML this
/// library keeps raw.
const FBC: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<sbml xmlns="http://www.sbml.org/sbml/level3/version1/core" xmlns:fbc="http://www.sbml.org/sbml/level3/version1/fbc/version2" level="3" version="1" fbc:required="false">
  <model id="m" fbc:strict="true">
    <listOfCompartments><compartment id="cell" size="1" constant="true" spatialDimensions="3"/></listOfCompartments>
    <listOfSpecies>
      <species id="A" compartment="cell" initialConcentration="1" hasOnlySubstanceUnits="false" boundaryCondition="false" constant="false"/>
    </listOfSpecies>
    <listOfReactions>
      <reaction id="J1" reversible="false">
        <listOfReactants><speciesReference species="A" stoichiometry="1" constant="true"/></listOfReactants>
      </reaction>
    </listOfReactions>
    <fbc:listOfObjectives fbc:activeObjective="obj">
      <fbc:objective fbc:id="obj" fbc:type="maximize">
        <fbc:listOfFluxObjectives><fbc:fluxObjective fbc:reaction="J1" fbc:coefficient="1"/></fbc:listOfFluxObjectives>
      </fbc:objective>
    </fbc:listOfObjectives>
  </model>
</sbml>"#;

#[test]
fn references_this_tool_cannot_update_stop_the_rename() {
    let mut w = load(FBC).unwrap();
    let before = w.to_sbml();
    match w.rename(Element::Reaction(0), "PFK") {
        Err(RenameError::Unreachable(places)) => {
            assert!(places.iter().any(|p| p.contains("fbc:reaction")), "{places:?}")
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(w.to_sbml(), before);
    // The species is not referred to from the FBC data, so it can be
    // renamed.
    w.rename(Element::Species(0), "glc").unwrap();
}
