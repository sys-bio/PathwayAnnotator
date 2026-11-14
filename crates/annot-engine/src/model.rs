//! The engine's view of a model: what it is allowed to look at.
//!
//! Deliberately small and free of SBML: names, ids, compartments, and which
//! species each reaction uses. Annotations are not here.

/// A model to annotate.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Model {
    pub compartments: Vec<Compartment>,
    pub species: Vec<Species>,
    pub reactions: Vec<Reaction>,
}

/// A compartment. Its id and name are words species names may carry
/// ("IP3X_Cytosol" in compartment "Cytosol").
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Compartment {
    pub id: String,
    pub name: Option<String>,
}

/// A species: a molecule (or complex, protein, ...) in a compartment.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Species {
    pub id: String,
    pub name: Option<String>,
    /// The compartment's id.
    pub compartment: Option<String>,
}

/// A reaction.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reaction {
    pub id: String,
    pub name: Option<String>,
    pub participants: Vec<Participant>,
}

/// A species' part in a reaction.
#[derive(Clone, Debug, PartialEq)]
pub struct Participant {
    /// Index into [`Model::species`].
    pub species: usize,
    pub role: Role,
    /// Stoichiometry, positive; 1 when the model does not say.
    pub stoichiometry: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    Reactant,
    Product,
    /// An enzyme, activator or inhibitor.
    Modifier,
}
