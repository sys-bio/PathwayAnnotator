//! The annotator's inference engine.
//!
//! Given a model (its species and reactions, by name and id, and how the
//! reactions connect the species) and a knowledge pack, it suggests what
//! each species and reaction is, ranked, with the evidence for each
//! suggestion.
//!
//! The work is done by providers ([`Provider`]), each of which adds evidence
//! for candidates. Their weights add up to a candidate's score. DESIGN.md
//! section 4 lists the providers planned; the first, [`providers::names`],
//! looks names and ids up in the pack's dictionary.
//!
//! The engine never sees a model's existing annotations. That is what lets
//! the benchmark grade it against them.
//!
//! No I/O; works in WebAssembly.

mod candidates;
mod confidence;
pub mod decode;
mod model;
pub mod providers;

pub use candidates::{Candidate, Evidence, Suggestions};
pub use confidence::{Basis, Confidence, basis, confidence};
pub use model::{Compartment, Model, Participant, Reaction, Role, Species};

use kpack::Pack;

/// One source of evidence.
pub trait Provider {
    /// A short name, shown with its evidence.
    fn name(&self) -> &'static str;
    /// Adds this provider's evidence to `out`.
    fn run(&self, pack: &Pack, model: &Model, out: &mut Suggestions);
}

/// The providers, in the order they run. Order matters: the decoder only
/// works on elements the names provider found nothing for, and context
/// builds on what both found.
pub fn providers() -> Vec<Box<dyn Provider>> {
    vec![
        Box::new(providers::names::Names),
        Box::new(providers::decoder::Decoder),
        Box::new(providers::context::Context),
    ]
}

/// Suggestions for every species and reaction in `model`, best first.
pub fn annotate(pack: &Pack, model: &Model) -> Suggestions {
    let mut out = Suggestions::for_model(model);
    for p in providers() {
        p.run(pack, model, &mut out);
    }
    out.rank();
    out
}
