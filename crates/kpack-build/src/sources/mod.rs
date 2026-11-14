//! One module per data source. Each parses the source's cached files into
//! plain records, then adds them to a [`crate::PackBuilder`].
//!
//! Order matters: BiGG makes the entities and reactions; ChEBI then adds
//! names and ids to BiGG's entities; BioModels last, since it resolves model
//! annotations against all the ids the other two added.

pub mod bigg;
pub mod biomodels;
pub mod chebi;
